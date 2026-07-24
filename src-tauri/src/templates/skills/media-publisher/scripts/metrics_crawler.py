#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Polaris GEO · 机动化数据爬虫 metrics_crawler.py
======================================================

一句话职责：**登录态复用**地把「你自己在各平台创作者后台的数据」（阅读 / 点赞 / 评论 /
粉丝 / 播放 / 收益……）自动抓下来、落盘，喂给「数据复盘官」（content-analytics-report）。
看的是**自己的账号、自己的数据**，正当自用，不碰别人。

── 为什么这么设计（「机动化」的核心） ────────────────────────────────────────
后台数据面板全是 JS 渲染，DOM 选择器一改版就废。所以本爬虫**以拦截平台后台自己的
XHR / JSON 接口为主**——页面加载时它自己会 fetch 一堆 JSON，我们把响应体原样收下来，
比死磕 DOM 稳得多，也天然「机动」：
  · 加一个平台 = 往 CRAWL_TARGETS 加一条配置（登录 profile / CDP 端口都已就位）。
  · 加一个数据页 = 往该平台 views 里加一条 {url}。
  · 连配置都没有的页面 = 用 `--url X` 万能口，对任意页抓接口。
DOM 文本探针（dom）只作兜底，抓个概览大数字，抓不到也不影响接口捕获。

**捕获策略（2026-07 大修，起因：老版真跑一次 api_count=0）**——老版靠一张手写的
URL 片段白名单（`/statistic`、`/data`…）筛响应，是照着「接口大概长这样」猜的，从没对过
真流量：头条真正带数字的是 `/mp/agw/creator_center/list/v2`、`/mp/fe_api/home/merge_v2`，
一个片段都不命中 → 白名单全空转。而且老版只认 `content-type: json`，头条一半数据接口
发的是 `text/plain` 里装 JSON → 就算命中也被丢掉。现改为**默认全捕获 + 排除法**：
同源非静态响应一律收下、按内容判断是不是 JSON（不信 content-type），再用噪音名单剔掉
埋点/监控/推荐位。`capture` 退化为「额外强制包含」的提示，配错也不再致命。

**噪音过滤**——后台页面里到处是「不是你的数据」的数字：推荐话题热度（上亿阅读）、
创作学院课程播放量、活动参与人数、未读消息计数、粉丝等级门槛。老版 `_digest_metrics`
见数字就收，这些会直接污染 KPI（把别人的 10 亿阅读算成你的）。现按 URL 与键路径双层
denylist 剔除，并在 `normalized` 里只留可信的账号级字段。

── 复用的现成基建 ──────────────────────────────────────────────────────────
  · 浏览器：detached 真实 Chrome + `connect_over_cdp` 接管（与 draft_uploader /
    account_window 同配方、同端口）——直接接管你**已登录**的那个窗口，免重复扫码；
    脚本收尾只断连不关窗。
  · 登录态：每平台持久 profile（~/PolarisGEO/browser-profiles/{platform} 等）。
  · 落盘：data_store.save_record("metrics", ...) —— 与投递回执同库，数据复盘官直接能读；
    另存一份人类可读快照到 ~/PolarisGEO/data/metrics_latest/{platform}.json 方便自己看。

── 用法 ────────────────────────────────────────────────────────────────────
  # 抓某个平台所有已配置的数据页
  python metrics_crawler.py --platform toutiao
  # 抓多个 / 全部已配置平台（未登录的会报 need_login 并跳过，不中断其它）
  python metrics_crawler.py --platform toutiao,baijia,douyin
  python metrics_crawler.py --all
  # 只抓某平台的某几个数据页
  python metrics_crawler.py --platform douyin --views overview,contents
  # 万能口：对任意页面抓任意接口（未配置的平台 / 临时探查都用它）
  python metrics_crawler.py --platform zhihu --url "https://www.zhihu.com/creator" \
      --capture "/analytics,/statistics,/member"
  # 看有哪些平台 / 数据页可抓
  python metrics_crawler.py --list

输出协议：每步一行 JSON 进度 {"step":..,"ok":..}；每平台一行
{"result":"crawled"|"need_login"|"failed","platform":..,"api_count":..,"snapshot":..}；
最后一行 {"result":"done","platforms":N,"ok":N,"records":N}。
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request

# Windows 控制台默认 GBK，中文进度会乱码；统一 UTF-8。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

