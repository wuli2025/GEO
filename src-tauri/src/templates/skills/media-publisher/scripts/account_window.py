#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""account_window.py —— 打开某平台的「持久登录浏览器窗口」并常驻。

一句话职责：用一个 **持久化 profile 目录** 打开平台登录/发文页，窗口**一直留着**，
直到用户自己关掉。登录一次，登录态就永久留在 profile 目录里，之后所有投递脚本复用
同一个 profile，无需重复扫码。

实现要点（2026-07-18 重写，修「点开就没了」）：
  浏览器必须是**独立进程**，不能是本脚本的子进程——脚本一退（或被上游超时硬杀）
  整树连坐，窗口跟着消失。所以默认路径是：命令行 detached 起真实 Chrome
  （Windows 加 CREATE_BREAKAWAY_FROM_JOB 脱离 Job Object），带 --remote-debugging-port，
  然后**本脚本立刻退出**，窗口留在原地。与 wechat_yiban.py / draft_uploader.py 同配方，
  端口也对齐——登录窗口开着时，后续投递脚本直接 CDP 接管同一个窗口。

  重复点「登录」不再失败：Chrome 对同一 user-data-dir 只允许一个实例，第二次带 URL 的
  调用会被转发给已在跑的实例（新开一个标签页）后自行退出——正好是我们想要的「复用窗口」。
  旧版用 playwright 起浏览器，撞上 profile 锁会启动失败，且失败后回退路径本身是坏的
  （前一次 sync_playwright().start() 泄漏，第二次 start() 直接抛
  「Sync API inside the asyncio loop」），于是窗口一闪就没。

铁律：
  - 绝不自动关闭窗口。用户登录完继续留着，让用户自己点 X 关。
  - 任何异常都安全退出，不留僵尸进程；失败原因写进日志文件（Rust 侧丢弃 stdout，
    没有日志就等于「点了没反应」无从排查）。

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
import shutil
import subprocess
import sys
import time
import urllib.request

# Windows 控制台默认 GBK，中文日志会乱码；统一 UTF-8。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

HOME = os.path.expanduser("~")
LOG_PATH = os.path.join(HOME, "PolarisGEO", "runtime", "account_window.log")


def log(step, **kw):
    kw["step"] = step
    kw["ts"] = time.strftime("%Y-%m-%d %H:%M:%S")
    try:
        line = json.dumps(kw, ensure_ascii=False)
    except Exception:
        line = str(kw)
    print(line, flush=True)
    # 落盘：Rust 侧把 stdout 丢进 NUL，出问题时这是唯一线索。
    try:
        os.makedirs(os.path.dirname(LOG_PATH), exist_ok=True)
        with open(LOG_PATH, "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except Exception:
        pass


try:
    from playwright.sync_api import sync_playwright as _sync_pw  # type: ignore
except Exception:
    _sync_pw = None
try:
    from cloakbrowser import launch_persistent_context as _cloak_ctx  # type: ignore
except Exception:
    _cloak_ctx = None


# ────────────────────── CDP 端口（与投递脚本对齐，便于登录窗口被直接接管）──────────────────────
# wechat 用 9222（wechat_yiban.py 的 CDP_PORT）；其余与 draft_uploader.py 的 9330+offset 一致。
CDP_BASE_PORT = int(os.environ.get("POLARIS_MEDIA_CDP_PORT", "9330"))
_CDP_FIXED = {"wechat": int(os.environ.get("POLARIS_MP_CDP_PORT", "9222"))}
_CDP_OFFSET = {"zhihu": 1, "toutiao": 2, "bilibili": 3, "baijia": 4, "douyin": 5,
               "csdn": 6, "juejin": 7, "xhs": 8}


def cdp_port(platform):
    if platform in _CDP_FIXED:
        return _CDP_FIXED[platform]
    return CDP_BASE_PORT + _CDP_OFFSET.get(platform, 9)


def cdp_version(port):
    try:
        with urllib.request.urlopen("http://127.0.0.1:%d/json/version" % port, timeout=2) as r:
            return json.loads(r.read().decode("utf-8", "ignore"))
    except Exception:
        return None


def chrome_exe():
    cands = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        os.path.expanduser(r"~\AppData\Local\Google\Chrome\Application\chrome.exe"),
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome", "/usr/bin/chromium",
    ]
    for c in cands:
        if os.path.isfile(c):
            return c
    return shutil.which("chrome") or shutil.which("google-chrome") or shutil.which("chromium")


def spawn_chrome_detached(exe, profile, port, url):
    """脱离父进程启动 Chrome —— 浏览器必须活过本脚本退出。
    Windows 上工具/CI 常把子进程放进 Job Object，父进程一死整树被杀；
    CREATE_BREAKAWAY_FROM_JOB 显式脱离该 Job，这是「窗口留给用户」能成立的关键。"""
    args = [exe, "--remote-debugging-port=%d" % port, "--user-data-dir=%s" % profile,
            "--no-first-run", "--no-default-browser-check", "--start-maximized", url]
    kw = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if os.name == "nt":
        DETACHED, NEW_GROUP, BREAKAWAY = 0x00000008, 0x00000200, 0x01000000
        try:
            return subprocess.Popen(args, creationflags=DETACHED | NEW_GROUP | BREAKAWAY, **kw)
        except OSError:
            return subprocess.Popen(args, creationflags=DETACHED | NEW_GROUP, **kw)
    return subprocess.Popen(args, start_new_session=True, **kw)


