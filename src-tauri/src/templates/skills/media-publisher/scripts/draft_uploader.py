#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Polaris GEO · 多平台草稿投递引擎 draft_uploader.py
==================================================

把写好的稿件（标题 + 正文 markdown/html + 可选配图）送进各平台创作者后台的编辑器，
**只存草稿 / 停在编辑页，绝不点发布**。发布键永远留给用户亲手点。

设计与 wechat_yiban.py 同源：
  - 浏览器：**detached 本地 Chrome + CDP 接管**（缺省引擎）——浏览器是独立进程，脚本结束
    （含被上游默认超时硬杀）只断开 CDP 连接，窗口留在原地给用户预览草稿、核对配图、亲手点
    发布，根治「传完自己关窗」；每平台固定调试端口，同平台连投复用已在跑的 Chrome。
    CDP 不可用时回退 playwright(channel=chrome) → CloakBrowser → 自带 chromium；
  - 持久 profile（登录态永久留在 ~/PolarisGEO/browser-profiles/{platform}）；
  - 正文注入走「粘贴通道」：合成 ClipboardEvent + DataTransfer（text/html + text/plain），
    走编辑器（ProseMirror / Draft.js / Quill）自己的 schema 解析与事务模型，内容才真正入档；
    三级降级：paste → execCommand(insertHTML/insertText) → innerText 直写，每级按字数校验；
  - 任何一步失败**降级 manual 而不是崩溃**：打开编辑页 + 标题正文进系统剪贴板，窗口保持打开。

平台适配矩阵（PLATFORMS dict，改版只动选择器）：
  zhihu    full     zhuanlan.zhihu.com/write（标题 textarea + Draft.js，知乎自动存草稿）
  toutiao  full     mp.toutiao.com 图文编辑器（标题 textarea + ProseMirror，点「存草稿」）
  bilibili full     member.bilibili.com/read/editor（标题 input + Quill .ql-editor，「存草稿」）
  baijia   partial  打开编辑页 + 剪贴板辅助（编辑器在 iframe 里且改版频繁，先手贴）
  douyin   partial  打开图文发布页 + 剪贴板辅助
  wechat   →走现有  提示改用 wechat-md-typesetter 的 wechat_yiban.py（更强：套样式+两段直送）
  xhs      →走现有  提示改用 post-to-xhs 技能（图文/视频全流程）

用法：
  python draft_uploader.py --platform zhihu --title "标题" --content-file a.md
  python draft_uploader.py --platform toutiao --title T --content-file a.md --images c1.png,c2.png
  python draft_uploader.py --platform zhihu --title T --content-file a.md --manual   # 只开页+剪贴板

输出协议：每步一行 JSON 进度 {"step":..,"ok":..}，最终一行
  {"result":"draft_uploaded"|"manual_assist"|"need_login"|"failed","detail":..}
