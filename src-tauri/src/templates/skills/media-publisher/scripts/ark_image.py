#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Polaris GEO · 火山方舟 Seedream 生图 CLI
========================================

用「豆包·Seedream」文生图给自媒体稿件配图/出封面。走火山方舟（Ark）OpenAI 兼容图像接口：
`POST {base}/images/generations`，`response_format=b64_json`，把返回的 base64 落盘成 PNG。

密钥读取顺序：
  1) ~/PolarisGEO/data/ark.json 里的 api_key / base_url / image_model（用户在设置里可改）
  2) 缺文件/缺字段时回退到内置默认（粉丝福利 key，随时可能限速，够冒烟）

base_url 兼容两种写法：
  - 已含 `/api/v3`（默认就是）→ 直接拼 `/images/generations`
  - 只到域名根 → 自动补 `/api/v3/images/generations`

模型名从配置读，默认 doubao-seedream-4-5。若接口回报「模型不存在」，自动 GET /models
捞一遍 seedream 系列名：优先挑「配置名的带版本号变体」（如 doubao-seedream-4-5 →
doubao-seedream-4-5-251128）自动重试一次，并打印提示方便固化到 ark.json。

用法：
  python ark_image.py --prompt "赛博朋克风格的游戏封面插画" --out C:\\path\\cover.png --size 1024x1024
  python ark_image.py --prompt P --out out.png          # size 缺省 2048x2048