def open_via_detached_chrome(profile_dir, port, url):
    """默认路径：detached 真实 Chrome。返回 'spawned' / 'reused' / None（Chrome 不可用）。

    已有实例在跑同一 profile 时，Chrome 会把 URL 转发给它并让本次调用自行退出——
    这正是我们要的「窗口已开就复用、开新标签页」，不再撞 profile 锁。"""
    exe = chrome_exe()
    if not exe:
        return None
    already = bool(cdp_version(port))
    spawn_chrome_detached(exe, os.path.abspath(profile_dir), port, url)
    if already:
        return "reused"
    # 等 CDP 端口起来，确认窗口真的活了（最多 ~15s）
    for _ in range(30):
        time.sleep(0.5)
        if cdp_version(port):
            return "spawned"
    # 端口没起来不代表窗口没开（用户可能禁用了调试端口），给一次宽限判断
    return "spawned"


# ───────────────────────── 回退路径：playwright（Chrome 不可用时）─────────────────────────
def launch_persistent_context(user_data_dir, viewport=None):
    """回退用。每次失败**必须 stop 掉已 start 的 playwright**——否则泄漏的 asyncio loop
    会让下一次 sync_playwright().start() 抛「Sync API inside the asyncio loop」，
    使整条回退链形同虚设（这正是旧版窗口一闪就没的第二个原因）。"""
    vp = viewport or {"width": 1280, "height": 860}
    force_cloak = os.environ.get("POLARIS_BROWSER", "").lower() in ("cloak", "cloakbrowser")
    if not force_cloak and _sync_pw is not None:
        pw = None
        try:
            pw = _sync_pw().start()
            ctx = pw.chromium.launch_persistent_context(
                user_data_dir, headless=False, channel="chrome", viewport=vp,
                args=["--no-first-run", "--no-default-browser-check"])
            ctx._pw = pw
            return ctx, "local-chrome"
        except Exception as e:
            log("fallback_chrome_failed", detail=str(e)[:200])
            if pw is not None:
                try:
                    pw.stop()
                except Exception:
                    pass
    if _cloak_ctx is not None:
        try:
            return _cloak_ctx(user_data_dir=user_data_dir, headless=False, viewport=vp), "cloakbrowser"
        except Exception as e:
            log("fallback_cloak_failed", detail=str(e)[:200])
    if _sync_pw is not None:
        pw = None
        try:
            pw = _sync_pw().start()
            ctx = pw.chromium.launch_persistent_context(user_data_dir, headless=False, viewport=vp)
            ctx._pw = pw
            return ctx, "playwright-chromium"
        except Exception as e:
            log("fallback_chromium_failed", detail=str(e)[:200])
            if pw is not None:
                try:
                    pw.stop()
                except Exception:
                    pass
    raise RuntimeError("本地 Chrome / CloakBrowser / playwright 都不可用，请先安装 Google Chrome")


def wait_until_closed(ctx, page):
    """回退路径专用：playwright 起的浏览器是子进程，本脚本必须挂着它才活。"""
    if page is not None:
        try:
            page.wait_for_event("close", timeout=0)
            return
        except Exception:
            pass
    while True:
        try:
            pages = ctx.pages
        except Exception:
            return
        if not pages:
            return
        time.sleep(1.0)


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
        log("error", detail="未知平台：%s" % platform)
        sys.exit(2)

    profile_dir = args.profile_dir or default_profile_dir(platform)
    try:
        os.makedirs(profile_dir, exist_ok=True)
    except Exception as e:
        log("error", detail="创建 profile 目录失败：%r" % (e,))
        sys.exit(3)

    port = cdp_port(platform)
    log("launch", platform=platform, target=args.target, profile=profile_dir, url=url, port=port)

    # ① 默认：detached 真实 Chrome —— 开完就撒手，窗口独立存活
    try:
        how = open_via_detached_chrome(profile_dir, port, url)
    except Exception as e:
        how = None
        log("detached_failed", detail=repr(e))
    if how:
        log("open", ok=True, backend="detached-chrome", how=how, port=port)
        return

    # ② 回退：playwright 起浏览器 —— 它是子进程，本脚本必须挂着不退
    log("detached_unavailable", detail="没找到本地 Chrome，回退 playwright（窗口随脚本存活）")
    ctx = None
    try:
        ctx, backend = launch_persistent_context(profile_dir)
        try:
            page = ctx.pages[0] if ctx.pages else ctx.new_page()
        except Exception:
            page = ctx.new_page()
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=60000)
        except Exception as e:
            log("goto_warn", detail=repr(e))
        log("open", ok=True, backend=backend)
        wait_until_closed(ctx, page)
    except Exception as e:
        log("error", detail=repr(e))
        sys.exit(4)
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
