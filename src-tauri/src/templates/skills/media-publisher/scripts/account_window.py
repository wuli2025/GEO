#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""account_window.py —— 打开某平台的「持久登录浏览器窗口」并常驻。

一句话职责：用一个 **持久化 profile 目录** 打开平台登录/发文页，然后 **一直挂着**，
直到用户自己关掉这个浏览器窗口才退出。登录一次，登录态就永久留在 profile 目录里，
之后所有投递脚本复用同一个 profile，无需重复扫码。

铁律：
  - 绝不自动关闭窗口。用户登录完继续留着，让用户自己点 X 关。
  - 关窗口 = 脚本正常退出（进程结束）。
  - 任何异常都安全退出，不留僵尸进程。

用法：
  python account_window.py --platform zhihu  --target login
  python account_window.py --platform wechat --target draft
  python account_window.py --platform xhs --url https://... --profile-dir C:\\path\\to\\profile

Rust 侧（accounts.rs::media_account_open）会显式带上 --url / --profile-dir（权威值）；
直接命令行手跑时不带也行，脚本用内置平台表兜底。进度以 JSON 行打印，便于日志排查。
"""

import argparse
import json
import os
import sys
import time


def log(step, **kw):
    kw["step"] = step
    try:
        print(json.dumps(kw, ensure_ascii=False), flush=True)
    except Exception:
        print(str(kw), flush=True)


# ────── 本地真实 Chrome 优先（channel=chrome），CloakBrowser 仅作回退 ──────
# 用户要求优先本地浏览器：本地 Chrome 渲染正常、登录态与后续投递复用同一 profile。
# 仅本地 Chrome 起不来才回退 CloakBrowser。POLARIS_BROWSER=cloak 可强制 CloakBrowser。
try:
    from playwright.sync_api import sync_playwright as _sync_pw  # type: ignore
except Exception:
    _sync_pw = None
try:
    from cloakbrowser import launch_persistent_context as _cloak_ctx  # type: ignore
except Exception:
    _cloak_ctx = None
_BACKEND = "local-chrome"


def launch_persistent_context(user_data_dir=".", headless=False, viewport=None, **_):
    global _BACKEND
    vp = viewport or {"width": 1440, "height": 900}
    force_cloak = os.environ.get("POLARIS_BROWSER", "").lower() in ("cloak", "cloakbrowser")
    if not force_cloak and _sync_pw is not None:
        try:
            pw = _sync_pw().start()
            ctx = pw.chromium.launch_persistent_context(
                user_data_dir, headless=headless, channel="chrome", viewport=vp,
                args=["--no-first-run", "--no-default-browser-check"])
            ctx._pw = pw
            _BACKEND = "local-chrome"
            return ctx
        except Exception:
            pass
    if _cloak_ctx is not None:
        _BACKEND = "cloakbrowser"
        return _cloak_ctx(user_data_dir=user_data_dir, headless=headless, viewport=vp)
    if _sync_pw is not None:
        pw = _sync_pw().start()
        ctx = pw.chromium.launch_persistent_context(user_data_dir, headless=headless, viewport=vp)
        ctx._pw = pw
        _BACKEND = "playwright-chromium"
        return ctx
    raise RuntimeError("本地 Chrome / CloakBrowser / playwright 都不可用，请先安装 Google Chrome")


HOME = os.path.expanduser("~")

# 平台表：id -> (login_url, draft_url)。须与 accounts.rs 的 PLATFORMS 保持一致。
PLATFORMS = {
    "wechat": ("https://mp.weixin.qq.com/", "https://mp.weixin.qq.com/"),
    "xhs": ("https://creator.xiaohongshu.com/login", "https://creator.xiaohongshu.com/publish/publish"),
    "zhihu": ("https://www.zhihu.com/signin", "https://zhuanlan.zhihu.com/write"),
    "toutiao": ("https://mp.toutiao.com/auth/page/login", "https://mp.toutiao.com/profile_v4/graphic/publish"),
    "baijia": ("https://baijiahao.baidu.com/builder/theme/bjh/login", "https://baijiahao.baidu.com/builder/rc/edit?type=news"),
    "bilibili": ("https://passport.bilibili.com/login", "https://member.bilibili.com/read/editor/#/new"),
    "douyin": ("https://creator.douyin.com/", "https://creator.douyin.com/creator-micro/content/publish-media/text"),
}


def default_profile_dir(platform):
    """与 accounts.rs 的 profile_candidates 一致的兜底推导。"""
    if platform == "wechat":
        return os.path.join(HOME, ".polaris-mp-profile")
    if platform == "xhs":
        lad = os.environ.get("LOCALAPPDATA", HOME)
        return os.path.join(lad, "Google", "Chrome", "XiaohongshuProfiles", "default")
    return os.path.join(HOME, "PolarisGEO", "browser-profiles", platform)


def open_ctx(profile_dir):
    """打开持久上下文；不同后端对 viewport kwarg 支持不一，失败则退化重试。"""
    try:
        return launch_persistent_context(
            profile_dir, headless=False, viewport={"width": 1280, "height": 860}
        )
    except TypeError:
        return launch_persistent_context(profile_dir, headless=False)


def wait_until_closed(ctx, page):
    """常驻：一直等到用户手动关掉浏览器窗口才返回。"""
    # 首选 close 事件（timeout=0 = 无限等待）
    if page is not None:
        try:
            page.wait_for_event("close", timeout=0)
            return
        except Exception:
            pass  # 事件通道不可用 → 走轮询兜底
    # 轮询兜底：context.pages 变空 / 断开即视为已关闭
    while True:
        try:
            pages = ctx.pages
        except Exception:
            return  # 浏览器已断开
        if not pages:
            return
        time.sleep(1.0)


def main():
    ap = argparse.ArgumentParser(description="打开平台持久登录浏览器窗口并常驻")
    ap.add_argument("--platform", required=True)
    ap.add_argument("--target", default="login", choices=["login", "draft"])
    ap.add_argument("--url", default=None, help="覆盖目标 URL（Rust 侧传权威值）")
    ap.add_argument("--profile-dir", dest="profile_dir", default=None,
                    help="覆盖 profile 目录（Rust 侧传权威值）")
    args = ap.parse_args()

    platform = args.platform
    urls = PLATFORMS.get(platform)
    if args.url:
        url = args.url
    elif urls:
        url = urls[1] if args.target == "draft" else urls[0]
    else:
        log("error", detail=f"未知平台：{platform}")
        sys.exit(2)

    profile_dir = args.profile_dir or default_profile_dir(platform)
    try:
        os.makedirs(profile_dir, exist_ok=True)
    except Exception as e:
        log("error", detail=f"创建 profile 目录失败：{e!r}")
        sys.exit(3)

    log("launch", platform=platform, target=args.target, backend=_BACKEND,
        profile=profile_dir, url=url)

    ctx = None
    try:
        ctx = open_ctx(profile_dir)
        try:
            page = ctx.pages[0] if ctx.pages else ctx.new_page()
        except Exception:
            page = ctx.new_page()
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=60000)
        except Exception as e:
            log("goto_warn", detail=repr(e))
        log("open", ok=True)
        # 挂着不走 —— 直到用户自己关窗口
        wait_until_closed(ctx, page)
    except Exception as e:
        log("error", detail=repr(e))
    finally:
        if ctx is not None:
            pw = getattr(ctx, "_pw", None)
            try:
                ctx.close()
            except Exception:
                pass
            if pw is not None:
                try:
                    pw.stop()
                except Exception:
                    pass
    log("closed", ok=True)


if __name__ == "__main__":
    main()