try:
    from playwright.sync_api import sync_playwright as _sync_pw  # type: ignore
except Exception:
    _sync_pw = None

# 落盘：与投递回执同一个 data_store（同库，数据复盘官直接能读）。缺失也不致命。
try:
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import data_store as _data_store  # type: ignore
except Exception:
    _data_store = None

HOME = os.path.expanduser("~")
_T0 = time.time()

# ───────────────────────── CDP 端口 / profile：与 account_window / draft_uploader 对齐 ─────────────────────────
CDP_BASE_PORT = int(os.environ.get("POLARIS_MEDIA_CDP_PORT", "9330"))
_CDP_FIXED = {"wechat": int(os.environ.get("POLARIS_MP_CDP_PORT", "9222"))}
_CDP_OFFSET = {"zhihu": 1, "toutiao": 2, "bilibili": 3, "baijia": 4, "douyin": 5,
               "csdn": 6, "juejin": 7, "xhs": 8}


def cdp_port(platform):
    if platform in _CDP_FIXED:
        return _CDP_FIXED[platform]
    return CDP_BASE_PORT + _CDP_OFFSET.get(platform, 9)


def default_profile_dir(platform):
    """与 account_window.default_profile_dir 一致，才能接管同一登录态窗口。"""
    if platform == "wechat":
        return os.path.join(HOME, ".polaris-mp-profile")
    if platform == "xhs":
        lad = os.environ.get("LOCALAPPDATA", HOME)
        return os.path.join(lad, "Google", "Chrome", "XiaohongshuProfiles", "default")
    return os.path.join(HOME, "PolarisGEO", "browser-profiles", platform)


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
    """脱离父进程启动 Chrome（Windows 加 CREATE_BREAKAWAY_FROM_JOB 脱离 Job Object），
    浏览器活过本脚本退出，窗口留给用户。与 draft_uploader._spawn_chrome_detached 同配方。"""
    args = [exe, "--remote-debugging-port=%d" % port, "--user-data-dir=%s" % profile,
            "--no-first-run", "--no-default-browser-check", "--start-maximized", "about:blank"]
    kw = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if os.name == "nt":
        DETACHED, NEW_GROUP, BREAKAWAY = 0x00000008, 0x00000200, 0x01000000
        try:
            return subprocess.Popen(args, creationflags=DETACHED | NEW_GROUP | BREAKAWAY, **kw)
        except OSError:
            return subprocess.Popen(args, creationflags=DETACHED | NEW_GROUP, **kw)
    return subprocess.Popen(args, start_new_session=True, **kw)


def connect_cdp(platform, profile):
    """detached Chrome + CDP 接管，返回 (ctx, browser, pw) 或 None（不可用则由上层回退）。"""
    if _sync_pw is None:
        return None
    exe = _chrome_exe()
    if not exe:
        return None
    port = cdp_port(platform)
    try:
        if not _cdp_version(port):
            os.makedirs(profile, exist_ok=True)
            _spawn_chrome_detached(exe, os.path.abspath(profile), port)
            for _i in range(30):
                time.sleep(0.5)
                if _cdp_version(port):
                    break
        if not _cdp_version(port):
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
        _log("cdp_attached", port=port, platform=platform)
        return ctx, browser, pw
    except Exception as e:
        _log("cdp_failed", ok=False, error=str(e).splitlines()[0][:120])
        return None


def _launch_local_chrome(profile):
    """回退：playwright 驱动本地 Chrome（channel=chrome）。失败必须 pw.stop() 避免泄漏
    asyncio loop（否则下次 start() 抛「Sync API inside the asyncio loop」）。返回 (ctx, None, pw)。"""
    if _sync_pw is None:
        return None
    pw = _sync_pw().start()
    try:
        ctx = pw.chromium.launch_persistent_context(
            profile, headless=False, channel="chrome", no_viewport=True,
            args=["--no-first-run", "--no-default-browser-check", "--start-maximized"])
    except Exception:
        try:
            pw.stop()
        except Exception:
            pass
        return None
    return ctx, None, pw


