# -*- coding: utf-8 -*-
"""
仿 ChatGPT 的 AI 对话网站 —— 零依赖后端（仅用 Python 标准库）。

职责：
  1. 把 index.html 提供给浏览器
  2. 把 /api/chat 的请求代理到 Anthropic 兼容的 Kimi API（key 藏在后端，不暴露给前端）
  3. 流式（streaming）把模型回复一段段吐回前端

运行：
  python server.py
然后浏览器打开 http://127.0.0.1:8000
"""

import json
import os
import urllib.request
import urllib.error
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# ---- 配置：可用环境变量覆盖 ----------------------------------------------
API_BASE = os.environ.get("ANTHROPIC_BASE_URL", "https://api.kimi.com/coding/").rstrip("/")
API_TOKEN = os.environ.get(
    "ANTHROPIC_AUTH_TOKEN",
    "sk-kimi-9WLHZfUxcpzi2l9AsTqMatBYOzpHoCtOsxIVFuyRIlQIPDz76p0aoXLjSHuVw4pu",
)
MODEL = os.environ.get("MODEL", "opus")
MAX_TOKENS = int(os.environ.get("MAX_TOKENS", "4096"))
HOST = os.environ.get("HOST", "127.0.0.1")
PORT = int(os.environ.get("PORT", "8000"))

HERE = os.path.dirname(os.path.abspath(__file__))
SYSTEM_PROMPT = "你是一个乐于助人的 AI 助手，用简洁、结构化的中文回答。"


def call_upstream_stream(messages):
    """调用上游 Anthropic 兼容接口，逐块 yield 文本。"""
    url = API_BASE + "/v1/messages"
    payload = {
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "system": SYSTEM_PROMPT,
        "messages": messages,
        "stream": True,
    }
    data = json.dumps(payload).encode("utf-8")
    headers = {
        "content-type": "application/json",
        "anthropic-version": "2023-06-01",
        "x-api-key": API_TOKEN,
        "authorization": "Bearer " + API_TOKEN,
    }
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        resp = urllib.request.urlopen(req, timeout=300)
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8", "replace")
        yield "[上游错误 %s] %s" % (e.code, body)
        return
    except Exception as e:  # noqa
        yield "[连接失败] %s" % e
        return

    # 逐行解析 SSE：只取 content_block_delta 的文本增量
    for raw in resp:
        line = raw.decode("utf-8", "replace").strip()
        if not line.startswith("data:"):
            continue
        chunk = line[len("data:"):].strip()
        if not chunk or chunk == "[DONE]":
            continue
        try:
            evt = json.loads(chunk)
        except json.JSONDecodeError:
            continue
        if evt.get("type") == "content_block_delta":
            delta = evt.get("delta", {})
            text = delta.get("text") or delta.get("partial_json") or ""
            if text:
                yield text
        elif evt.get("type") == "error":
            yield "[模型错误] %s" % json.dumps(evt.get("error", {}), ensure_ascii=False)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):  # 静默默认日志
        pass

    def _send_file(self, path, ctype):
        try:
            with open(path, "rb") as f:
                body = f.read()
        except FileNotFoundError:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path in ("/", "/index.html"):
            self._send_file(os.path.join(HERE, "index.html"), "text/html; charset=utf-8")
        else:
            self.send_error(404)

    def do_POST(self):
        if self.path != "/api/chat":
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", "0"))
        try:
            req = json.loads(self.rfile.read(length).decode("utf-8"))
            messages = req.get("messages", [])
        except Exception:
            self.send_error(400, "bad json")
            return

        # 以 chunked 纯文本流式回传，前端直接读取拼接
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("X-Accel-Buffering", "no")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        try:
            for piece in call_upstream_stream(messages):
                data = piece.encode("utf-8")
                self.wfile.write(b"%X\r\n%s\r\n" % (len(data), data))
                self.wfile.flush()
            self.wfile.write(b"0\r\n\r\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    srv = ThreadingHTTPServer((HOST, PORT), Handler)
    print("ChatGPT-style 对话网站已启动：http://%s:%d" % (HOST, PORT))
    print("模型：%s   上游：%s" % (MODEL, API_BASE))
    print("按 Ctrl+C 停止。")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        print("\n已停止。")


if __name__ == "__main__":
    main()
