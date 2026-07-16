#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Polaris GEO · 采集备份 CLI collect_backup.py
==================================================

data_store 的命令行外壳，给人/上游 agent 手动管备份用。Windows / UTF-8 可跑。

用法：
  python collect_backup.py backup                 # 立即做一份全量快照，滚动保留最近 30 份
  python collect_backup.py list                   # 列出现有备份（全量 + 当日增量）
  python collect_backup.py list --json            # 机器可读输出
  python collect_backup.py verify                 # 校验 collect 下所有 jsonl 每行可解析并统计条数
  python collect_backup.py restore 20260717-142530   # 从某份备份恢复 collect（恢复前自动安全快照）
  python collect_backup.py restore 2026-07-17     # 也可用当日增量备份的日期恢复

  # 测试/演示：往 collect 塞几条假数据（顺带触发当日增量备份）
  python collect_backup.py demo-save --category metrics --count 3

退出码：0 成功；verify 有坏行返回 2；出错返回 1。
"""

import argparse
import json
import os
import sys

# 允许作为脚本直接跑（同目录导入 data_store）
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import data_store as ds  # noqa: E402


def _fix_utf8():
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass


def _human_size(n):
    if n is None or n < 0:
        return "?"
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024:
            return "%.1f%s" % (n, unit) if unit != "B" else "%dB" % n
        n /= 1024.0
    return "%.1fTB" % n


def cmd_backup(_args):
    path = ds.backup()
    print("[备份] 全量快照已生成: %s" % path)
    kept = ds.list_backups(include_daily=False)
    print("[备份] 现存全量快照 %d 份（滚动上限 %d）。" % (len(kept), ds.MAX_BACKUPS))
    return 0


def cmd_list(args):
    items = ds.list_backups()
    if args.json:
        print(json.dumps(items, ensure_ascii=False, indent=2))
        return 0
    if not items:
        print("[备份] 暂无任何备份。先跑一次 `collect_backup.py backup`。")
        return 0
    print("[备份] 现有备份（新→旧）：")
    print("  %-20s %-6s %-10s %-8s" % ("时间戳", "类型", "大小", "条数"))
    for it in items:
        print("  %-20s %-6s %-10s %-8s" % (
            it["timestamp"], it["kind"], _human_size(it["size"]),
            it["records"] if it["records"] >= 0 else "?"))
    return 0


def cmd_verify(args):
    rep = ds.verify_collect()
    if args.json:
        print(json.dumps(rep, ensure_ascii=False, indent=2))
    else:
        print("[校验] collect 目录: %s" % ds.COLLECT_DIR)
        print("[校验] 文件数=%d  总行数=%d  坏行=%d" % (
            rep["files"], rep["total_lines"], rep["bad_lines"]))
        for d in rep["details"]:
            if "error" in d:
                print("  ! %s  读取失败: %s" % (d.get("file"), d["error"]))
            else:
                flag = "OK " if d["bad"] == 0 else "BAD"
                extra = ("  坏行号: %s" % d["bad_lineno"]) if d["bad"] else ""
                print("  %s %s  (%d 条)%s" % (flag, d["file"], d["ok"], extra))
    return 2 if rep["bad_lines"] > 0 else 0


def cmd_restore(args):
    try:
        src = ds.restore(args.timestamp)
    except FileNotFoundError as e:
        print("[恢复] 失败: %s" % e)
        return 1
    print("[恢复] 已从 %s 恢复到 %s" % (src, ds.COLLECT_DIR))
    print("[恢复] 恢复前已对当前 collect 做安全快照（前缀 pre-restore-）。")
    return 0


def cmd_demo_save(args):
    """演示/测试：往指定类别塞 count 条假记录，验证落盘 + 当日增量备份链路。"""
    import datetime
    for i in range(args.count):
        rec = {
            "platform": "demo",
            "title": "假数据演示记录 #%d" % (i + 1),
            "read_count": 100 * (i + 1),
            "draft_status": "draft_saved",
            "note": "collect_backup.py demo-save 生成",
            "seq": i + 1,
            "ts": datetime.datetime.now().isoformat(timespec="seconds"),
        }
        path = ds.save_record(args.category, rec)
    print("[演示] 已向类别 '%s' 写入 %d 条假数据 → %s" % (args.category, args.count, path))
    print("[演示] 已顺带触发当日增量备份（backups/daily/）。")
    return 0


def build_parser():
    ap = argparse.ArgumentParser(
        description="Polaris 采集数据备份 CLI（backup/list/verify/restore）")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_b = sub.add_parser("backup", help="立即做一份全量快照并滚动清理")
    p_b.set_defaults(func=cmd_backup)

    p_l = sub.add_parser("list", help="列出现有备份")
    p_l.add_argument("--json", action="store_true", help="机器可读 JSON 输出")
    p_l.set_defaults(func=cmd_list)

    p_v = sub.add_parser("verify", help="校验 collect 下所有 jsonl 每行可解析并统计条数")
    p_v.add_argument("--json", action="store_true", help="机器可读 JSON 输出")
    p_v.set_defaults(func=cmd_verify)

    p_r = sub.add_parser("restore", help="从某份备份恢复 collect 目录")
    p_r.add_argument("timestamp", help="全量快照时间戳(YYYYMMDD-HHMMSS)或当日增量日期(YYYY-MM-DD)")
    p_r.set_defaults(func=cmd_restore)

    p_d = sub.add_parser("demo-save", help="写入假数据用于测试/演示落盘链路")
    p_d.add_argument("--category", default="metrics",
                     help="类别: metrics/hot_topics/competitor/probe（缺省 metrics）")
    p_d.add_argument("--count", type=int, default=3, help="写入条数（缺省 3）")
    p_d.set_defaults(func=cmd_demo_save)

    return ap


def main():
    _fix_utf8()
    ap = build_parser()
    args = ap.parse_args()
    try:
        return args.func(args)
    except Exception as e:
        print("[错误] %s" % e, file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