def open_context(platform, profile):
    """CDP 接管（缺省，接管已登录窗口、脚本退出不关窗）→ 回退本地 Chrome。返回 (ctx, browser, pw, engine)。"""
    os.makedirs(profile, exist_ok=True)
    forced = os.environ.get("POLARIS_BROWSER", "").lower()
    if forced not in ("cloak", "cloakbrowser"):
        got = connect_cdp(platform, profile)
        if got:
            return got[0], got[1], got[2], "cdp-chrome"
    got = _launch_local_chrome(profile)
    if got:
        return got[0], got[1], got[2], "local-chrome"
    raise RuntimeError("无可用浏览器引擎（需本地 Chrome + playwright）")


# ───────────────────────── 进度输出 ─────────────────────────
def _log(step, ok=True, **extra):
    rec = {"step": step, "ok": ok, "t": round(time.time() - _T0, 1)}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


def _final(result, **extra):
    rec = {"result": result}
    rec.update(extra)
    print(json.dumps(rec, ensure_ascii=False), flush=True)


# ───────────────────────── 爬取目标配置表（机动化：改这里即可增删平台 / 数据页）─────────────────────────
# 每个 view：
#   name    数据页别名
#   url     打开的页面（后台数据页）
#   capture 【可选】额外强制包含的 URL 片段。默认已「同源非静态全捕获」，这里只用来
#           把默认规则会漏掉的跨域数据接口拉回来——**不再是白名单，配空也照抓**。
#   settle  页面停留秒数（等它把接口都 fetch 完；懒加载页会配合滚动）
#   scroll  是否滚动页面触发懒加载接口
#   dom     兜底 DOM 文本探针 [{label, selector}]，抓概览大数字（抓不到不影响接口捕获）
#   verified 该平台配置是否对过真流量（true=跑通并见到真数字；false=按经验写、待验证）
def _profile(p):
    return default_profile_dir(p)


CRAWL_TARGETS = {
    "toutiao": {
        "name": "今日头条",
        "login_url_patterns": ["auth/page/login", "sso.toutiao.com", "/login"],
        "views": [
            {"name": "overview", "url": "https://mp.toutiao.com/profile_v4/index",
             "capture": ["/statistic", "/data", "/overview", "/index/", "/profit", "/income", "/fans"],
             "settle": 6, "scroll": True},
            {"name": "contents", "url": "https://mp.toutiao.com/profile_v4/graphic/articles",
             "capture": ["/article", "/content", "/feed", "/statistic", "/data"],
             "settle": 6, "scroll": True},
        ],
    },
    "baijia": {
        "name": "百家号",
        "login_url_patterns": ["builder/theme/bjh/login", "passport.baidu.com", "/login"],
        "views": [
            {"name": "overview", "url": "https://baijiahao.baidu.com/builder/rc/home",
             "capture": ["/data", "/statistic", "/appdata", "/overview", "/income", "/asset", "/fans"],
             "settle": 6, "scroll": True},
            {"name": "dataall", "url": "https://baijiahao.baidu.com/builder/rc/dataall",
             "capture": ["/data", "/statistic", "/analysis", "/content", "/trend"],
             "settle": 6, "scroll": True},
        ],
    },
    "douyin": {
        "name": "抖音",
        "login_url_patterns": ["creator.douyin.com/login", "/passport/", "sso.douyin.com"],
        "views": [
            {"name": "overview", "url": "https://creator.douyin.com/creator-micro/home",
             "capture": ["/data", "/statistic", "/overview", "/fans", "/janus", "/aweme"],
             "settle": 6, "scroll": True},
            {"name": "contents", "url": "https://creator.douyin.com/creator-micro/data-center/content",
             "capture": ["/data", "/aweme", "/statistic", "/content", "/janus"],
             "settle": 7, "scroll": True},
        ],
    },
    "zhihu": {
        "name": "知乎",
        "login_url_patterns": ["signin", "/login", "account"],
        "views": [
            {"name": "overview", "url": "https://www.zhihu.com/creator",
             "capture": ["/creator", "/analytics", "/statistics", "/member", "/data"],
             "settle": 6, "scroll": True},
            {"name": "analytics", "url": "https://www.zhihu.com/creator/analytics/content",
             "capture": ["/analytics", "/statistics", "/content", "/data"],
             "settle": 6, "scroll": True},
        ],
    },
    "bilibili": {
        "name": "B站",
        "login_url_patterns": ["passport.bilibili.com/login", "passport.bilibili.com"],
        "views": [
            {"name": "overview", "url": "https://member.bilibili.com/platform/home",
             "capture": ["/data", "/statistic", "/overview", "/nav", "/pcgateway", "/article", "/fans"],
             "settle": 6, "scroll": True},
            {"name": "datacenter", "url": "https://member.bilibili.com/platform/data-center/overview",
             "capture": ["/data", "/statistic", "/overview", "/trend", "/pcgateway"],
             "settle": 6, "scroll": True},
        ],
    },
    "xhs": {
        "name": "小红书",
        "login_url_patterns": ["/login", "creator.xiaohongshu.com/login"],
        "views": [
            {"name": "overview", "url": "https://creator.xiaohongshu.com/creator/home",
             "capture": ["/data", "/statistics", "/dashboard", "/galaxy", "/note", "/fans"],
             "settle": 6, "scroll": True},
            {"name": "analysis", "url": "https://creator.xiaohongshu.com/statistics/data-analysis",
             "capture": ["/data", "/statistics", "/analysis", "/galaxy", "/note"],
             "settle": 6, "scroll": True},
        ],
    },
    "wechat": {
        "name": "公众号",
        "login_url_patterns": ["/login", "bizlogin"],
        "views": [
            # 公众号数据页需 token 参数（登录后 URL 带 token），直接开首页让它自己跳；
            # 数字多为服务端渲染，接口捕获可能少 → 主要靠 dom 探针 + 万能口补。
            {"name": "home", "url": "https://mp.weixin.qq.com/",
             "capture": ["/cgi-bin", "/datacube", "statistics", "/data"],
             "settle": 6, "scroll": False},
        ],
    },
}