"""

import argparse
import base64
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

# ─────────── 浏览器引擎：CDP detached Chrome 缺省，多级回退 ───────────
# 优先级：① detached 本地 Chrome + CDP 接管（脚本退出不关窗，见下方 CDP 保活段）
#        ② playwright channel=chrome  ③ CloakBrowser  ④ playwright 自带 chromium。
# 本地 Chrome 渲染正常（CloakBrowser 会把某些平台编辑器布局渲染歪、发布键点不准）。
# 可用环境变量 POLARIS_BROWSER=cloak 强制用 CloakBrowser。
try:
    from playwright.sync_api import sync_playwright as _sync_pw  # type: ignore
except Exception:
    _sync_pw = None
try:
    from cloakbrowser import launch_persistent_context as _cloak_launch  # type: ignore
except Exception:
    _cloak_launch = None
# 采集落盘：投递时能拿到草稿状态/回执，顺手落 metrics 备份。导入失败也不影响投递主流程。
try:
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import data_store as _data_store  # type: ignore
except Exception:
    _data_store = None

BROWSER_ENGINE = "local-chrome"

# ─────────── CDP 保活（与 wechat_yiban.py 同配方，2026-07 真机验证）───────────
# 「上传完不要关浏览器、留窗预览」的根：浏览器必须是**独立进程**。playwright 起的浏览器
# 是脚本的子进程，脚本一退（或被上游 2 分钟默认超时硬杀）整树连坐，窗口跟着没——
# 这正是过去「传完自己关窗、预览做不了」的病灶。改为：命令行 detached 起真实 Chrome
# （Windows 上加 CREATE_BREAKAWAY_FROM_JOB 脱离 Job Object，否则 CI/工具壳里父进程一死
# 整 Job 被杀）→ connect_over_cdp 接管 → 收尾只断连不关窗。
# 每平台一个固定调试端口：同平台连投第二篇直接接管已在跑的 Chrome（免重启、免 profile 锁）。
CDP_BASE_PORT = int(os.environ.get("POLARIS_MEDIA_CDP_PORT", "9330"))
_CDP_OFFSET = {"zhihu": 1, "toutiao": 2, "bilibili": 3, "baijia": 4, "douyin": 5,
               "csdn": 6, "juejin": 7}


def _cdp_port(platform):
    return CDP_BASE_PORT + _CDP_OFFSET.get(platform, 9)


def _cdp_version(port):
    try:
        with urllib.request.urlopen("http://127.0.0.1:%d/json/version" % port, timeout=2) as r:
            return json.loads(r.read().decode("utf-8", "ignore"))
    except Exception:
        return None


def _chrome_exe():
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


def _spawn_chrome_detached(exe, profile, port):
    """脱离父进程启动 Chrome —— 浏览器必须活过本脚本退出。
    Windows 上工具/CI 常把子进程放进 Job Object，父进程一死整树被杀；
    CREATE_BREAKAWAY_FROM_JOB 显式脱离该 Job，这是「窗口留给用户」能成立的关键。"""
    args = [exe, "--remote-debugging-port=%d" % port, "--user-data-dir=%s" % profile,
            "--no-first-run", "--no-default-browser-check",
            "--start-maximized", "about:blank"]
    kw = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if os.name == "nt":
        DETACHED, NEW_GROUP, BREAKAWAY = 0x00000008, 0x00000200, 0x01000000
        try:
            return subprocess.Popen(args, creationflags=DETACHED | NEW_GROUP | BREAKAWAY, **kw)
        except OSError:
            return subprocess.Popen(args, creationflags=DETACHED | NEW_GROUP, **kw)
    return subprocess.Popen(args, start_new_session=True, **kw)


def _connect_cdp(platform, profile):
    """detached Chrome + CDP 接管。任何一步不成返回 None，由 open_editor 回退其它引擎。"""
    if _sync_pw is None:
        return None
    exe = _chrome_exe()
    if not exe:
        return None
    port = _cdp_port(platform)
    try:
        if not _cdp_version(port):
            os.makedirs(profile, exist_ok=True)
            _spawn_chrome_detached(exe, os.path.abspath(profile), port)
            for _i in range(30):
                time.sleep(0.5)
                if _cdp_version(port):
                    break
        ver = _cdp_version(port)
        if not ver:
            return None
        pw = _sync_pw().start()
        try:
            browser = pw.chromium.connect_over_cdp("http://127.0.0.1:%d" % port)
        except Exception:
            try:
                pw.stop()
            except Exception:
                pass
            return None
        ctx = browser.contexts[0] if browser.contexts else browser.new_context()
        ctx._pw = pw
        ctx._cdp = True          # 标记：收尾只断连，绝不关窗
        _log("cdp_attached", browser=ver.get("Browser", "?"), port=port)
        return ctx
    except Exception:
        return None


def _launch_local_chrome(user_data_dir, headless):
    """playwright 驱动本地安装的 Google Chrome（channel=chrome）。
    用 no_viewport + --start-maximized 起**真·最大化窗口**：页面视口跟随真实窗口大小，
    消除「固定 1600x1000 视口 vs 更大窗口」的错位死区（表现为鼠标滚不到底、够不着抖音发布栏）。

    失败时**必须 stop 掉已 start 的 playwright**：泄漏的 asyncio loop 会让下一次
    sync_playwright().start() 抛「Sync API inside the asyncio loop」，整条回退链形同虚设。"""
    pw = _sync_pw().start()
    try:
        ctx = pw.chromium.launch_persistent_context(
            user_data_dir, headless=headless, channel="chrome",
            no_viewport=True,
            args=["--no-first-run", "--no-default-browser-check", "--start-maximized"])
    except Exception:
        try:
            pw.stop()
        except Exception:
            pass
        raise
    ctx._pw = pw
    return ctx


def launch_persistent_context(user_data_dir=".", headless=False, humanize=False, **_):
    global BROWSER_ENGINE
    force = os.environ.get("POLARIS_BROWSER", "").lower()
    # 强制 CloakBrowser
    if force in ("cloak", "cloakbrowser") and _cloak_launch is not None:
        BROWSER_ENGINE = "cloakbrowser"
        return _cloak_launch(user_data_dir=user_data_dir, headless=headless, humanize=humanize)
    # 默认：本地 Chrome 优先
    if _sync_pw is not None:
        try:
            ctx = _launch_local_chrome(user_data_dir, headless)
            BROWSER_ENGINE = "local-chrome"
            return ctx
        except Exception as e:
            print("[投递] 本地 Chrome 启动失败(%s)，回退 CloakBrowser。" % str(e)[:60], flush=True)
    # 回退 CloakBrowser（本地卡住/未装 Chrome 时）
    if _cloak_launch is not None:
        BROWSER_ENGINE = "cloakbrowser"
        return _cloak_launch(user_data_dir=user_data_dir, headless=headless, humanize=humanize)
    # 最后回退：playwright 自带 chromium
    if _sync_pw is not None:
        pw = _sync_pw().start()
        ctx = pw.chromium.launch_persistent_context(user_data_dir, headless=headless)
        ctx._pw = pw
        BROWSER_ENGINE = "playwright-chromium"
        return ctx
    raise RuntimeError("本地 Chrome / CloakBrowser / playwright 都不可用，请先安装 Google Chrome 或 pip install playwright cloakbrowser")


def _pick_page(ctx):
    """挑一个可用页：只复用空白页（about:blank）。CDP 接管已在跑的 Chrome 时，
    里面可能还开着用户上一篇草稿的预览页——绝不抢占非空白标签页，另开新页。"""
    try:
        for p in (getattr(ctx, "pages", None) or []):
            try:
                if (p.url or "about:blank") in ("about:blank", ""):
                    return p
            except Exception:
                continue
    except Exception:
        pass
    return ctx.new_page()


def _new_page_goto(ctx, url, timeout_ms=25000, tries=2):
    """取页并导航，超时重试。domcontentloaded 就返回（不等 networkidle，避免国内站长连接吊死）。"""
    page = _pick_page(ctx)
    last = None
    for i in range(tries):
        try:
            page.goto(url, wait_until="domcontentloaded", timeout=timeout_ms)
            return page
        except Exception as e:
            last = e
            _log("goto_retry", ok=False, attempt=i + 1, error=str(e).splitlines()[0][:80])
            time.sleep(2)
    raise last


def open_editor(cfg):
    """启动浏览器 + 打开编辑页，多引擎自动回退：
      cdp（缺省，脚本退出不关窗）→ local playwright(channel=chrome) → CloakBrowser → 自带 chromium。
    本地 Chrome 若在启动/导航阶段崩溃（Target closed）或导航超时 → 自动收掉、换下一个引擎重来，
    彻底免掉「头条本地 Chrome 一启动就退」和「goto 卡满 30s」两类硬伤。返回 (ctx, page)。
    POLARIS_BROWSER=cloak 时直接从 Cloak 起（无保窗能力，收尾会如实说明）。"""
    global BROWSER_ENGINE
    profile = cfg["profile"]
    platform = os.path.basename(profile)
    os.makedirs(profile, exist_ok=True)
    forced_cloak = os.environ.get("POLARIS_BROWSER", "").lower() in ("cloak", "cloakbrowser")
    engines = []
    if not forced_cloak and _sync_pw is not None:
        engines.append("cdp")    # 首选：detached Chrome + CDP，唯一能「脚本退了窗口还在」的引擎
        engines.append("local")
    if _cloak_launch is not None:
        engines.append("cloak")
    if _sync_pw is not None:
        engines.append("pw")  # 最后兜底：playwright 自带 chromium
    last = None
    for eng in engines:
        ctx = None
        try:
            if eng == "cdp":
                ctx = _connect_cdp(platform, profile)
                if ctx is None:
                    raise RuntimeError("CDP 接管不可用（无 Chrome 或调试端口起不来）")
                BROWSER_ENGINE = "cdp-chrome"
            elif eng == "local":
                ctx = _launch_local_chrome(profile, False)
                BROWSER_ENGINE = "local-chrome"
            elif eng == "cloak":
                ctx = _cloak_launch(user_data_dir=profile, headless=False, humanize=True)
                BROWSER_ENGINE = "cloakbrowser"
            else:
                pw = _sync_pw().start()
                ctx = pw.chromium.launch_persistent_context(profile, headless=False)
                ctx._pw = pw
                BROWSER_ENGINE = "playwright-chromium"
            _log("browser_launched", engine=BROWSER_ENGINE, profile=profile)
            page = _new_page_goto(ctx, cfg["draft_url"], cfg.get("goto_timeout", 25000))
            _log("page_opened", url=page.url, engine=BROWSER_ENGINE)
            ctx._polaris_page = page   # 记住本次投递开的页：--close-after 只收它，不动用户其它标签
            return ctx, page
        except Exception as e:
            last = e
            _log("engine_fallback", ok=False, engine=eng, error=str(e).splitlines()[0][:90])
            try:
                if ctx:
                    if getattr(ctx, "_cdp", False):
                        # CDP 引擎失败：只断连。Chrome 是独立进程，留着给用户/下次接管；
                        # 若真起不来，后续引擎会因 profile 被占而继续回退，属可接受的罕见路径。
                        pw = getattr(ctx, "_pw", None)
                        if pw:
                            pw.stop()
                    else:
                        ctx.close()
                        pw = getattr(ctx, "_pw", None)
                        if pw:
                            pw.stop()
            except Exception:
                pass
            time.sleep(1.5)  # 等 profile 锁释放，下个引擎才能用
    raise last or RuntimeError("open_editor：所有浏览器引擎均失败")


# ───────────────────────── 平台适配器（后台改版只改这里）─────────────────────────
def _profile(platform):
    return os.path.join(os.path.expanduser("~"), "PolarisGEO", "browser-profiles", platform)


PLATFORMS = {
    "zhihu": {
        "name": "知乎",
        "status": "full",
        "draft_url": "https://zhuanlan.zhihu.com/write",
        "profile": _profile("zhihu"),
        # URL 被重定向到这些 pattern = 未登录
        "login_url_patterns": ["signin", "login", "account"],
        # 页面上出现这些 = 登录组件挡路
        "login_selectors": [".SignFlow", ".signQr", ".Login-content", "div[role=dialog] .Modal-content .SignContainer"],
        "title_selectors": [
            ".WriteIndex-titleInput textarea",
            "textarea[placeholder*='请输入标题']",
            "textarea[placeholder*='标题']",
        ],
        "editor_selectors": [
            ".DraftEditor-root .public-DraftEditor-content",
            ".DraftEditor-root [contenteditable=true]",
            ".Editable-content [contenteditable=true]",
        ],
        # 知乎写文章页自动存草稿（顶部显示「草稿已自动保存」），不需要点按钮
        "save_selectors": [],
        "auto_save": True,
        "save_ok_selectors": ["*:has-text('已自动保存')", "*:has-text('草稿已保存')"],
    },
    "toutiao": {
        "name": "今日头条",
        "status": "full",
        "draft_url": "https://mp.toutiao.com/profile_v4/graphic/publish",
        "profile": _profile("toutiao"),
        "login_url_patterns": ["auth/page/login", "sso.toutiao.com", "/login"],
        "login_selectors": [".web-login", ".sso_login", "div[class*='login-panel']", "#SSO_LOGIN"],
        "title_selectors": [
            ".editor-title input",
            "div.editor-title textarea",
            "textarea[placeholder*='请输入文章标题']",
            "textarea[placeholder*='标题']",
            "input[placeholder*='标题']",
        ],
        "editor_selectors": [
            ".ProseMirror[contenteditable=true]",
            "div.ProseMirror",
            ".syl-editor [contenteditable=true]",
        ],
        # 2026-07-14 二次校准：头条新版编辑器已去掉「存草稿」按钮，改为**页脚自动保存**
        # （span.footer-tip-save：编辑即「草稿保存中...」→settle 成「草稿已保存」）。故按 auto_save
        # 处理——只等页脚回执，绝不再回退 Ctrl+S（那会弹浏览器保存框、且根本不存草稿）。
        # save_selectors 保留仅为让收尾文案判定「有草稿箱」，auto_save 分支会先短路、不真正点它。
        "save_selectors": ["span.footer-tip-save"],
        "auto_save": True,
        # 头条页脚长期显示「草稿保存中...」（实测 20s+ 不 settle 成「已保存」），这是它一直在
        # 自动持久化到草稿箱的常态指示——其存在即证明内容已被编辑器接管并写草稿，故兜底接受它。
        "save_ok_selectors": [
            "span.footer-tip-save:has-text('已保存')",
            "*:has-text('草稿已保存')",
            "span.footer-tip-save",
        ],
    },
    "bilibili": {
        # 2026-07-14 深挖结论：B站专栏编辑器正在迁移且对自动化不友好——
        #   · 旧 #/new 已下线，页面空白（SPA 不挂载任何元素）
        #   · 新 #/web 弹「旧版编辑器已停止使用」模态，标题 textarea + 正文 div.ql-editor 虽在 DOM 但
        #     全部 visibility:false，点「前往」按钮 same-page 不跳转、编辑器始终不可见（疑似反自动化隐藏）
        # 本地 Chrome 和 CloakBrowser 均如此。标准/穿透 locator 都点不到可见编辑器 → 维持 partial：
        # 打开编辑页 + 标题正文进剪贴板，人工 Ctrl+V。待 B站迁移稳定或另寻 CDP/坐标方案。
        "name": "B站专栏",
        "status": "partial",
        "draft_url": "https://member.bilibili.com/read/editor/#/web",
        "profile": _profile("bilibili"),
        "login_url_patterns": ["passport.bilibili.com/login", "passport.bilibili.com"],
        "login_selectors": [".login-scan-box", ".login-pwd-wp", "div.bili-mini-mask"],
        "title_selectors": [
            "input[placeholder*='请输入标题']",
            ".article-title input",
            "textarea[placeholder*='标题']",
            "input[placeholder*='标题']",
        ],
        "editor_selectors": [
            ".ql-editor",
            "div[contenteditable=true].ql-editor",
        ],
        "save_selectors": [
            "*:text-is('存草稿')",
            "button:has-text('存草稿')",
            "span:has-text('存草稿')",
            "*:has-text('保存草稿')",
        ],
        "auto_save": False,
        "save_ok_selectors": ["*:has-text('保存成功')", "*:has-text('已保存')", "*:has-text('保存于')"],
    },
    "baijia": {
        # 2026-07-14 真机 DOM 探测重校准：百家号已换 React 新编辑器（FeEditorApp）——
        #   · 标题 = 主框架里唯一的 div[contenteditable=true]（不是 input/textarea！旧配置在这里全落空）
        #   · 正文 = url 为空的子 iframe 里的 <body class="view news-editor-pc" contenteditable>
        #   · 封面 = #bjhNewsCover 区块：先点「单图」radio → 点「选择封面」弹窗 → input[accept*=image] → 「确定」
        # 标题选择器只用 div[ce]（iframe 里的正文是 body 标签，不会被 div[ce] 误命中）；
        # 正文选择器去掉松散的 [contenteditable=true] 兜底，避免把正文误灌进标题 div。
        "name": "百家号",
        "status": "full",
        "draft_url": "https://baijiahao.baidu.com/builder/rc/edit?type=news",
        "profile": _profile("baijia"),
        "login_url_patterns": ["builder/theme/bjh/login", "passport.baidu.com", "/login"],
        "login_selectors": ["#passport-login-pop", ".pass-login-pop", ".tang-pass-qrcode"],
        "title_selectors": ["div[contenteditable=true]",
                            "textarea[placeholder*='标题']", "input[placeholder*='标题']"],
        "editor_selectors": ["body.view.news-editor-pc", "body.view", "body[contenteditable=true]",
                            "body.edui-body-container"],
        "editor_wait": 8,  # 正文 iframe(url 空)渲染慢，正文注入前多等几秒
        "save_selectors": ["button:has-text('存草稿')", "*:text-is('存草稿')", "*:has-text('存草稿')", "*:has-text('保存草稿')"],
        "auto_save": False,
        "save_ok_selectors": ["*:has-text('已保存')", "*:has-text('保存成功')", "*:has-text('保存于')"],
        # 封面流程（2026-07-14 二次真机重校准）：封面区在正文下方，需先滚动到「设置封面」；
        #   单图默认选中 → 悬浮封面缩略图出现「更换」→ 点开图片弹窗（tab「正文/本地上传」默认激活，
        #   有「点击本地上传」区 + input[accept=image/*]，自动裁 3:2）→ set_input_files → 「确定」。
        # 关键教训：绝不回退 input[type=file]（那是页面的视频上传框，喂图会弹"视频格式不正确"）；
        #          open 必须点「更换」，旧的「选择封面」文字不存在，导致整条流程从未真正打开弹窗。
        "cover": {
            "scroll_into": ["label:has-text('设置封面')", "*:text-is('设置封面')"],
            "pre_click": ["label.cheetah-radio-wrapper:has-text('单图')"],
            "hover": ["span.form-item-cover", "div.cheetah-form-item:has-text('设置封面')"],  # 悬浮露出「更换」overlay
            # 封面两种态：①空封面→框里直接是「选择封面」；②已有封面→hover 缩略图冒出「更换」。两者都点，
            # 靠 set_cover 的「验证弹窗真开」筛出有效那个（点中不开弹窗=假成功，会被 _modal_open 挡掉）。
            "open": ["text=选择封面", "text=更换", "text=编辑"],
            "post_open_click": ["text=点击本地上传"],  # 激活本地上传 tab（若 input 未现）
            "file_input": ["input[accept*='image']"],  # 只认图片 input，绝不 input[type=file]
            "confirm": ["div.cheetah-modal-footer button.cheetah-btn-primary",
                        "button.cheetah-btn-primary:has-text('确定')",
                        "button.cheetah-btn-primary:has-text('确认')"],
            "upload_wait": 3.5,
        },
    },
    "douyin": {
        # 2026-07 实测：抖音图文标题 input.semi-input[placeholder=添加作品标题]，正文
        # div.editor-kit-container[contenteditable=true]（placeholder=添加作品描述）→ 可全自动填充。
        # 但页面没有「存草稿」按钮（只有发布/保存权限），故只填充不保存、更不发布，留人工核对。
        "name": "抖音图文",
        "status": "full",
        "draft_url": "https://creator.douyin.com/creator-micro/content/publish-media/text",
        "profile": _profile("douyin"),
        "login_url_patterns": ["creator.douyin.com/login", "/passport/", "sso.douyin.com"],
        "login_selectors": [".login-pannel", "div[class*='qrcode']", "img[src*='qrcode']"],
        # 2026-07-14 真机 DOM 校准：标题 input.semi-input(添加作品标题)、正文 div.editor-kit-container[ce]、
        # 封面独立区块 div.content-upload-kVVDpn「选择一张图片作为封面」。
        "title_selectors": ["input.semi-input[placeholder*='标题']", "input[placeholder*='标题']", "input.semi-input"],
        "editor_selectors": ["div.zone-container.editor-kit-container[contenteditable=true]",
                            "div.editor-kit-container[contenteditable=true]",
                            "div[contenteditable=true][data-placeholder*='描述']"],
        "save_selectors": [],  # 抖音图文无存草稿按钮：只填充，绝不点发布，留人工核对
        "auto_save": False,
        "save_ok_selectors": [],
        # 2026-07-14 真机：抖音图文上传区无 input[type=file]（点击触发系统对话框），必须走 file_chooser；
        # 图库首图默认即封面，无需再单独设封面。图不塞正文，正文只放「作品描述」文字。
        "image_upload": {
            "mode": "file_chooser",
            "trigger": ["div.bold-KtUGPM:has-text('点击上传')", "text=点击上传",
                        "div.preview-_mkRqT", "div.container-IRuUu2"],
        },
    },
    "csdn": {
        # 2026-07-17 真机 DOM 探测 + 全链路实测（存草稿成功，articleId=162975362）：
        #   · 编辑器是 **StackEdit 系**，不是 CodeMirror——正文 = pre.editor__inner[contenteditable]，
        #     里面直接放 markdown **源码**（无 .CodeMirror、window.CodeMirror 也不存在）→ body_mode=markdown
        #   · 首开必弹「模版库」div.modal(z-index:100 全屏)，不关掉则标题/存草稿的点击全被它吃
        #     （Playwright 明报 <div class="modal"> intercepts pointer events）→ dismiss_selectors
        #   · 标题是「点开才出 input」：常态 div.article-bar__title-display，input 本体 display:none
        #     → 必须先点显示层，否则 fill 被可操作性检查挡下 → title_pre_click
        #   · 入口只用 https://editor.csdn.net/md/：带 ?not_checkout=1 实测会被重定向去内容管理页
        #   · 回执：保存 toast 抓不到（一闪即逝），但存成功后 URL 会带上 ?articleId=xxx（服务端真发了
        #     草稿 id）→ 走 save_ok_url_patterns 这条硬证据
        "name": "CSDN",
        "status": "full",
        "draft_url": "https://editor.csdn.net/md/",
        "profile": _profile("csdn"),
        "login_url_patterns": ["passport.csdn.net", "/login"],
        "login_selectors": [".login-box", ".main-login", "iframe[src*='passport.csdn.net']"],
        "dismiss_selectors": ["button.modal__close-button",
                              ".modal__button-bar button:has-text('取消')"],
        "title_pre_click": [".article-bar__title-display"],
        "title_selectors": ["input.article-bar__title"],
        "editor_selectors": ["pre.editor__inner"],
        "body_mode": "markdown",
        "editor_wait": 10,
        "save_selectors": ["button.btn-save"],
        "auto_save": False,
        "save_ok_selectors": ["*:has-text('保存成功')"],
        "save_ok_url_patterns": ["articleId="],
    },
    "juejin": {
        # 2026-07-17 真机 DOM 探测：与 CSDN 同为 markdown 但**编辑器内核完全不同**——
        #   · 掘金用 **CodeMirror 5**（.CodeMirror，实例挂在元素的 .CodeMirror 属性上）
        #     → 注入必须走 cm.setValue()：CM 的 paste 监听在它自己的隐藏 textarea 上，
        #       往包装 div 派合成 paste 事件接不住（CSDN 那条 text/plain 通道在这儿不成立）
        #     → 校验也必须走 cm.getValue()：CM 虚拟滚动只渲染可视行，innerText 量长稿只有一屏
        #   · 标题 input.title-input 是常态可见的普通 input，直接 fill 即可（不像 CSDN 要先点开）
        #   · **没有存草稿按钮**：页面明写「文章将自动保存至草稿箱」，顶栏只有「草稿箱」(去列表)
        #     和「发布」→ auto_save=True，save_selectors 留空（绝不能去点「发布」）
        "name": "掘金",
        "status": "full",
        "draft_url": "https://juejin.cn/editor/drafts/new?v=2",
        "profile": _profile("juejin"),
        "login_url_patterns": ["juejin.cn/login", "/login"],
        "login_selectors": [".login-box", ".auth-box", "input[placeholder*='手机号']"],
        "title_selectors": ["input.title-input", "input[placeholder*='标题']"],
        "editor_selectors": [".CodeMirror"],
        "body_mode": "markdown",
        "editor_wait": 10,
        "save_selectors": [],   # 无存草稿按钮；auto_save 分支会短路，绝不会误点「发布」
        "auto_save": True,
        "save_ok_selectors": ["*:has-text('已自动保存')", "*:has-text('保存成功')",
                              "*:has-text('已保存')"],
    },
    # 下面两个平台已有更强的专用链路，不在这里重复实现
    "wechat": {
        "name": "微信公众号",
        "status": "delegate",
        "delegate_hint": ("公众号请改用「壹伴排版优化」技能（wechat-md-typesetter）："
                          "python ~/PolarisGEO/skills/wechat-md-typesetter/scripts/wechat_yiban.py "
                          "--mode publish --body-file 正文.html --title 标题 ——它带样式引擎+两段直送，比本脚本强。"),
    },
    "xhs": {
        "name": "小红书",
        "status": "delegate",
        "delegate_hint": ("小红书请改用「post-to-xhs」技能（图文/视频全流程、登录检查、只填不发），"
                          "本脚本不重复实现。"),
    },
}

LOGIN_WAIT_SECS = 180   # 等扫码登录的上限
MANUAL_HOLD_HINT = "浏览器窗口保持打开——填完/贴完后自己关窗口即可，脚本会等你。"


_T0 = time.time()


def _log(step, ok=True, **extra):
    rec = {"step": step, "ok": ok, "t": round(time.time() - _T0, 1)}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


def _final(result, detail="", **extra):
    rec = {"result": result, "detail": detail}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


def _collect_metric(**fields):
    """把一条投递侧采集数据（草稿状态/回执/平台/标题等）落盘到 data_store 的 metrics 类别。
    **最小侵入 + 失败静默降级**：data_store 缺失或落盘出错都只打一行日志，绝不影响投递主流程。"""
    if _data_store is None:
        return
    try:
        rec = {"source": "draft_uploader", "engine": BROWSER_ENGINE}
        rec.update(fields)
        _data_store.save_record("metrics", rec)
        _log("metric_saved", ok=True, category="metrics")
    except Exception as e:
        _log("metric_save_failed", ok=False, error=str(e).splitlines()[0][:120])


# ───────────────────────── markdown → 简单语义 HTML（零依赖，够粘贴用）─────────────────────────
def _md_inline(s):
    s = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", s)
    s = re.sub(r"(?<!\*)\*([^*]+?)\*(?!\*)", r"<em>\1</em>", s)
    s = re.sub(r"`([^`]+?)`", r"<code>\1</code>", s)
    s = re.sub(r"\[([^\]]+?)\]\(([^)]+?)\)", r"\1", s)  # 链接降级为纯文案（平台外链多半被拦）
    return s


def md_to_html(md):
    """极简 markdown → 语义 HTML。图片行 ![..](..) 直接剔除（图片单独走 --images 通道）。"""
    lines = md.replace("\r\n", "\n").split("\n")
    out, para, in_list = [], [], None

    def flush_para():
        if para:
            out.append("<p>" + _md_inline(" ".join(para)) + "</p>")
            para.clear()

    def close_list():
        nonlocal in_list
        if in_list:
            out.append("</%s>" % in_list)
            in_list = None

    for raw in lines:
        line = raw.rstrip()
        if re.match(r"^\s*!\[[^\]]*\]\([^)]*\)\s*$", line):
            continue  # 图片占位行剔除
        if not line.strip():
            flush_para()
            close_list()
            continue
        m = re.match(r"^(#{1,6})\s+(.*)$", line)
        if m:
            flush_para(); close_list()
            level = min(len(m.group(1)), 3)
            out.append("<h%d>%s</h%d>" % (level, _md_inline(m.group(2)), level))
            continue
        if re.match(r"^\s*([-*+])\s+", line):
            flush_para()
            if in_list != "ul":
                close_list(); out.append("<ul>"); in_list = "ul"
            out.append("<li>%s</li>" % _md_inline(re.sub(r"^\s*[-*+]\s+", "", line)))
            continue
        if re.match(r"^\s*\d+[.)]\s+", line):
            flush_para()
            if in_list != "ol":
                close_list(); out.append("<ol>"); in_list = "ol"
            out.append("<li>%s</li>" % _md_inline(re.sub(r"^\s*\d+[.)]\s+", "", line)))
            continue
        if line.lstrip().startswith(">"):
            flush_para(); close_list()
            out.append("<blockquote>%s</blockquote>" % _md_inline(line.lstrip()[1:].strip()))
            continue
        if re.match(r"^\s*(---+|\*\*\*+)\s*$", line):
            flush_para(); close_list()
            out.append("<hr>")
            continue
        para.append(line.strip())
    flush_para(); close_list()
    return "\n".join(out)


def _plain_text(html):
    txt = re.sub(r"<[^>]+>", " ", html)
    txt = (txt.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">")
              .replace("&amp;", "&").replace("&quot;", '"').replace("&#39;", "'"))
    return re.sub(r"[ \t]+", " ", txt).strip()


def _plain_len(html):
    return len(re.sub(r"\s+", "", _plain_text(html)))


def load_content(path):
    """读稿件文件：.html/内容以 < 开头按 HTML；否则按 markdown 转。返回 (html, md_or_raw)。"""
    with open(path, "r", encoding="utf-8") as f:
        raw = f.read()
    stripped = raw.lstrip()
    if path.lower().endswith((".html", ".htm")) or stripped.startswith("<"):
        return raw, _plain_text(raw)
    return md_to_html(raw), raw


# ───────────────────────── 系统剪贴板（manual 模式 / 降级辅助）─────────────────────────
def set_clipboard(text):
    """先 pyperclip，无则 powershell Set-Clipboard（走 UTF-8 临时文件避免编码坑）。"""
    try:
        import pyperclip  # type: ignore
        pyperclip.copy(text)
        return "pyperclip"
    except Exception:
        pass
    tmp_name = None
    try:
        tmp = tempfile.NamedTemporaryFile(mode="w", suffix=".txt", delete=False,
                                          encoding="utf-8-sig")
        tmp_name = tmp.name
        tmp.write(text)
        tmp.close()
        cmd = ("Get-Content -Raw -Encoding UTF8 '%s' | Set-Clipboard" % tmp_name.replace("'", "''"))
        for exe in ("powershell", "pwsh"):
            try:
                r = subprocess.run([exe, "-NoProfile", "-Command", cmd],
                                   capture_output=True, timeout=20)
                if r.returncode == 0:
                    return exe + " Set-Clipboard"
            except Exception:
                continue
    except Exception:
        pass
    finally:
        # 临时稿件文件含全文，用完即删，别残留在 temp 目录
        if tmp_name:
            try:
                os.remove(tmp_name)
            except Exception:
                pass
    return None


# ───────────────────────── 浏览器侧注入 JS（与 wechat_yiban.py 同一条粘贴通道）─────────────────────────
JS_FOCUS_SELECT = r"""
(root) => {
  root.focus();
  var sel = window.getSelection();
  var range = document.createRange();
  range.selectNodeContents(root);
  sel.removeAllRanges();
  sel.addRange(range);
  return true;
}
"""

# 合成粘贴：ClipboardEvent + DataTransfer（不碰系统剪贴板），走编辑器自己的 paste handler。
JS_PASTE = r"""
(root, args) => {
  try {
    var dt = new DataTransfer();
    dt.setData("text/html", args.html);
    dt.setData("text/plain", args.text || "");
    var ev = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
    root.dispatchEvent(ev);
    return true;
  } catch (e) { return false; }
}
"""

# markdown 编辑器专用粘贴：**只塞 text/plain，绝不塞 text/html**。
# CSDN/掘金 的正文框要的是 markdown **源码**；给了 text/html 它们会走「HTML→md 转换」
# 或直接吞掉语法，结果是 # / ** / ``` 全丢。少给一个 MIME 反而是正确性要求，不是偷懒。
JS_PASTE_TEXT = r"""
(root, args) => {
  try {
    root.focus();
    var dt = new DataTransfer();
    dt.setData("text/plain", args.text);
    var ev = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
    root.dispatchEvent(ev);
    return true;
  } catch (e) { return false; }
}
"""

# 降级 2a：execCommand('insertHTML')；2b：insertText（Draft.js 只认 text）。
JS_EXEC_INSERT_HTML = r"""
(root, args) => {
  root.focus();
  try { document.execCommand("selectAll", false, null); } catch (e) {}
  try { return document.execCommand("insertHTML", false, args.html); } catch (e) { return false; }
}
"""
JS_EXEC_INSERT_TEXT = r"""
(root, args) => {
  root.focus();
  try { document.execCommand("selectAll", false, null); } catch (e) {}
  try { return document.execCommand("insertText", false, args.text); } catch (e) { return false; }
}
"""

# 降级 3：innerText 直写 + 补发 input 事件（最后兜底，格式丢失但文字保底）。
JS_RAW_TEXT = r"""
(root, args) => {
  root.innerText = args.text;
  try { root.dispatchEvent(new InputEvent("input", { bubbles: true })); } catch (e) {}
  return true;
}
"""

JS_TEXT_LEN = r"""
(root) => ((root.innerText || root.textContent || "").replace(/\s+/g, "").length)
"""

# markdown 模式校验用：**不去空白、不删标记**的原始长度。md 源码里的换行/缩进/```
# 都是内容本身，按 JS_TEXT_LEN 那样折叠空白会系统性低估落字数、把成功判成失败。
# CodeMirror（掘金）必须走实例 API 读：CM 只把**可视行**渲染进 DOM（虚拟滚动），
# 长稿用 innerText 量到的永远只有一屏，会把成功的注入误判成失败。
JS_TEXT_LEN_RAW = r"""
(root) => {
  try {
    var cm = root.CodeMirror;
    if (cm && typeof cm.getValue === "function") return (cm.getValue() || "").length;
  } catch (e) {}
  return (root.innerText || root.textContent || "").length;
}
"""

# CodeMirror 专用注入：直接调实例 API。CM 的 paste 监听挂在它自己的隐藏 textarea 上，
# 往 .CodeMirror 包装 div 派合成 paste 事件根本不会被它接住——必须走 setValue。
JS_CM_SET_VALUE = r"""
(root, args) => {
  try {
    var cm = root.CodeMirror;
    if (!cm || typeof cm.setValue !== "function") return false;
    cm.setValue(args.text);
    cm.refresh();
    return true;
  } catch (e) { return false; }
}
"""

# 图片粘贴：dataURL → File → DataTransfer → 合成 paste（编辑器原生欢迎图片粘贴）。
JS_PASTE_IMAGE = r"""
async (root, args) => {
  try {
    var res = await fetch(args.dataUrl);
    var blob = await res.blob();
    var file = new File([blob], args.name, { type: blob.type });
    var dt = new DataTransfer();
    dt.items.add(file);
    root.focus();
    // 关键修复：inject_body 的粘贴通道会把整段正文留在"选中"态（selectNodeContents）。
    // 若此时直接贴图，image paste 会用图片**替换掉整段选中的正文**——用户就只看到图、没了正文。
    // 先把光标收拢到正文开头（collapse 到 start），既不覆盖正文，图片又落在最前=天然封面。
    try {
      var sel = window.getSelection();
      var range = document.createRange();
      range.selectNodeContents(root);
      range.collapse(true);
      sel.removeAllRanges();
      sel.addRange(range);
    } catch (e) {}
    var ev = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
    root.dispatchEvent(ev);
    return true;
  } catch (e) { return false; }
}
"""


def _first(frame_or_page, selectors):
    for sel in selectors:
        try:
            el = frame_or_page.query_selector(sel)
            if el:
                return el, sel
        except Exception:
            continue
    return None, None


def _find_in_frames(page, selectors):
    """跨 frame 找元素（百家号 UEditor 在 iframe 里）。返回 (frame, el, sel)。"""
    try:
        frames = page.frames
    except Exception:
        frames = [page]
    for fr in frames:
        el, sel = _first(fr, selectors)
        if el:
            return fr, el, sel
    return None, None, None


# ───────────────────────── 登录检测与等待 ─────────────────────────
def _looks_logged_out(page, cfg):
    try:
        url = (page.url or "").lower()
    except Exception:
        return False
    for pat in cfg.get("login_url_patterns", []):
        if pat.lower() in url:
            return True
    el, _ = _first(page, cfg.get("login_selectors", []))
    return el is not None


def wait_for_login(page, cfg, draft_url):
    """输出 need_login，保窗供扫码，轮询 URL/登录组件变化，最多 LOGIN_WAIT_SECS。
    登录成功返回 True（并重新导航到 draft_url）。"""
    _final("need_login", "检测到未登录（URL 被重定向或出现登录组件）。"
           "请在已打开的浏览器窗口里扫码登录，脚本等你 %d 秒。" % LOGIN_WAIT_SECS)
    deadline = time.time() + LOGIN_WAIT_SECS
    last_tick = time.time()
    while time.time() < deadline:
        time.sleep(2)
        try:
            if not _looks_logged_out(page, cfg):
                _log("login_detected", note="登录成功，继续投递")
                try:
                    page.goto(cfg["draft_url"] if not draft_url else draft_url,
                              wait_until="domcontentloaded")
                    time.sleep(3)
                except Exception:
                    pass
                if not _looks_logged_out(page, cfg):
                    return True
        except Exception:
            # 页面可能在登录跳转中被销毁重建，容错继续轮询
            pass
        if time.time() - last_tick > 20:
            _log("waiting_login", remain=int(deadline - time.time()))
            last_tick = time.time()
    return False


# ───────────────────────── 注入正文（粘贴通道 + 三级降级 + 字数校验）─────────────────────────
def inject_body(frame, el_sel, html, text):
    expect = max(1, len(re.sub(r"\s+", "", text)))
    threshold = max(1, int(expect * 0.6))

    def landed():
        try:
            return frame.eval_on_selector(el_sel, JS_TEXT_LEN)
        except Exception:
            return -1

    # ① 粘贴通道（选区与粘贴分两步，给编辑器留同步选区的时隙）
    try:
        frame.eval_on_selector(el_sel, JS_FOCUS_SELECT)
        time.sleep(0.4)
        frame.eval_on_selector(el_sel, JS_PASTE, {"html": html, "text": text})
        time.sleep(1.0)
        n = landed()
        if n >= threshold:
            return True, "paste", n
    except Exception:
        pass
    # ② execCommand insertHTML → insertText
    try:
        frame.eval_on_selector(el_sel, JS_EXEC_INSERT_HTML, {"html": html})
        time.sleep(0.8)
        n = landed()
        if n >= threshold:
            return True, "execCommand:insertHTML", n
    except Exception:
        pass
    try:
        frame.eval_on_selector(el_sel, JS_EXEC_INSERT_TEXT, {"text": text})
        time.sleep(0.8)
        n = landed()
        if n >= threshold:
            return True, "execCommand:insertText", n
    except Exception:
        pass
    # ③ innerText 直写（保文字弃格式）
    try:
        frame.eval_on_selector(el_sel, JS_RAW_TEXT, {"text": text})
        time.sleep(0.6)
        n = landed()
        return (n >= threshold), "innerText", n
    except Exception:
        return False, "none", -1


def inject_markdown(frame, el_sel, md):
    """markdown 编辑器（CSDN StackEdit 系 pre.editor__inner / 掘金）注入 **markdown 源码**。

    与 inject_body 的根本区别：这里要的是源码保真，不是富文本渲染——
      · 只走 text/plain 粘贴通道（塞 text/html 会让编辑器把 md 语法转换掉）
      · 校验按**源码字符数**（含空白与 #/*/` 等标记），不能复用 inject_body 那套
        「去空白后的纯文字长度」——那会把 ```python 之类的语法标记算漏、误判失败。
    返回 (ok, method, landed)。"""
    expect = max(1, len(md))
    threshold = max(1, int(expect * 0.6))

    def landed():
        try:
            return frame.eval_on_selector(el_sel, JS_TEXT_LEN_RAW)
        except Exception:
            return -1

    # ① CodeMirror（掘金）：直接调实例 setValue。放在最前——CM 接不住合成 paste，
    #    先试粘贴只会白等一轮超时。非 CM 平台（CSDN）这里必然 false，立刻落到 ②。
    try:
        if frame.eval_on_selector(el_sel, JS_CM_SET_VALUE, {"text": md}):
            time.sleep(0.8)
            n = landed()
            if n >= threshold:
                return True, "codemirror:setValue", n
    except Exception:
        pass
    # ② 全选（覆盖模板/欢迎稿）→ 只贴 text/plain
    try:
        frame.eval_on_selector(el_sel, JS_FOCUS_SELECT)
        time.sleep(0.4)
        frame.eval_on_selector(el_sel, JS_PASTE_TEXT, {"text": md})
        time.sleep(1.2)
        n = landed()
        if n >= threshold:
            return True, "paste:text/plain", n
    except Exception:
        pass
    # ③ execCommand insertText（同样只走纯文本）
    try:
        frame.eval_on_selector(el_sel, JS_EXEC_INSERT_TEXT, {"text": md})
        time.sleep(0.8)
        n = landed()
        if n >= threshold:
            return True, "execCommand:insertText", n
    except Exception:
        pass
    # ④ innerText 直写兜底
    try:
        frame.eval_on_selector(el_sel, JS_RAW_TEXT, {"text": md})
        time.sleep(0.6)
        n = landed()
        return (n >= threshold), "innerText", n
    except Exception:
        return False, "none", -1


def fill_title(page, cfg, title):
    if not title:
        return False
    # 「点开才出 input」型标题（CSDN：常态是 div.article-bar__title-display，input 本体 display:none，
    # 直接 fill 会被 Playwright 的可操作性检查挡下）——先点显示层把真 input 唤出来。
    for sel in cfg.get("title_pre_click", []):
        try:
            el = page.query_selector(sel)
            if el and el.is_visible():
                el.click()
                time.sleep(0.5)
                break
        except Exception:
            continue
    fr, el, sel = _find_in_frames(page, cfg.get("title_selectors", []))
    if not el:
        return False
    try:
        el.fill(title)
        return True
    except Exception:
        try:
            el.click()
            page.keyboard.type(title)
            return True
        except Exception:
            return False


def dismiss_overlays(page, cfg):
    """关掉开页即弹、且会吃掉后续所有点击的浮层。
    CSDN 首开编辑器必弹「模版库」(div.modal, z-index:100 全屏)——不关掉，点标题和存草稿
    全被它拦截（真机现象：Playwright 报 <div class="modal"> intercepts pointer events）。"""
    for sel in cfg.get("dismiss_selectors", []):
        try:
            el = page.query_selector(sel)
            if el and el.is_visible():
                el.click()
                time.sleep(0.6)
                _log("overlay_dismissed", selector=sel)
                return True
        except Exception:
            continue
    return False


def paste_images(page, frame, el_sel, images):
    """逐张贴图：优先编辑器粘贴通道（File 进 DataTransfer），失败试 input[type=file]，
    再失败就提示手动。返回 (pasted, hints)。"""
    pasted, hints = [], []
    # 关键：inject_body 走 selectNodeContents 把整段正文留在"选中"态，且 ProseMirror/Draft 维护自己的
    # 选区模型（无视 JS 层 window.getSelection 的收拢）。必须用**真实光标移动**把选区收拢到正文开头，
    # 否则接下来的合成贴图会用图片替换掉整段选中的正文（现象=只剩图、正文全没）。图落最前=天然封面。
    try:
        el0 = frame.query_selector(el_sel)
        if el0:
            el0.click()
            page.keyboard.press("Control+Home")
            time.sleep(0.3)
    except Exception:
        pass
    for img in images:
        img = img.strip()
        if not img:
            continue
        if not os.path.isfile(img):
            hints.append("图片不存在: %s" % img)
            continue
        ok = False
        try:
            with open(img, "rb") as f:
                raw = f.read()
            ext = os.path.splitext(img)[1].lower().lstrip(".") or "png"
            mime = {"jpg": "jpeg", "jpeg": "jpeg", "gif": "gif", "webp": "webp"}.get(ext, "png")
            data_url = "data:image/%s;base64,%s" % (mime, base64.b64encode(raw).decode("ascii"))
            frame.eval_on_selector(el_sel, JS_PASTE_IMAGE,
                                   {"dataUrl": data_url, "name": os.path.basename(img)})
            time.sleep(2.5)  # 等编辑器接收/上传
            ok = True  # 粘贴事件已派发；是否真落位由用户在窗口里目检
        except Exception:
            ok = False
        if not ok:
            try:
                fin, _, _ = _find_in_frames(page, ["input[type=file]"])
                if fin:
                    el, _ = _first(fin, ["input[type=file]"])
                    el.set_input_files(img)
                    time.sleep(2.5)
                    ok = True
            except Exception:
                ok = False
        if ok:
            pasted.append(img)
            _log("image_pasted", path=img)
        else:
            hints.append("图片未能自动贴入，请手动拖进编辑器: %s" % img)
            _log("image_manual", ok=False, path=img)
    return pasted, hints


def set_cover(page, cfg, cover_path):
    """按平台 cover 配置设置封面（区别于往正文塞图）：可选 pre_click(如「单图」) →
    open(打开封面弹窗) → 把文件喂给 input[accept=image]（隐藏 input 也能 set_input_files）→
    confirm(确定/完成)。全程 best-effort，任何一步失败都不抛异常，窗口保持供人工核对。
    返回 (ok, note)。"""
    cc = cfg.get("cover")
    if not cc:
        return False, "本平台未配置封面流程"
    if not cover_path or not os.path.isfile(cover_path):
        return False, "封面图不存在: %s" % cover_path
    try:
        # 封面区常在正文下方——先把「设置封面」滚进视口，否则 open/更换 都点不到（元素在 DOM 但离屏）
        for sel in cc.get("scroll_into", []):
            try:
                page.locator(sel).first.scroll_into_view_if_needed(timeout=3000)
                time.sleep(0.6)
                break
            except Exception:
                pass
        for sel in cc.get("pre_click", []):
            try:
                page.click(sel, timeout=3000)
                time.sleep(0.6)
            except Exception:
                pass
        # 「更换」是 hover 封面缩略图才冒出来的 overlay；且点「标签 span」这类元素虽能点中却不开弹窗
        # （假成功）。故：先 hover 缩略图露出 overlay → 点 open 选择器 → **验证弹窗真开了**（图片 input
        # 或「点击本地上传」出现）才算 opened，否则继续试下一个。整段最多重试 3 轮，根治偶发点空。
        file_sels = cc.get("file_input", ["input[accept*='image']"])

        def _modal_open():
            _, e, _ = _find_in_frames(page, file_sels)
            if e:
                return True
            try:
                return page.locator("text=点击本地上传").count() > 0
            except Exception:
                return False

        def _hover_thumb():
            # 优先 hover 已配置的缩略图选择器；都不行就按「设置封面」标签下方 ~110px 坐标 hover
            for hsel in cc.get("hover", []):
                try:
                    page.hover(hsel, timeout=1500)
                    time.sleep(0.4)
                    return
                except Exception:
                    pass
            for rsel in cc.get("scroll_into", []):
                try:
                    bb = page.locator(rsel).first.bounding_box()
                    if bb:
                        page.mouse.move(bb["x"] + 90, bb["y"] + 110)
                        time.sleep(0.4)
                        return
                except Exception:
                    pass

        opened = False
        for _ in range(3):
            if _modal_open():
                opened = True
                break
            _hover_thumb()
            for sel in cc.get("open", []):
                try:
                    page.click(sel, timeout=2500)
                    time.sleep(1.5)
                    if _modal_open():
                        opened = True
                        break
                except Exception:
                    pass
            if opened:
                break
            time.sleep(0.6)
        if not opened:
            return False, "没能打开封面弹窗（hover+「更换」重试 3 轮仍未见图片上传区，结构可能又变了）"
        # 弹窗可能默认停在别的 tab——若图片 input 还没出现，点「点击本地上传」激活本地上传区
        for sel in cc.get("post_open_click", []):
            try:
                pre_fr, pre_el, _ = _find_in_frames(page, file_sels)
                if pre_el:
                    break
                page.click(sel, timeout=2500)
                time.sleep(1.0)
            except Exception:
                pass
        # 找图片 file input（只认 accept=image，绝不回退 input[type=file]=视频框）
        ffr, fel, fsel = _find_in_frames(page, file_sels)
        if not fel:
            return False, "没找到封面图片 input[accept=image]（弹窗未开或结构变了）"
        try:
            fel.set_input_files(cover_path)
        except Exception as e:
            return False, "喂图失败: %s" % str(e)[:60]
        # 轮询「确定」按钮出现且可点就立即点（上传/裁剪是异步的，盲等要么白等要么点到禁用键）。
        # 最多等 12s；点成功即弹窗关闭，后续存草稿才不会被弹窗挡住。
        time.sleep(min(1.5, cc.get("upload_wait", 3.0)))  # 给上传一点起步时间
        confirmed = False
        deadline = time.time() + 12
        confirm_sels = cc.get("confirm", [])
        while time.time() < deadline and not confirmed:
            for sel in confirm_sels:
                try:
                    btn = page.locator(sel).first
                    if btn.count() and btn.is_enabled(timeout=600):
                        btn.click(timeout=2000)
                        confirmed = True
                        break
                except Exception:
                    pass
            if not confirmed:
                time.sleep(0.5)
        time.sleep(0.8)
        return True, ("封面已上传并点确认" if confirmed else "封面已喂入，但未点到确认键，请在窗口里手动点「确定」")
    except Exception as e:
        return False, "封面流程异常: %s" % str(e)[:80]


def upload_via_file_chooser(page, triggers, images):
    """图库型平台（抖音图文等）：上传区是「点击触发系统文件对话框」，DOM 里没有 input[type=file]，
    必须用 Playwright expect_file_chooser 捕获。逐张上传，返回 (done, hints)。
    这类平台第一张图库图通常即默认封面，无需再单独设封面。"""
    done, hints = [], []
    for img in images:
        img = img.strip()
        if not img:
            continue
        if not os.path.isfile(img):
            hints.append("图不存在: %s" % img)
            continue
        ok = False
        for trig in triggers:
            try:
                with page.expect_file_chooser(timeout=6000) as fc:
                    page.click(trig, timeout=4000)
                fc.value.set_files(img)
                time.sleep(3.0)  # 等上传
                ok = True
                _log("gallery_uploaded", path=img, via=trig)
                break
            except Exception:
                continue
        if ok:
            done.append(img)
        else:
            hints.append("图库上传失败（请手动把图拖入上传区）: %s" % img)
            _log("gallery_upload_fail", ok=False, path=img)
    return done, hints


def save_draft(page, cfg):
    """点「存草稿」并等回执。auto_save 平台只等自动保存提示。返回 (clicked, confirmed)。"""
    if cfg.get("auto_save"):
        # 知乎等平台按内容变更自动存草稿——正文刚粘贴入档就是变更，这里只等保存提示出现
        confirmed = _wait_receipt(page, cfg, 12)
        return True, confirmed
    # 本平台无存草稿按钮（如抖音图文）：只填充、不保存、不按 Ctrl+S（避免触发浏览器保存框），留人工
    if not cfg.get("save_selectors"):
        return False, False
    clicked = False
    fr, el, sel = _find_in_frames(page, cfg.get("save_selectors", []))
    if el:
        try:
            el.click()
            clicked = True
            _log("save_clicked", selector=sel)
        except Exception:
            pass
    if not clicked:
        try:
            page.keyboard.press("Control+s")
            clicked = True
            _log("save_hotkey")
        except Exception:
            pass
    confirmed = _wait_receipt(page, cfg, 12) if clicked else False
    return clicked, confirmed


def _wait_receipt(page, cfg, seconds):
    """等保存回执：文案选择器 **或** URL 特征，任一命中即确认。
    URL 通道是给 CSDN 这类「toast 一闪而过、但存草稿成功后 URL 会带上 ?articleId=xxx」的平台——
    真机实测它的成功提示抓不到（3.5s 内已消失），而 articleId 是服务端真发了草稿 id 的硬证据。"""
    selectors = cfg.get("save_ok_selectors", [])
    url_pats = cfg.get("save_ok_url_patterns", [])
    if not selectors and not url_pats:
        return False
    deadline = time.time() + seconds
    while time.time() < deadline:
        if url_pats:
            try:
                url = page.url or ""
                if any(p in url for p in url_pats):
                    return True
            except Exception:
                pass
        if selectors:
            fr, el, _ = _find_in_frames(page, selectors)
            if el:
                return True
        time.sleep(0.5)
    return False


# 批量/AI 模式：--close-after 时存完草稿即收尾退出，便于同一账号连续发多篇
# （默认 True=保持窗口，供人工预览核对；置 False=自动收尾）
HOLD_WINDOW = True


def hold_window(ctx):
    """收尾三态（草稿已入库/剪贴板已备好之后才会走到这里）：
      ① CDP（缺省引擎）：Chrome 是独立进程——**只断开连接、立即退出**，窗口留在原地供用户
         预览草稿、核对配图、亲手点发布；脚本被上游超时硬杀也不影响窗口。
         --close-after 时只关本次投递开的那个标签页（绝不动用户其它标签），Chrome 常驻，
         下一篇直接接管，免重启免 profile 锁。
      ② 非 CDP 回退引擎 + 保窗：浏览器是脚本子进程，进程必须活着陪窗口——轮询到用户
         把页面全关掉才收尾（老行为，需要上游给足超时）。
      ③ 非 CDP + --close-after：直接关浏览器退出。"""
    if getattr(ctx, "_cdp", False):
        if not HOLD_WINDOW:
            page = getattr(ctx, "_polaris_page", None)
            if page:
                try:
                    page.close()
                except Exception:
                    pass
        pw = getattr(ctx, "_pw", None)
        if pw:
            try:
                pw.stop()          # 只断 CDP；Chrome 独立进程，继续活着
            except Exception:
                pass
        if HOLD_WINDOW:
            print("[投递] 已断开 CDP —— 浏览器窗口保持打开（独立进程，脚本退出/被超时杀掉"
                  "都不影响），请在窗口里预览草稿、核对配图与封面，确认后自行发布。", flush=True)
        return
    if not HOLD_WINDOW:
        try:
            ctx.close()
        except Exception:
            pass
        pw = getattr(ctx, "_pw", None)
        if pw:
            try:
                pw.stop()
            except Exception:
                pass
        return
    print("[投递] %s（当前引擎 %s 非 CDP：浏览器随脚本进程存活，"
          "请别在窗口核对完之前杀掉本脚本）" % (MANUAL_HOLD_HINT, BROWSER_ENGINE), flush=True)
    try:
        while True:
            pages = list(getattr(ctx, "pages", []) or [])
            if not pages:
                break
            time.sleep(2)
    except Exception:
        pass
    try:
        ctx.close()
    except Exception:
        pass
    pw = getattr(ctx, "_pw", None)
    if pw:
        try:
            pw.stop()
        except Exception:
            pass


def clipboard_assist(title, text):
    """标题+正文进系统剪贴板。返回使用的通道名或 None。"""
    payload = (title + "\n\n" + text) if title else text
    via = set_clipboard(payload)
    if via:
        _log("clipboard_set", via=via, chars=len(payload))
    else:
        _log("clipboard_failed", ok=False, note="pyperclip 与 powershell Set-Clipboard 都失败")
    return via


# ───────────────────────── 主流程 ─────────────────────────
def _await_ready(page, cfg):
    """自适应等待编辑器就绪：标题框或正文编辑器一旦在任一 frame 出现就立即返回，
    最多等 editor_wait 秒。取代固定盲等——页面快时秒过，慢时(如百家号 iframe)才多等。"""
    time.sleep(0.8)  # 给首帧一点渲染时间
    deadline = time.time() + cfg.get("editor_wait", 4)
    sels = list(cfg.get("editor_selectors", [])) + list(cfg.get("title_selectors", []))
    if not sels:
        time.sleep(cfg.get("editor_wait", 4) - 0.8)
        return False
    while time.time() < deadline:
        try:
            fr, el, _ = _find_in_frames(page, sels)
            if el:
                time.sleep(0.4)  # 命中后再稳定一下
                return True
        except Exception:
            pass
        time.sleep(0.4)
    return False


def run(platform, title, content_file, images, manual):
    cfg = PLATFORMS.get(platform)
    if not cfg:
        _final("failed", "未知平台 %s；支持: %s" % (platform, "/".join(PLATFORMS)))
        return 2

    # wechat / xhs：已有更强专用链路，直接提示后退出
    if cfg["status"] == "delegate":
        _log("delegate", platform=platform)
        print("[投递] " + cfg["delegate_hint"], flush=True)
        _final("manual_assist", cfg["delegate_hint"], delegate=True)
        return 0

    # 读稿件
    html, raw = "", ""
    if content_file:
        try:
            html, raw = load_content(content_file)
            _log("content_loaded", file=os.path.abspath(content_file), chars=_plain_len(html))
        except Exception as e:
            _final("failed", "读稿件失败: %s" % e)
            return 1
    text = _plain_text(html) if html else ""

    # 开浏览器 + 打开编辑页（多引擎自动回退：本地 Chrome 崩溃/超时 → CloakBrowser；导航自带重试）
    try:
        ctx, page = open_editor(cfg)
    except Exception as e:
        _final("failed", "打开编辑页失败（所有引擎耗尽）: %s" % str(e).splitlines()[0][:120])
        return 1

    try:
        _await_ready(page, cfg)  # 自适应等待编辑器就绪（快则秒过，慢则最多 editor_wait 秒）

        # 登录检测（manual 模式也做——没登录连手贴都贴不了）
        if _looks_logged_out(page, cfg):
            if not wait_for_login(page, cfg, cfg["draft_url"]):
                # 登录超时：降级 manual——剪贴板备好，窗口留着慢慢扫
                clipboard_assist(title, raw or text)
                _final("manual_assist",
                       "登录等待超时（%ds）。窗口保持打开，登录后正文可直接从剪贴板粘贴。" % LOGIN_WAIT_SECS)
                hold_window(ctx)
                return 0

        # 关掉开页即弹的浮层（CSDN「模版库」）——必须在任何点击动作之前，否则它吃掉所有 click。
        # 放在登录检测之后：未登录时弹的是登录框，不该被这里误关。
        dismiss_overlays(page, cfg)

        # ── manual 模式 / partial 平台：编辑页已开，剪贴板辅助 ──
        if manual or cfg["status"] == "partial":
            # partial 平台若标题选择器可用，先尽力把标题自动填上（如 B站标题 input 可用，
            # 只有正文编辑器够不到），减少人工步骤——只剩正文一次 Ctrl+V。
            title_auto = False
            if cfg.get("title_selectors"):
                try:
                    title_auto = fill_title(page, cfg, title)
                    _log("title_filled", ok=title_auto, title=title)
                except Exception:
                    title_auto = False
            # 标题已自动填入时，剪贴板只放正文，人工一次 Ctrl+V 即可（避免重复贴标题）
            via = clipboard_assist("" if title_auto else title, raw or text)
            head = "标题已自动填入，正文" if title_auto else "标题+正文"
            note = ("已打开%s编辑页，%s已进系统剪贴板（%s），"
                    "光标点进正文框 Ctrl+V 即可。" % (cfg["name"], head, via or "剪贴板失败，请从稿件文件复制"))
            if images:
                note += " 配图请手动拖入: %s" % ", ".join(images)
            print("[投递] " + note, flush=True)
            _final("manual_assist", note, platform=platform,
                   partial=(cfg["status"] == "partial" and not manual))
            hold_window(ctx)
            return 0

        # ── AI 直传：填标题 → 粘贴正文 → 贴图 → 存草稿 ──
        title_ok = fill_title(page, cfg, title)
        _log("title_filled", ok=title_ok, title=title)

        # 正文编辑器可能比标题晚挂载（如头条 ProseMirror）——_await_ready 见到标题就返回了，
        # 这里对编辑器再轮询重试，避免"标题在、编辑器还没好"时误判后台改版而降级手动。
        fr, el, el_sel = _find_in_frames(page, cfg["editor_selectors"])
        if not el:
            deadline = time.time() + 8
            while time.time() < deadline and not el:
                time.sleep(0.6)
                fr, el, el_sel = _find_in_frames(page, cfg["editor_selectors"])
        if not el:
            raise RuntimeError("没找到正文编辑器（选择器全部落空，可能后台改版）")

        if cfg.get("body_mode") == "markdown":
            # markdown 编辑器（CSDN/掘金）：喂 **原始 md 源码**（load_content 的 raw），
            # 不能喂 html/纯文本——前者会被转换掉语法，后者直接丢掉 #/**/``` 标记。
            md_src = raw or text
            ok, method, landed = inject_markdown(fr, el_sel, md_src)
        else:
            ok, method, landed = inject_body(fr, el_sel, html, text)
        _log("body_injected", ok=ok, method=method, chars=landed)
        if not ok:
            raise RuntimeError("正文注入三级通道全部失败（落入 %d 字）" % landed)

        img_hints = []
        if images:
            if cfg.get("image_upload"):
                # 图库型平台（抖音图文）：图走图库上传(file_chooser)，不塞正文；首图默认即封面。
                gdone, ghints = upload_via_file_chooser(page, cfg["image_upload"]["trigger"], images)
                img_hints += ghints
                if gdone:
                    img_hints.append("已上传 %d 张到图库（首图默认封面）" % len(gdone))
            elif cfg.get("cover"):
                # 有专门封面流程的平台：第一张图走「设置封面」，其余(若有)才塞进正文。
                cov_ok, cov_note = set_cover(page, cfg, images[0])
                _log("cover_set", ok=cov_ok, note=cov_note, path=images[0])
                img_hints.append(("封面：" + cov_note) if not cov_ok else "封面已设置")
                if images[1:]:
                    _, more_hints = paste_images(page, fr, el_sel, images[1:])
                    img_hints += more_hints
            else:
                # 没有封面/图库流程的平台：维持原逻辑——全部塞进正文。
                _, more_hints = paste_images(page, fr, el_sel, images)
                img_hints += more_hints

        clicked, confirmed = save_draft(page, cfg)
        _log("draft_saved", ok=clicked, confirmed=confirmed)

        # 标题没自动填上（如百家号 FeEditor 标题框定位不稳）：存完草稿后把标题送剪贴板兜底，
        # 让它成为剪贴板最后内容（不与正文/贴图的合成粘贴冲突），用户点标题栏一次 Ctrl+V 即可。
        title_clip = False
        if not title_ok:
            title_clip = bool(set_clipboard(title))
            _log("title_to_clipboard", ok=title_clip)

        # 「没有存草稿按钮」≠「没有草稿箱」：知乎/掘金都没按钮，但都会自动存进草稿箱。
        # 只按 save_selectors 空判定会对这两个平台谎报「本平台无草稿箱」——auto_save 必须排除在外。
        no_draft_btn = not cfg.get("save_selectors") and not cfg.get("auto_save")
        if no_draft_btn:
            save_desc = "本平台无草稿箱，已完成填充，请核对后自行发布"
            confirm_desc = ""
        else:
            save_desc = "已存草稿" if clicked else "没找到存草稿按钮，请在窗口里手动保存"
            confirm_desc = "（见保存回执）" if confirmed else "（未见明确回执，请在窗口目检）"
        detail = "%s：正文已入编辑器（通道=%s，%d 字），%s%s" % (
            cfg["name"], method, landed, save_desc, confirm_desc)
        if not title_ok:
            detail += "；标题未能自动填入%s，请点标题栏 Ctrl+V" % ("（已复制到剪贴板）" if title_clip else "")
        if img_hints:
            detail += "；" + "；".join(img_hints)
        detail += "。铁律：脚本不点发布，请自行到后台核对后发布。"
        _final("draft_uploaded", detail, platform=platform, method=method,
               title_filled=title_ok, title_clipboard=title_clip,
               save_clicked=clicked, save_confirmed=confirmed)
        # 采集点：投递刚拿到草稿状态/回执，落一条 metrics 备份（失败静默，不影响下方保窗收尾）
        _collect_metric(result="draft_uploaded", platform=platform, title=title,
                        method=method, chars=landed, title_filled=title_ok,
                        save_clicked=clicked, save_confirmed=confirmed,
                        images=len(images) if images else 0)
        # 结果 JSON 已输出（上游可解析）。CDP 模式：断连即退，窗口独立常驻供预览；
        # 非 CDP 回退：老行为，进程陪窗口等用户关。
        hold_window(ctx)
        return 0

    except Exception as e:
        # 任何失败降级 manual：剪贴板备好 + 窗口留着
        _log("degrade_to_manual", ok=False, error=str(e))
        try:
            via = clipboard_assist(title, raw or text)
            note = ("自动投递失败（%s）。已降级手动辅助：编辑页保持打开，"
                    "标题+正文已进剪贴板（%s），Ctrl+V 贴入即可。" % (e, via or "剪贴板也失败，请从稿件文件复制"))
            print("[投递] " + note, flush=True)
            _final("manual_assist", note, platform=platform, degraded=True)
            hold_window(ctx)
            return 0
        except Exception as e2:
            _final("failed", "自动投递失败且降级也失败: %s / %s" % (e, e2))
            try:
                ctx.close()
            except Exception:
                pass
            return 1


def main():
    ap = argparse.ArgumentParser(description="多平台草稿投递（只存草稿，绝不发布）")
    ap.add_argument("--platform", required=True,
                    help="平台: %s" % "/".join(PLATFORMS))
    ap.add_argument("--title", default="", help="文章标题")
    ap.add_argument("--content-file", default="", help="正文文件（.md 或 .html，UTF-8）")
    ap.add_argument("--images", default="", help="配图路径，逗号分隔")
    ap.add_argument("--manual", action="store_true",
                    help="手动辅助模式：只开编辑页+标题正文进剪贴板，不自动填充")
    ap.add_argument("--close-after", action="store_true",
                    help="存完草稿即收尾退出（批量/AI 模式）：CDP 引擎只关本次标签页、"
                         "Chrome 常驻给下一篇复用；非 CDP 引擎整个关掉")
    args = ap.parse_args()

    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

    global HOLD_WINDOW
    if args.close_after:
        HOLD_WINDOW = False

    images = [p for p in (args.images.split(",") if args.images else []) if p.strip()]
    return run(args.platform.strip().lower(), args.title, args.content_file, images, args.manual)


if __name__ == "__main__":
    sys.exit(main())
