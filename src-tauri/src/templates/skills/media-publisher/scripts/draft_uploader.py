#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Polaris GEO · 多平台草稿投递引擎 draft_uploader.py
==================================================

把写好的稿件（标题 + 正文 markdown/html + 可选配图）送进各平台创作者后台的编辑器，
**只存草稿 / 停在编辑页，绝不点发布**。发布键永远留给用户亲手点。

设计与 wechat_yiban.py 同源：
  - CloakBrowser 优先（drop-in 替换 Playwright），没装退回 playwright.sync_api；
  - launch_persistent_context 持久 profile（登录态永久留在 ~/PolarisGEO/browser-profiles/{platform}）；
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
import subprocess
import sys
import tempfile
import time

# ───────────────────────── CloakBrowser 优先，playwright 回退 ─────────────────────────
try:
    from cloakbrowser import launch_persistent_context  # type: ignore
    BROWSER_ENGINE = "cloakbrowser"
except Exception:
    try:
        from playwright.sync_api import sync_playwright  # type: ignore
    except Exception:
        sync_playwright = None
    BROWSER_ENGINE = "playwright"

    def launch_persistent_context(user_data_dir=".", headless=False, humanize=False, **_):
        if sync_playwright is None:
            raise RuntimeError("cloakbrowser 与 playwright 都不可用，请先 pip install cloakbrowser")
        pw = sync_playwright().start()
        ctx = pw.chromium.launch_persistent_context(user_data_dir, headless=headless)
        ctx._pw = pw
        return ctx


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
        "save_selectors": [
            "button:has-text('存草稿')",
            "div[class*='garbage']:has-text('存草稿')",
            "*:text-is('存草稿')",
            "button:has-text('保存草稿')",
        ],
        "auto_save": False,
        "save_ok_selectors": ["*:has-text('保存成功')", "*:has-text('草稿保存成功')", "*:has-text('已保存')"],
    },
    "bilibili": {
        # 2026-07 实测：B站专栏正文用非标准编辑器（4 次 DOM 探测均无任何 contenteditable/
        # textarea/富文本类元素，点击后焦点仍停在 body，疑似 canvas 类自绘），标准选择器
        # 自动化够不到。故降级 partial：自动填标题（标题 input 可用）+ 正文进剪贴板，人工粘贴。
        "name": "B站专栏",
        "status": "partial",
        "draft_url": "https://member.bilibili.com/read/editor/#/new",
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
        # 2026-07 实测：百家号用百度 UEditor，正文在 about:blank 子 iframe 的 <body contenteditable>
        # 里，页面有明确「存草稿」按钮 → 可全自动。正文选择器靠 _find_in_frames 进子 iframe 命中 body。
        "name": "百家号",
        "status": "full",
        "draft_url": "https://baijiahao.baidu.com/builder/rc/edit?type=news",
        "profile": _profile("baijia"),
        "login_url_patterns": ["builder/theme/bjh/login", "passport.baidu.com", "/login"],
        "login_selectors": ["#passport-login-pop", ".pass-login-pop", ".tang-pass-qrcode"],
        "title_selectors": ["textarea[placeholder*='标题']", "input[placeholder*='标题']",
                            "textarea.article-title", ".title-content textarea", "div[contenteditable=true][data-placeholder*='标题']"],
        "editor_selectors": ["body.view", "body[contenteditable=true]", "body.edui-body-container",
                            "#ueditor_0", "[contenteditable=true]"],
        "save_selectors": ["button:has-text('存草稿')", "*:text-is('存草稿')", "*:has-text('存草稿')", "*:has-text('保存草稿')"],
        "auto_save": False,
        "save_ok_selectors": ["*:has-text('已保存')", "*:has-text('保存成功')", "*:has-text('保存于')"],
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
        "title_selectors": ["input[placeholder*='标题']", "input.semi-input", "textarea[placeholder*='标题']"],
        "editor_selectors": ["div.editor-kit-container[contenteditable=true]",
                            "div[contenteditable=true][data-placeholder*='描述']", "div[contenteditable=true]"],
        "save_selectors": [],  # 抖音图文无存草稿按钮：只填充，绝不点发布，留人工核对
        "auto_save": False,
        "save_ok_selectors": [],
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


def _log(step, ok=True, **extra):
    rec = {"step": step, "ok": ok}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


def _final(result, detail="", **extra):
    rec = {"result": result, "detail": detail}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


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


def fill_title(page, cfg, title):
    if not title:
        return False
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


def paste_images(page, frame, el_sel, images):
    """逐张贴图：优先编辑器粘贴通道（File 进 DataTransfer），失败试 input[type=file]，
    再失败就提示手动。返回 (pasted, hints)。"""
    pasted, hints = [], []
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


def save_draft(page, cfg):
    """点「存草稿」并等回执。auto_save 平台只等自动保存提示。返回 (clicked, confirmed)。"""
    if cfg.get("auto_save"):
        # 知乎等平台按内容变更自动存草稿——正文刚粘贴入档就是变更，这里只等保存提示出现
        confirmed = _wait_any(page, cfg.get("save_ok_selectors", []), 12)
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
    confirmed = _wait_any(page, cfg.get("save_ok_selectors", []), 12) if clicked else False
    return clicked, confirmed


def _wait_any(page, selectors, seconds):
    if not selectors:
        return False
    deadline = time.time() + seconds
    while time.time() < deadline:
        fr, el, _ = _find_in_frames(page, selectors)
        if el:
            return True
        time.sleep(0.5)
    return False


# 批量/AI 模式：--close-after 时存完草稿即关窗退出，便于同一 profile 连续发多篇
# （默认 True=保持窗口，供人工核对；置 False=自动收尾）
HOLD_WINDOW = True


def hold_window(ctx):
    """窗口保持到用户自己关（manual / 降级辅助模式的收尾）。
    --close-after 模式下不等待，直接收尾关闭，让批量投递能顺序释放 profile 锁。"""
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
    print("[投递] %s" % MANUAL_HOLD_HINT, flush=True)
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

    # 开浏览器（持久 profile，登录态常驻）
    profile = cfg["profile"]
    os.makedirs(profile, exist_ok=True)
    try:
        ctx = launch_persistent_context(user_data_dir=profile, headless=False, humanize=True)
    except Exception as e:
        _final("failed", "浏览器启动失败（%s）: %s" % (BROWSER_ENGINE, e))
        return 1
    _log("browser_launched", engine=BROWSER_ENGINE, profile=profile)

    page = None
    try:
        page = ctx.new_page() if hasattr(ctx, "new_page") else ctx.pages[0]
        page.goto(cfg["draft_url"], wait_until="domcontentloaded")
        time.sleep(4)  # 富编辑器初始化
        _log("page_opened", url=page.url)

        # 登录检测（manual 模式也做——没登录连手贴都贴不了）
        if _looks_logged_out(page, cfg):
            if not wait_for_login(page, cfg, cfg["draft_url"]):
                # 登录超时：降级 manual——剪贴板备好，窗口留着慢慢扫
                clipboard_assist(title, raw or text)
                _final("manual_assist",
                       "登录等待超时（%ds）。窗口保持打开，登录后正文可直接从剪贴板粘贴。" % LOGIN_WAIT_SECS)
                hold_window(ctx)
                return 0

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

        fr, el, el_sel = _find_in_frames(page, cfg["editor_selectors"])
        if not el:
            raise RuntimeError("没找到正文编辑器（选择器全部落空，可能后台改版）")

        ok, method, landed = inject_body(fr, el_sel, html, text)
        _log("body_injected", ok=ok, method=method, chars=landed)
        if not ok:
            raise RuntimeError("正文注入三级通道全部失败（落入 %d 字）" % landed)

        img_hints = []
        if images:
            pasted, img_hints = paste_images(page, fr, el_sel, images)

        clicked, confirmed = save_draft(page, cfg)
        _log("draft_saved", ok=clicked, confirmed=confirmed)

        # 标题没自动填上（如百家号 FeEditor 标题框定位不稳）：存完草稿后把标题送剪贴板兜底，
        # 让它成为剪贴板最后内容（不与正文/贴图的合成粘贴冲突），用户点标题栏一次 Ctrl+V 即可。
        title_clip = False
        if not title_ok:
            title_clip = bool(set_clipboard(title))
            _log("title_to_clipboard", ok=title_clip)

        no_draft_btn = not cfg.get("save_selectors")
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
        # 结果 JSON 已输出（上游可解析）；窗口保持到用户自己关——草稿已入库，关早关晚都安全
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
                    help="存完草稿即关窗退出（批量/AI 模式，便于同一账号连续发多篇）")
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