# ───────────────────────── 捕获筛选：默认全收 + 排除法 ─────────────────────────
# 静态资源：一律不收（体积大、无数据）。
_STATIC_EXT = (".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".ico",
               ".woff", ".woff2", ".ttf", ".mp4", ".mp3", ".map")
# 埋点 / 监控 / 风控域名与路径：只产生噪音，收下来白占体积。
_NOISE_URL_RE = re.compile(
    r"ibytedapm\.com|zijieapi\.com|bytegoofy\.com|bytescm\.com|bytetos\.com|byteimg\.com|"
    r"toutiaostatic\.com|toutiaoimg\.com|byte-gslb\.com|bytetcc\.com|mssdk\.|"
    r"/monitor_web/|/monitor_browser/|/slardar|/abtest|/sentry|/log/|/report/|"
    r"beacon|hm\.baidu\.com|google-analytics|doubleclick|"
    # ↓ 「别人的数据」板块：热榜 / 排行 / 活动 / 创作学院 / 推荐位。实测百家号首页的
    #   activity/ranking/articlelist?alias=week_hot 一条就带进 2200 万阅读——那是周热榜里
    #   别人的爆文，混进 KPI 会让「本周阅读」凭空多出七位数。
    r"/activity/|/ranking|/rank_|/hotlist|/hot_list|/leaderboard|/billboard|"
    r"college|/school/|/course|/material/center|/recommend|/discover|/square", re.I)
# 键路径噪音：**不是你的账号数据**的数字（推荐话题热度 / 学院课程播放 / 活动参与人数 /
# 未读消息 / 等级门槛 / 游标）。老版把这些一并算进 KPI，会把别人的上亿阅读记到自己头上。
_NOISE_KEY_RE = re.compile(
    r"forum|suggest|class_list|creator_college|activity_list|banner|notice|unread|"
    r"threshold|benefit|cursor|read_cursor|display_duration|level_num|part_num|"
    r"comment_status|comment_level|income_composition|hide_topcell|show_et_status|"
    r"recommend_list|participant|hot_list|rank_list|top_list", re.I)


def _should_capture(url, extra_patterns=None):
    """默认捕获：非静态、非埋点即收。extra_patterns 命中则强制收（覆盖排除规则）。"""
    low = (url or "").lower()
    if not low.startswith("http"):
        return False
    for p in (extra_patterns or []):
        if p and p.lower() in low:
            return True
    path = low.split("?", 1)[0]
    if path.endswith(_STATIC_EXT):
        return False
    if _NOISE_URL_RE.search(low):
        return False
    return True


# ───────────────────────── 通用「指标摘要」：从任意 JSON 里挖出像数据的数字，方便自己看 ─────────────────────────
_METRIC_KEY_RE = re.compile(
    r"read|view|play|like|digg|comment|share|repin|collect|fav|fan|follow|income|profit|"
    r"earn|impress|click|show|exposure|pv|uv|count|num|total|"
    r"阅读|播放|点赞|评论|转发|收藏|粉丝|关注|收益|收入|曝光|展现|点击|涨粉|新增", re.I)


