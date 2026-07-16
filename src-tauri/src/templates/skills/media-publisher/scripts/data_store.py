#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Polaris GEO · 采集数据落盘与备份模块 data_store.py
==================================================

自媒体采集链路（热点雷达 WebSearch/WebFetch、投递脚本顺带采集的阅读数/回执等）过去
「拿到即丢」，没有任何落盘。本模块给整条链路补上**统一落盘 + 滚动备份 + 崩溃可恢复**：

  save_record(category, record)  一条采集记录原子追加进当日 jsonl；写完顺手做「当日增量备份」
  backup()                       把整个 collect 目录快照成带时间戳的 zip，滚动保留最近 30 份
  restore(timestamp)             从某份备份把 collect 目录恢复回来
  list_backups()                 列出现有备份（时间戳 + 大小 + 条数估计）
  fetch_with_retry(fn, ...)      通用重试执行器（配套 @with_retry 装饰器），供采集脚本包住网络请求

目录结构（全部在用户目录下，跨平台 os.path.expanduser("~")）：
  ~/PolarisGEO/data/
    ├─ collect/                          采集数据主库（真身，按类别分目录、按天分文件）
    │    ├─ metrics/2026-07-17.jsonl        投递回执 / 阅读数 / 草稿状态
    │    ├─ hot_topics/2026-07-17.jsonl     热点雷达抓到的热榜 / 爆文
    │    ├─ competitor/2026-07-17.jsonl     对标账号数据
    │    └─ probe/2026-07-17.jsonl          探针 / 其它探测数据
    ├─ backups/
    │    ├─ 20260717-142530.zip             全量快照（backup() 产出，滚动 30 份）
    │    └─ daily/2026-07-17.zip            当日增量备份（save_record 顺手做，同日覆盖同一份）
    └─ logs/data_store.log                  本模块自身日志

设计要点：
  · jsonl 行级追加 + flush + fsync，单行原子；每行一条 JSON，坏一行不毁全表。
  · 全量备份用「临时文件 + 原子替换」，中途被杀不会留半个 zip。
  · 当日增量备份同日只留一份（覆盖），避免每次 save 都堆一份把盘写爆。
  · 一切备份操作 best-effort：失败只记日志、绝不抛给调用方（投递主流程不能被备份拖垮）。