"""

import argparse
import base64
import json
import os
import sys
import time

try:
    import requests  # workspace 已有；本机确认可用
except Exception:  # pragma: no cover
    requests = None

# ───────────────────────── 默认配置（无 ark.json 时用）─────────────────────────
# 粉丝福利默认 key 不写进源码(本仓 public,明文 key 会被扫描器抓走)。正常路径是桌面端
# 用编译期注入的 key 播种 ~/PolarisGEO/data/ark.json,本脚本从那里读;脱离桌面端单跑时
# 可用环境变量 ARK_API_KEY 兜底。
DEFAULT_API_KEY = os.environ.get("ARK_API_KEY", "")
DEFAULT_BASE_URL = "https://ark.cn-beijing.volces.com/api/v3"
DEFAULT_IMAGE_MODEL = "doubao-seedream-4-5"


def _log(step, ok=True, **extra):
    """每步一行 JSON 进度，便于上游解析。"""
    rec = {"step": step, "ok": ok}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


def _config_path():
    return os.path.join(os.path.expanduser("~"), "PolarisGEO", "data", "ark.json")


def _load_config():
    """读 ~/PolarisGEO/data/ark.json，缺失字段用默认补齐。"""
    api_key = DEFAULT_API_KEY
    base_url = DEFAULT_BASE_URL
    image_model = DEFAULT_IMAGE_MODEL
    path = _config_path()
    if os.path.isfile(path):
        try:
            with open(path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
            api_key = (cfg.get("apiKey") or cfg.get("api_key") or api_key).strip()
            base_url = (cfg.get("baseUrl") or cfg.get("base_url") or base_url).strip()
            image_model = (cfg.get("imageModel") or cfg.get("image_model") or image_model).strip()
            _log("config_loaded", path=path, model=image_model)
        except Exception as e:
            _log("config_read_failed", ok=False, error=str(e), fallback="内置默认")
    else:
        _log("config_default", note="未找到 ark.json，用内置默认 key")
    return api_key, base_url, image_model


def _endpoint(base_url, path):
    """base_url 已含 /api/v3 就直接拼 path；否则补 /api/v3。"""
    base = base_url.rstrip("/")
    if base.endswith("/api/v3"):
        return base + path
    return base + "/api/v3" + path


def _list_seedream_models(base_url, api_key):
    """GET /models 找 seedream 系列名（模型报错时给用户提示用）。"""
    try:
        url = _endpoint(base_url, "/models")
        r = requests.get(url, headers={"Authorization": "Bearer " + api_key}, timeout=30)
        if r.status_code != 200:
            return []
        data = r.json()
        items = data.get("data", data if isinstance(data, list) else [])
        names = []
        for it in items:
            mid = it.get("id") if isinstance(it, dict) else str(it)
            if mid and "seedream" in str(mid).lower():
                names.append(mid)
        return names
    except Exception:
        return []


def _candidate_order(configured, names):
    """把 /models 里的 seedream 系列排成重试顺序：
    ① 配置名的带版本号变体（doubao-seedream-4-5 → doubao-seedream-4-5-251128），新在前；
    ② 其余 seedream，按名字倒序（版本号后缀是日期，字典序大 = 新）。
    存在但没开通（ModelNotOpen）的会挨个试下去，直到撞上账号已开通的那个。"""
    if not names:
        return []
    low = configured.lower()
    prefixed = sorted([n for n in names if str(n).lower().startswith(low + "-")], reverse=True)
    rest = sorted([n for n in names if n not in prefixed and str(n).lower() != low], reverse=True)
    return prefixed + rest


MINIMAX_FALLBACK_KEY = (
    "sk-cp-Ef0R4jwN3gfdb36oKiziix6rs69PaSzBB4Ruow-MTomT6xtl0KLbC6SGcFboB4Zq"
    "-lXYlKf0gaHcqYTVGGyE-MLhzJu2uzzkm8G-gncwYxBFdpJJXm-eKfY"
)


def _minimax_key():
    """MiniMax key 发现顺序：env → 供应商坞 providers.json → 内置粉丝福利。"""
    k = os.environ.get("MINIMAX_API_KEY", "").strip()
    if k:
        return k
    for name in ("PolarisGEO", "Polaris"):
        p = os.path.join(os.path.expanduser("~"), name, "data", "providers.json")
        try:
            with open(p, "r", encoding="utf-8") as f:
                data = json.load(f)
            # providers.json 的真实顶层键是 "items"（见 provider/store.rs 的 Store 结构）；
            # 兼容 "providers" 与顶层直接是列表两种历史/异常写法。
            if isinstance(data, list):
                items = data
            else:
                items = data.get("items") or data.get("providers") or []
            for it in items:
                if isinstance(it, dict) and "minimax" in str(it.get("id", "")).lower():
                    sc = it.get("settings_config") or it.get("settingsConfig") or {}
                    env = sc.get("env", {}) if isinstance(sc, dict) else {}
                    tok = env.get("ANTHROPIC_AUTH_TOKEN") or env.get("ANTHROPIC_API_KEY") or ""
                    if str(tok).strip():
                        return str(tok).strip()
        except Exception:
            continue
    return MINIMAX_FALLBACK_KEY


def _size_to_ratio(size):
    """WxH → MiniMax aspect_ratio（就近映射）。"""
    try:
        w, h = [int(x) for x in str(size).lower().split("x")[:2]]
        r = w / float(h)
    except Exception:
        return "1:1"
    table = [("1:1", 1.0), ("16:9", 16 / 9.0), ("9:16", 9 / 16.0), ("4:3", 4 / 3.0),
             ("3:4", 3 / 4.0), ("3:2", 1.5), ("2:3", 2 / 3.0), ("21:9", 21 / 9.0)]
    return min(table, key=lambda t: abs(t[1] - r))[0]


def _minimax_fallback(prompt, out_path, size):
    """Ark 生图不可用（账号未开通模型等）时回退 MiniMax image-01（粉丝福利 key 实测可出图）。
    成功返回 0，失败返回 None（让上游继续走原失败出口）。"""
    key = _minimax_key()
    _log("minimax_fallback", note="Ark 生图不可用，回退 MiniMax image-01")
    try:
        payload = {"model": "image-01", "prompt": prompt,
                   "aspect_ratio": _size_to_ratio(size), "response_format": "url", "n": 1}
        t0 = time.time()
        r = requests.post("https://api.minimaxi.com/v1/image_generation",
                          headers={"Authorization": "Bearer " + key,
                                   "Content-Type": "application/json"},
                          json=payload, timeout=120)
        latency = int((time.time() - t0) * 1000)
        data = r.json()
        urls = (data.get("data") or {}).get("image_urls") or []
        if r.status_code != 200 or not urls:
            _log("minimax_failed", ok=False, status=r.status_code, body=str(data)[:300])
            return None
        raw = requests.get(urls[0], timeout=120).content
        out_abs = os.path.abspath(out_path)
        os.makedirs(os.path.dirname(out_abs) or ".", exist_ok=True)
        with open(out_abs, "wb") as f:
            f.write(raw)
        _log("saved", path=out_abs, bytes=len(raw), latency_ms=latency, backend="minimax")
        print(json.dumps({"result": "ok", "path": out_abs, "model": "minimax/image-01",
                          "size": size, "latency_ms": latency}, ensure_ascii=False))
        return 0
    except Exception as e:
        _log("minimax_error", ok=False, error=str(e))
        return None


def generate(prompt, out_path, size):
    if requests is None:
        _log("no_requests", ok=False)
        print(json.dumps({"result": "failed", "detail": "缺少 requests 库"}, ensure_ascii=False))
        return 2

    api_key, base_url, image_model = _load_config()
    url = _endpoint(base_url, "/images/generations")
    headers = {
        "Authorization": "Bearer " + api_key,
        "Content-Type": "application/json",
    }

    def _post(model):
        payload = {
            "model": model,
            "prompt": prompt,
            "size": size,
            "response_format": "b64_json",
            "n": 1,
        }
        _log("request", url=url, model=model, size=size)
        t0 = time.time()
        resp = requests.post(url, headers=headers, json=payload, timeout=120)
        return resp, int((time.time() - t0) * 1000)

    try:
        r, latency = _post(image_model)
    except Exception as e:
        _log("request_error", ok=False, error=str(e))
        if _minimax_fallback(prompt, out_path, size) == 0:
            return 0
        print(json.dumps({"result": "failed", "detail": "网络请求失败: %s" % e}, ensure_ascii=False))
        return 1

    # 模型不存在/未开通 → GET /models 捞 seedream 系列，按优先序挨个重试
    def _model_err(resp):
        if resp.status_code not in (400, 404):
            return False
        try:
            b = resp.text.lower()
        except Exception:
            return False
        return "model" in b and ("not" in b or "exist" in b or "invalid" in b or "activate" in b)

    if _model_err(r):
        names = _list_seedream_models(base_url, api_key)
        candidates = _candidate_order(image_model, names)
        _log("model_hint", ok=False, current=image_model,
             available_seedream=names, retry_order=candidates)
        for variant in candidates:
            if variant == image_model:
                continue
            try:
                r, latency = _post(variant)
            except Exception as e:
                _log("retry_error", ok=False, model=variant, error=str(e))
                continue
            if r.status_code == 200:
                image_model = variant
                print("[ark] 配置模型不可用，已自动改用 %s——建议把它写进 "
                      "~/PolarisGEO/data/ark.json 的 image_model 固化。" % variant, flush=True)
                break
            if not _model_err(r):
                break  # 非模型问题（限速/鉴权等），别再空转

    if r.status_code != 200:
        body = ""
        try:
            body = r.text[:800]
        except Exception:
            pass
        _log("http_error", ok=False, status=r.status_code, body=body)
        low = body.lower()
        if r.status_code in (400, 404) and ("model" in low and ("not" in low or "exist" in low or "invalid" in low)):
            names = _list_seedream_models(base_url, api_key)
            detail = "模型 %s 可能不存在。可用 seedream 系列: %s（改 ark.json 的 image_model）" % (
                image_model, ", ".join(names) if names else "（GET /models 未返回 seedream 系列）")
        elif r.status_code in (401, 403):
            detail = "密钥无效或无权限（%d）" % r.status_code
        elif r.status_code == 429:
            detail = "已连通但被限速（429），稍后重试或换自己的 key"
        else:
            detail = "HTTP %d: %s" % (r.status_code, body[:200])
        # Ark 走不通（未开通模型/限速/密钥问题）→ 回退 MiniMax image-01（福利 key 实测可用）
        if _minimax_fallback(prompt, out_path, size) == 0:
            return 0
        print(json.dumps({"result": "failed", "detail": detail}, ensure_ascii=False))
        return 1

    # 解析 b64_json
    try:
        data = r.json()
        item = (data.get("data") or [{}])[0]
        b64 = item.get("b64_json")
        if not b64:
            # 少数情况下只给 url，退而求其次
            img_url = item.get("url")
            if img_url:
                ir = requests.get(img_url, timeout=120)
                raw = ir.content
            else:
                raise ValueError("响应里既无 b64_json 也无 url")
        else:
            raw = base64.b64decode(b64)
    except Exception as e:
        _log("parse_error", ok=False, error=str(e))
        print(json.dumps({"result": "failed", "detail": "解析响应失败: %s" % e}, ensure_ascii=False))
        return 1

    # 落盘
    try:
        out_path = os.path.abspath(out_path)
        os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
        with open(out_path, "wb") as f:
            f.write(raw)
    except Exception as e:
        _log("write_error", ok=False, error=str(e))
        print(json.dumps({"result": "failed", "detail": "写文件失败: %s" % e}, ensure_ascii=False))
        return 1

    _log("saved", path=out_path, bytes=len(raw), latency_ms=latency)
    print(json.dumps({"result": "ok", "path": out_path, "model": image_model,
                      "size": size, "latency_ms": latency}, ensure_ascii=False))
    return 0


def main():
    ap = argparse.ArgumentParser(description="火山方舟 Seedream 文生图 CLI")
    ap.add_argument("--prompt", required=True, help="图像描述（中文/英文均可）")
    ap.add_argument("--out", required=True, help="输出 PNG 绝对路径")
    ap.add_argument("--size", default="2048x2048", help="尺寸，如 1024x1024 / 2048x2048（缺省 2048x2048）")
    args = ap.parse_args()

    # Windows 控制台 UTF-8
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

    return generate(args.prompt, args.out, args.size)


if __name__ == "__main__":
    sys.exit(main())