def _digest_metrics(obj, prefix="", out=None, depth=0):
    """递归从 JSON 里抽「键名像指标、值是数字」的字段，做人类可读摘要。跨平台通用。

    过噪音名单：键**路径**（含祖先键名）命中 _NOISE_KEY_RE 的整枝丢弃——噪音多半挂在
    一个明显的容器键下（data.hot[].forum.read_count），按路径剪枝比按叶子键名准。"""
    if out is None:
        out = {}
    if depth > 6 or len(out) > 200:
        return out
    if isinstance(obj, dict):
        for k, v in obj.items():
            key = "%s.%s" % (prefix, k) if prefix else str(k)
            if _NOISE_KEY_RE.search(str(k)):
                continue
            if isinstance(v, (int, float)) and not isinstance(v, bool):
                if _METRIC_KEY_RE.search(str(k)):
                    out[key] = v
            else:
                _digest_metrics(v, key, out, depth + 1)
    elif isinstance(obj, list):
        for i, v in enumerate(obj[:20]):
            _digest_metrics(v, "%s[%d]" % (prefix, i), out, depth + 1)
    return out


# ───────────────────────── 规范化：把各平台野生 JSON 收敛成同一张表 ─────────────────────────
# 看板要的是「本账号的：粉丝 / 展现 / 阅读 / 点赞 / 评论 / 收益 / 作品数」，各平台键名各叫各的。
# 这里按**叶子键名**做同义词归并（不猜路径，改版也不容易废），再按作品聚合。
_CANON = {
    "fans": ["total_fans_count", "fans_count", "fansCount", "fans_num", "follower_count",
             "fansNum", "total_fans", "粉丝"],
    "impressions": ["impression_count", "showCount", "show_count", "exposure", "impression",
                    "RecommendCount", "recommend_count", "展现", "曝光"],
    "reads": ["go_detail_count", "readCount", "read_count", "view_count", "play_count",
              "PlayCount", "total_click", "click_count", "阅读", "播放"],
    "likes": ["digg_count", "diggCount", "DiggCount", "like_count", "likeCount",
              "total_like", "点赞"],
    "comments": ["comment_count", "commentCount", "CommentCount", "answer_count",
                 "total_reply", "reply_count", "评论"],
    "shares": ["repin_count", "repinCount", "share_count", "forward_count",
               "total_share", "total_fav", "fav_count", "转发", "收藏"],
    "income": ["total_income", "income", "Income", "收益", "收入"],
    "drafts": ["draft_count"],
}
# 别名表的维护方式：跑一次 `--platform X`，翻 metrics_latest/X.json 的 `metrics_digest`，
# 把该平台管这些数叫什么补进上面对应行。不要图省事写成模糊匹配（如凡含 "like" 皆算点赞）
# ——`dislike_count`、`like_threshold` 会一起进来。宁可一条条列，也别让错数据混进 KPI。
# 叶子键名 → 规范字段（小写匹配）
_CANON_LOOKUP = {}
for _canon_field, _aliases in _CANON.items():
    for _a in _aliases:
        _CANON_LOOKUP.setdefault(_a.lower(), _canon_field)


def _leaf_name(path):
    """'contents[0].article_stat.digg_count' → 'digg_count'"""
    return re.split(r"[.\[]", path)[-1].strip("]")


def _normalize(digest_map):
    """指标摘要 → 规范字段。同一规范字段出现多次：账号级取最大值（列表里是每篇，
    汇总取和会重复计入不同接口的同一批数据 → 保守取最大，宁少勿多）。"""
    buckets = {}
    for path, val in (digest_map or {}).items():
        canon = _CANON_LOOKUP.get(_leaf_name(path).lower())
        if not canon:
            continue
        buckets.setdefault(canon, []).append(val)
    out = {}
    for canon, vals in buckets.items():
        nums = [v for v in vals if isinstance(v, (int, float))]
        if not nums:
            continue
        # 账号级标量（粉丝/收益/草稿）取最大；作品级计数取和更有意义，但同一批数据可能
        # 被多个接口重复吐出 —— 这里统一给出 sum 与 max 两个口径，消费方自己选。
        out[canon] = {"sum": round(sum(nums), 2), "max": max(nums), "n": len(nums)}
    return out