"""

import argparse
import contextlib
import datetime as _dt
import functools
import io
import json
import os
import shutil
import sys
import tempfile
import time
import zipfile

# ─────────────────────────── 路径定式 ───────────────────────────
DATA_ROOT = os.path.join(os.path.expanduser("~"), "PolarisGEO", "data")
COLLECT_DIR = os.path.join(DATA_ROOT, "collect")
BACKUP_DIR = os.path.join(DATA_ROOT, "backups")
DAILY_BACKUP_DIR = os.path.join(BACKUP_DIR, "daily")
LOG_DIR = os.path.join(DATA_ROOT, "logs")
LOG_FILE = os.path.join(LOG_DIR, "data_store.log")

MAX_BACKUPS = 30                       # 全量快照滚动保留份数
TS_FMT = "%Y%m%d-%H%M%S"               # 全量快照时间戳
DAY_FMT = "%Y-%m-%d"                   # 当日 jsonl / 增量备份日期
# 合法类别（软约束：不在表内也放行，只是记一条日志，避免手滑写歪目录名）
KNOWN_CATEGORIES = ("metrics", "hot_topics", "competitor", "probe")


def _now():
    return _dt.datetime.now()


def _ensure_dir(path):
    os.makedirs(path, exist_ok=True)
    return path


def _log(event, **extra):
    """本模块自身日志：best-effort 追加进 logs/data_store.log，顺带 stderr 一行。绝不抛异常。"""
    rec = {"ts": _now().strftime("%Y-%m-%d %H:%M:%S"), "event": event}
    rec.update(extra)
    line = json.dumps(rec, ensure_ascii=False)
    try:
        _ensure_dir(LOG_DIR)
        with open(LOG_FILE, "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except Exception:
        pass
    try:
        sys.stderr.write("[data_store] " + line + "\n")
    except Exception:
        pass


def _safe_category(category):
    """把类别名规整成安全目录名（挡住路径穿越 / 空值）。"""
    cat = (category or "misc").strip().strip("/\\")
    # 只留字母数字下划线连字符，其余替换为下划线，防目录穿越
    cat = "".join(c if (c.isalnum() or c in "_-") else "_" for c in cat)
    return cat or "misc"


# ─────────────────────────── 采集落盘：原子行级追加 ───────────────────────────
def save_record(category, record, do_daily_backup=True):
    """把一条采集记录追加进 ~/PolarisGEO/data/collect/{category}/{YYYY-MM-DD}.jsonl。

    · record 若不是 dict 会被包成 {"value": record}；自动补 _saved_at 时间戳（不覆盖已有）。
    · 行级原子：单行 JSON 一次 write + flush + fsync 落盘，坏一行不毁全表。
    · 写完顺手触发「当日增量备份」（同日覆盖同一份 zip，见 _daily_backup）。
    返回落盘的 jsonl 绝对路径；写失败抛异常（这是采集主动作，调用方应能感知）。
    但当日增量备份失败只记日志、不影响返回（备份失败绝不能影响采集本身）。
    """
    cat = _safe_category(category)
    if category and category not in KNOWN_CATEGORIES:
        _log("unknown_category", category=category, note="不在已知类别表内，仍按名落盘")

    if not isinstance(record, dict):
        record = {"value": record}
    else:
        record = dict(record)  # 复制，别污染调用方对象
    record.setdefault("_saved_at", _now().isoformat(timespec="seconds"))

    day = _now().strftime(DAY_FMT)
    target_dir = _ensure_dir(os.path.join(COLLECT_DIR, cat))
    path = os.path.join(target_dir, day + ".jsonl")

    line = json.dumps(record, ensure_ascii=False) + "\n"
    with open(path, "a", encoding="utf-8") as f:
        f.write(line)
        f.flush()
        with contextlib.suppress(Exception):
            os.fsync(f.fileno())

    if do_daily_backup:
        try:
            _daily_backup(day)
        except Exception as e:
            _log("daily_backup_failed", error=str(e)[:160])

    return path


# ─────────────────────────── 备份：全量快照 + 当日增量 ───────────────────────────
def _zip_dir_atomic(src_dir, dest_zip):
    """把 src_dir 打包成 dest_zip：先写临时 zip 再原子替换，中途被杀不留半个包。
    src_dir 不存在时打一个空包（保证备份文件总是存在，restore 不会因缺文件炸）。"""
    _ensure_dir(os.path.dirname(dest_zip))
    fd, tmp = tempfile.mkstemp(suffix=".zip.tmp", dir=os.path.dirname(dest_zip))
    os.close(fd)
    try:
        with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zf:
            if os.path.isdir(src_dir):
                for root, _dirs, files in os.walk(src_dir):
                    for name in files:
                        full = os.path.join(root, name)
                        arc = os.path.relpath(full, src_dir)
                        try:
                            zf.write(full, arc)
                        except Exception as e:
                            _log("zip_skip_file", file=full, error=str(e)[:120])
        os.replace(tmp, dest_zip)   # 原子替换
    finally:
        with contextlib.suppress(Exception):
            if os.path.exists(tmp):
                os.remove(tmp)
    return dest_zip


def _daily_backup(day=None):
    """当日增量备份：把整个 collect 目录压进 backups/daily/{YYYY-MM-DD}.zip。
    同一天多次调用覆盖同一份（原子替换），避免每次 save 都堆一份把盘写爆。"""
    day = day or _now().strftime(DAY_FMT)
    _ensure_dir(DAILY_BACKUP_DIR)
    dest = os.path.join(DAILY_BACKUP_DIR, day + ".zip")
    _zip_dir_atomic(COLLECT_DIR, dest)
    return dest


def backup():
    """全量快照：把 collect 目录压成 backups/{YYYYMMDD-HHMMSS}.zip，随后滚动保留最近 MAX_BACKUPS 份。
    返回新备份的绝对路径。"""
    _ensure_dir(BACKUP_DIR)
    ts = _now().strftime(TS_FMT)
    dest = os.path.join(BACKUP_DIR, ts + ".zip")
    # 极端情况下同秒二次调用会撞名：加毫秒后缀避重
    if os.path.exists(dest):
        dest = os.path.join(BACKUP_DIR, "%s-%03d.zip" % (ts, _now().microsecond // 1000))
    _zip_dir_atomic(COLLECT_DIR, dest)
    _log("backup_created", path=dest, size=_safe_size(dest))
    _rotate_backups()
    return dest


def _rotate_backups(keep=MAX_BACKUPS):
    """按文件名（时间戳）升序，超出 keep 份的删最旧。只动 backups/ 下的 *.zip，不碰 daily/ 子目录。"""
    zips = _list_backup_files()
    excess = len(zips) - keep
    for path in zips[:max(0, excess)]:
        try:
            os.remove(path)
            _log("backup_rotated_out", path=path)
        except Exception as e:
            _log("backup_rotate_failed", path=path, error=str(e)[:120])


def _list_backup_files():
    """backups/ 目录下的全量快照 zip（不含 daily/），按名升序（= 时间升序）。"""
    if not os.path.isdir(BACKUP_DIR):
        return []
    out = []
    for name in os.listdir(BACKUP_DIR):
        full = os.path.join(BACKUP_DIR, name)
        if os.path.isfile(full) and name.lower().endswith(".zip"):
            out.append(full)
    return sorted(out)


def _safe_size(path):
    try:
        return os.path.getsize(path)
    except Exception:
        return -1


def _count_records_in_zip(zip_path):
    """粗略统计一份备份 zip 里所有 .jsonl 的总行数（= 采集条数），用于 list 展示。失败返回 -1。"""
    try:
        total = 0
        with zipfile.ZipFile(zip_path, "r") as zf:
            for info in zf.infolist():
                if info.filename.lower().endswith(".jsonl"):
                    with zf.open(info) as fp:
                        for line in io.TextIOWrapper(fp, encoding="utf-8"):
                            if line.strip():
                                total += 1
        return total
    except Exception:
        return -1


def list_backups(include_daily=True):
    """列出现有备份。返回 list[dict]，每项含 timestamp/path/size/records/kind。
    kind: 'full'=全量快照, 'daily'=当日增量。按时间倒序（新在前）。"""
    items = []
    for path in _list_backup_files():
        name = os.path.splitext(os.path.basename(path))[0]
        items.append({
            "timestamp": name,
            "path": path,
            "size": _safe_size(path),
            "records": _count_records_in_zip(path),
            "kind": "full",
        })
    if include_daily and os.path.isdir(DAILY_BACKUP_DIR):
        for name in sorted(os.listdir(DAILY_BACKUP_DIR)):
            full = os.path.join(DAILY_BACKUP_DIR, name)
            if os.path.isfile(full) and name.lower().endswith(".zip"):
                items.append({
                    "timestamp": os.path.splitext(name)[0],
                    "path": full,
                    "size": _safe_size(full),
                    "records": _count_records_in_zip(full),
                    "kind": "daily",
                })
    items.sort(key=lambda x: (x["kind"], x["timestamp"]), reverse=True)
    return items


def _find_backup(timestamp):
    """按时间戳定位备份 zip：先找全量快照，再找当日增量。找不到返回 None。
    timestamp 允许带或不带 .zip 后缀。"""
    ts = timestamp.strip()
    if ts.lower().endswith(".zip"):
        ts = ts[:-4]
    for cand in (os.path.join(BACKUP_DIR, ts + ".zip"),
                 os.path.join(DAILY_BACKUP_DIR, ts + ".zip")):
        if os.path.isfile(cand):
            return cand
    return None


def restore(timestamp, target_dir=None, safety_backup=True):
    """从某份备份恢复 collect 目录。

    · timestamp：全量快照时间戳（YYYYMMDD-HHMMSS）或当日增量日期（YYYY-MM-DD）。
    · 恢复前默认先给当前 collect 做一份安全快照（safety_backup=True），别把现场覆盖没了。
    · 恢复策略：把备份 zip 解压覆盖进 collect 目录（同名文件覆盖，不删备份里没有的新文件）。
    返回恢复用的 zip 路径；找不到备份抛 FileNotFoundError。"""
    zip_path = _find_backup(timestamp)
    if not zip_path:
        raise FileNotFoundError("找不到备份: %s（可用 list_backups 查看现有备份）" % timestamp)

    dest = target_dir or COLLECT_DIR
    if safety_backup and os.path.isdir(COLLECT_DIR) and os.listdir(COLLECT_DIR):
        try:
            snap = os.path.join(BACKUP_DIR, "pre-restore-" + _now().strftime(TS_FMT) + ".zip")
            _zip_dir_atomic(COLLECT_DIR, snap)
            _log("pre_restore_snapshot", path=snap)
        except Exception as e:
            _log("pre_restore_snapshot_failed", error=str(e)[:140])

    _ensure_dir(dest)
    with zipfile.ZipFile(zip_path, "r") as zf:
        zf.extractall(dest)
    _log("restored", source=zip_path, target=dest)
    return zip_path


# ─────────────────────────── 采集稳定性：通用重试 ───────────────────────────
def fetch_with_retry(fn, *args, retries=3, backoff=1.5, exceptions=(Exception,),
                     on_retry=None, **kwargs):
    """通用重试执行器：调用 fn(*args, **kwargs)，失败按指数退避重试，最多 retries 次。

    · retries=3 表示「首次 + 最多 3 次重试」= 最多 4 次尝试。
    · backoff：退避基数秒，第 n 次重试前 sleep backoff*2**(n-1)。
    · exceptions：只对这些异常重试，其余立即抛出。
    · on_retry(attempt, error)：可选回调（记指标 / 打日志）。
    全部尝试用尽后抛出最后一次异常。供采集脚本包住 WebFetch / requests 等网络请求。
    """
    attempt = 0
    while True:
        try:
            return fn(*args, **kwargs)
        except exceptions as e:
            attempt += 1
            if attempt > retries:
                _log("retry_exhausted", fn=getattr(fn, "__name__", str(fn)),
                     attempts=attempt, error=str(e)[:160])
                raise
            sleep_s = backoff * (2 ** (attempt - 1))
            _log("retry", fn=getattr(fn, "__name__", str(fn)),
                 attempt=attempt, sleep=round(sleep_s, 2), error=str(e)[:120])
            if on_retry:
                with contextlib.suppress(Exception):
                    on_retry(attempt, e)
            time.sleep(sleep_s)


def with_retry(retries=3, backoff=1.5, exceptions=(Exception,), on_retry=None):
    """@with_retry(...) 装饰器形态：等价于用 fetch_with_retry 包住被装饰函数每次调用。"""
    def deco(fn):
        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            return fetch_with_retry(fn, *args, retries=retries, backoff=backoff,
                                    exceptions=exceptions, on_retry=on_retry, **kwargs)
        return wrapper
    return deco


# ─────────────────────────── jsonl 校验（供 CLI verify 复用）───────────────────────────
def verify_collect():
    """遍历 collect 下所有 *.jsonl，逐行 json.loads 校验。
    返回 dict：files/total_lines/bad_lines/details（每文件 ok 条数、坏行行号）。"""
    report = {"files": 0, "total_lines": 0, "bad_lines": 0, "details": []}
    if not os.path.isdir(COLLECT_DIR):
        return report
    for root, _dirs, files in os.walk(COLLECT_DIR):
        for name in sorted(files):
            if not name.lower().endswith(".jsonl"):
                continue
            path = os.path.join(root, name)
            report["files"] += 1
            ok, bad, bad_lineno = 0, 0, []
            try:
                with open(path, "r", encoding="utf-8") as f:
                    for i, line in enumerate(f, 1):
                        if not line.strip():
                            continue
                        report["total_lines"] += 1
                        try:
                            json.loads(line)
                            ok += 1
                        except Exception:
                            bad += 1
                            report["bad_lines"] += 1
                            if len(bad_lineno) < 20:
                                bad_lineno.append(i)
            except Exception as e:
                report["details"].append({"file": path, "error": str(e)[:140]})
                continue
            report["details"].append({
                "file": os.path.relpath(path, COLLECT_DIR),
                "ok": ok, "bad": bad, "bad_lineno": bad_lineno,
            })
    return report


__all__ = [
    "save_record", "backup", "restore", "list_backups", "verify_collect",
    "fetch_with_retry", "with_retry",
    "DATA_ROOT", "COLLECT_DIR", "BACKUP_DIR", "DAILY_BACKUP_DIR",
]