# ───────────────────────── 登录检测 ─────────────────────────
def _looks_logged_out(page, patterns):
    try:
        url = (page.url or "").lower()
    except Exception:
        return False
    return any(p.lower() in url for p in (patterns or []))


# ───────────────────────── 单个 view 的抓取 ─────────────────────────
_MAX_PAYLOAD = 300 * 1024   # 单个接口响应体入库上限，超出只留顶层键名


def _attach_capture(page, patterns, hits):
    """挂响应监听：默认捕获（非静态、非埋点的 200 响应）→ 只**登记 Response 对象**，绝不在
    回调里读 body。sync playwright 在 response 事件回调里读 body 会静默失败（同线程重入），
    body 统一在导航结束后由 _drain_hits 在主流程里读——这是本爬虫踩过的真坑。

    patterns 现在只是「额外强制包含」，不再是白名单——老版白名单一条都没命中过真接口。"""

    def on_response(resp):
        try:
            if resp.status == 200 and _should_capture(resp.url, patterns):
                hits.append(resp)
        except Exception:
            pass

    page.on("response", on_response)


def _parse_json_body(resp):
    """按**内容**判断是不是 JSON，不信 content-type。

    头条一半数据接口（creator_center/list/v2、get_user_base_info…）发的是
    `text/plain; charset=utf-8` 里装 JSON；老版只认 content-type 含 json，
    这些全被丢掉 —— 这是 api_count=0 的第二个成因。"""
    txt = None
    try:
        txt = resp.text()
    except Exception:
        return None
    if not txt:
        return None
    s = txt.lstrip()
    if not s or s[0] not in "{[":
        return None
    try:
        return json.loads(s)
    except Exception:
        return None


def _drain_hits(hits):
    """导航结束后在主流程里读各命中响应的 body（此时安全），产出 {url: entry}。
    只保留「挖得出指标」的接口——默认全捕获会收到一堆配置/文案接口，全存进库没意义。"""
    sink = {}
    for resp in hits:
        try:
            url = resp.url
            body = _parse_json_body(resp)
            if body is None:
                continue
            metrics = _digest_metrics(body)
            if not metrics:
                continue          # 无指标的配置/文案接口：不入库
            raw = json.dumps(body, ensure_ascii=False)
            if len(raw) > _MAX_PAYLOAD:
                top = list(body.keys()) if isinstance(body, dict) else "<list %d>" % len(body)
                entry = {"url": url, "truncated": True, "top_keys": top, "metrics": metrics}
            else:
                entry = {"url": url, "data": body, "metrics": metrics}
            sink[url] = entry     # 同 URL 后到覆盖先到（拿最终态）
        except Exception:
            continue
    return sink


def _probe_dom(page, probes):
    out = {}
    for pr in (probes or []):
        try:
            el = page.query_selector(pr["selector"])
            if el:
                txt = (el.inner_text() or "").strip()
                if txt:
                    out[pr["label"]] = txt[:200]
        except Exception:
            continue
    return out


def crawl_view(ctx, view, login_patterns):
    """打开一个数据页、捕获接口、兜底探 DOM。返回 dict（含 need_login 标记时上层跳过该平台）。"""
    url = view["url"]
    capture = view.get("capture", [])
    settle = view.get("settle", 6)
    hits = []
    # 复用空白页；接管的窗口里可能开着别的标签，绝不抢占
    page = None
    for p in (getattr(ctx, "pages", None) or []):
        try:
            if (p.url or "about:blank") in ("about:blank", ""):
                page = p
                break
        except Exception:
            continue
    if page is None:
        page = ctx.new_page()

    _attach_capture(page, capture, hits)
    try:
        page.goto(url, wait_until="domcontentloaded", timeout=30000)
    except Exception as e:
        _log("goto_warn", ok=False, view=view["name"], error=str(e).splitlines()[0][:80])

    time.sleep(2)
    if _looks_logged_out(page, login_patterns):
        return {"need_login": True, "view": view["name"], "url": page.url}

    # 停留等接口 fetch 完；懒加载页滚一滚触发更多接口
    waited = 0
    while waited < settle:
        if view.get("scroll"):
            try:
                page.mouse.wheel(0, 1200)
            except Exception:
                pass
        time.sleep(1.5)
        waited += 1.5
    time.sleep(1)

    dom = _probe_dom(page, view.get("dom"))
    apis = list(_drain_hits(hits).values())
    # 汇总所有接口挖到的指标，做一个 view 级速览
    merged = {}
    for a in apis:
        merged.update(a.get("metrics") or {})
    normalized = _normalize(merged)
    _log("view_done", view=view["name"], api_count=len(apis),
         metric_fields=len(merged), canon_fields=len(normalized), dom_fields=len(dom))
    return {"view": view["name"], "url": url, "final_url": page.url,
            "api_count": len(apis), "apis": apis, "dom": dom,
            "metrics_digest": merged, "normalized": normalized}


# ───────────────────────── 落盘 ─────────────────────────
def _save(platform, view_result):
    if _data_store is None:
        return
    try:
        rec = {"source": "metrics_crawler", "kind": "account_metrics", "platform": platform}
        rec.update(view_result)
        _data_store.save_record("metrics", rec)
    except Exception as e:
        _log("save_warn", ok=False, error=str(e).splitlines()[0][:120])


# 账号级标量（跨 view 取最大，不求和——同一个数字每页都会出现一次）
_SCALAR_FIELDS = ("fans", "income", "drafts")


def _rollup(results):
    """多个 view 的 normalized → 一张平台级 summary（看板直接读这个）。

    作品级计数（阅读/展现/点赞…）在单个 view 内取 sum（那是「这批作品的合计」），
    跨 view 取 max（同一批作品在概览页和内容页会各报一次，求和会翻倍）。"""
    summary = {}
    for r in results:
        for field, agg in (r.get("normalized") or {}).items():
            val = agg.get("max") if field in _SCALAR_FIELDS else agg.get("sum")
            if val is None:
                continue
            summary[field] = max(summary.get(field, 0), val)
    return summary


def _write_snapshot(platform, name, results):
    """写一份人类可读的最新快照，方便自己直接打开看；`summary` 是给看板读的稳定契约。"""
    snap_dir = os.path.join(HOME, "PolarisGEO", "data", "metrics_latest")
    try:
        os.makedirs(snap_dir, exist_ok=True)
        path = os.path.join(snap_dir, platform + ".json")
        digest = {}
        for r in results:
            digest[r["view"]] = {
                "url": r.get("final_url") or r.get("url"),
                "api_count": r.get("api_count", 0),
                "metrics": r.get("metrics_digest", {}),
                "normalized": r.get("normalized", {}),
                "dom": r.get("dom", {}),
            }
        summary = _rollup(results)
        snap = {"platform": platform, "name": name,
                "crawled_at": time.strftime("%Y-%m-%d %H:%M:%S"),
                "crawled_ts": int(time.time()),
                # ok=false 表示「跑通了但一个指标都没抓到」——看板要能区分「没抓到」和
                # 「真的是 0」，否则一张空表看着像业务归零。
                "ok": bool(summary),
                "summary": summary,
                "views": digest}
        with open(path, "w", encoding="utf-8") as f:
            json.dump(snap, f, ensure_ascii=False, indent=2)
        return path
    except Exception as e:
        _log("snapshot_warn", ok=False, error=str(e).splitlines()[0][:120])
        return None


# ───────────────────────── 单平台驱动 ─────────────────────────
def crawl_platform(platform, views_filter=None, ad_hoc=None):
    cfg = CRAWL_TARGETS.get(platform)
    name = (cfg or {}).get("name", platform)
    login_patterns = (cfg or {}).get("login_url_patterns", ["/login"])

    if ad_hoc:                          # 万能口：临时页 + 临时 capture 模式
        views = [ad_hoc]
    else:
        views = (cfg or {}).get("views", [])
        if views_filter:
            want = set(v.strip() for v in views_filter)
            views = [v for v in views if v["name"] in want]
    if not views:
        _final("failed", platform=platform, detail="没有可抓的数据页（检查 --views / 配置）")
        return {"platform": platform, "ok": False, "records": 0}

    profile = _profile(platform)
    _log("platform_start", platform=platform, name=name, views=[v["name"] for v in views],
         profile=profile, port=cdp_port(platform))

    ctx = browser = pw = None
    try:
        ctx, browser, pw, engine = open_context(platform, profile)
        _log("browser_ready", platform=platform, engine=engine)
        results, records = [], 0
        for v in views:
            r = crawl_view(ctx, v, login_patterns)
            if r.get("need_login"):
                _final("need_login", platform=platform,
                       detail="检测到未登录，请先用「账号中心」扫码登录 %s 再抓。" % name,
                       url=r.get("url"))
                return {"platform": platform, "ok": False, "records": 0, "need_login": True}
            results.append(r)
            _save(platform, r)
            records += 1
        snap = _write_snapshot(platform, name, results)
        total_api = sum(r.get("api_count", 0) for r in results)
        summary = _rollup(results)
        # 「跑通但零指标」必须是可见的失败信号，不能报 crawled 了事——老版就是这样把
        # 一次空抓当成功，直到有人去翻快照才发现全是空的。
        _final("crawled" if summary else "empty", platform=platform, name=name,
               views=len(results), api_count=total_api, records=records,
               summary=summary, snapshot=snap,
               detail=None if summary else "跑通但未抓到任何指标：检查是否登录、或该平台配置待验证")
        return {"platform": platform, "ok": bool(summary), "records": records,
                "summary": summary}
    except Exception as e:
        _final("failed", platform=platform, detail=str(e).splitlines()[0][:160])
        return {"platform": platform, "ok": False, "records": 0}
    finally:
        # CDP 接管：只断连不关窗（窗口独立进程，留给用户）。回退引擎起的浏览器随 pw.stop 收掉。
        try:
            if pw is not None:
                pw.stop()
        except Exception:
            pass


# ───────────────────────── CLI ─────────────────────────
def _print_list():
    print("可抓平台 / 数据页（--platform / --views）：\n")
    for pid, cfg in CRAWL_TARGETS.items():
        vs = ", ".join(v["name"] for v in cfg.get("views", []))
        print("  %-9s %-6s 端口 %-5d  数据页: %s" % (pid, cfg["name"], cdp_port(pid), vs))
    print("\n万能口：--platform <任意> --url <页面> --capture <接口关键片段,逗号分隔>")
    print("落盘：~/PolarisGEO/data/collect/metrics/  快照：~/PolarisGEO/data/metrics_latest/")


def main():
    ap = argparse.ArgumentParser(description="机动化数据爬虫：抓自己账号后台数据并落盘")
    ap.add_argument("--platform", help="平台 id，逗号分隔；配合 --all 可省略")
    ap.add_argument("--all", action="store_true", help="抓所有已配置平台")
    ap.add_argument("--views", help="只抓这些数据页（逗号分隔，如 overview,contents）")
    ap.add_argument("--url", help="万能口：直接抓这个页面（需配 --capture）")
    ap.add_argument("--capture", help="额外强制捕获的接口 URL 片段（可选；默认已全捕获）")
    ap.add_argument("--settle", type=int, default=7, help="万能口页面停留秒数")
    ap.add_argument("--list", action="store_true", help="列出可抓平台 / 数据页")
    args = ap.parse_args()

    if args.list:
        _print_list()
        return
    if _sync_pw is None:
        _final("failed", detail="未安装 playwright（pip install playwright）")
        sys.exit(2)

    # 万能口（--capture 现在可选：默认已「同源非静态全捕获」）
    if args.url:
        platform = (args.platform or "adhoc").split(",")[0].strip()
        ad_hoc = {"name": "adhoc", "url": args.url,
                  "capture": [c.strip() for c in (args.capture or "").split(",") if c.strip()],
                  "settle": args.settle, "scroll": True}
        crawl_platform(platform, ad_hoc=ad_hoc)
        _final("done", platforms=1, ok=1, records=1)
        return

    if args.all:
        platforms = list(CRAWL_TARGETS.keys())
    elif args.platform:
        platforms = [p.strip() for p in args.platform.split(",") if p.strip()]
    else:
        _final("failed", detail="请指定 --platform / --all / --url（或 --list 看有哪些）")
        sys.exit(2)

    views_filter = [v.strip() for v in args.views.split(",")] if args.views else None
    ok_n, rec_n = 0, 0
    for p in platforms:
        if p not in CRAWL_TARGETS:
            _final("failed", platform=p, detail="未配置的平台，用 --url/--capture 万能口抓")
            continue
        res = crawl_platform(p, views_filter=views_filter)
        ok_n += 1 if res.get("ok") else 0
        rec_n += res.get("records", 0)
        time.sleep(1)   # 平台间隔，别抢系统资源
    _final("done", platforms=len(platforms), ok=ok_n, records=rec_n)


if __name__ == "__main__":
    main()
