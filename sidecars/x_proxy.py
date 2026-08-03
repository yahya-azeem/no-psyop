#!/usr/bin/env python3
"""
no-psyop x.com proxy.

A persistent headless-Chromium service (via Playwright) that drives the real
x.com web app and returns the JSON of the GraphQL operations it performs.
Plain HTTP/TLS clients (reqwest, curl) get 403 from x.com's bot protection, so
we let an actual browser runtime make the authenticated calls and capture the
API responses.

Endpoints (HTTP on 127.0.0.1:XPROXY_PORT or 8192):
  GET  /health            -> {"status":"ok"}
  POST /op                -> body: {"op":"feed"|"profile"|"inbox","username":"...","cookies":"auth_token=A; ct0=B; ..."}
                               returns: {"ok":true,"op":"<OperationName>","body":{...}} or {"ok":false,"error":"..."}

Requires: python3 with playwright + chromium installed.
"""

import json
import os
import queue
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from playwright.sync_api import sync_playwright

PORT = int(os.environ.get("XPROXY_PORT", "8192"))
HOME = "https://x.com/home"


def _cookie_list(cookie_str):
    out = []
    for pair in (cookie_str or "").split(";"):
        pair = pair.strip()
        if "=" not in pair:
            continue
        k, v = pair.split("=", 1)
        k, v = k.strip(), v.strip()
        if k in ("auth_token", "ct0"):
            out.append({"name": k, "value": v, "domain": ".x.com", "path": "/"})
    return out


class BrowserWorker(threading.Thread):
    """Owns the Playwright browser; runs operations sequentially."""

    def __init__(self, jobs):
        super().__init__(daemon=True)
        self.jobs = jobs
        self.ready = threading.Event()
        self.pw = None
        self.browser = None
        self.ctx = None
        self.page = None
        self._captured = {}

    def run(self):
        try:
            self.pw = sync_playwright().start()
            self.browser = self.pw.chromium.launch(
                headless=True,
                args=[
                    "--disable-blink-features=AutomationControlled",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                ],
            )
            self.ctx = self.browser.new_context(locale="en-US", timezone_id="UTC")
            self.page = self.ctx.new_page()
            self.ready.set()
            print("[x_proxy] browser ready", flush=True)
        except Exception as e:
            print(f"[x_proxy] browser init failed: {e}", flush=True)
            self.ready.set()
            return

        while True:
            item = self.jobs.get()
            if item is None:
                break
            op, payload, result = item
            try:
                result.put(("ok", self._dispatch(op, payload)))
            except Exception as e:
                result.put(("err", f"{type(e).__name__}: {e}"))

    def _dispatch(self, op, payload):
        cookie_str = payload.get("cookies", "")
        if op == "feed":
            return self._capture("https://x.com/home", ["HomeTimeline"], 15, cookie_str)
        if op == "profile":
            return self._capture(
                f"https://x.com/{payload.get('username', '')}", ["UserByScreenName"], 15, cookie_str
            )
        if op == "inbox":
            return self._capture(
                "https://x.com/messages", ["DMPinnedInboxQuery", "DmInboxTimeline"], 12, cookie_str
            )
        raise ValueError(f"unknown op {op}")

    def _capture(self, url, names, wait, cookie_str):
        if cookie_str:
            self.ctx.clear_cookies()
            self.ctx.add_cookies(_cookie_list(cookie_str))

        captured = {}
        pref = "/i/api/graphql/"

        def on_response(resp):
            u = resp.url
            if pref not in u:
                return
            tail = u.split(pref)[-1]
            name = tail.split("/")[1].split("?")[0] if "/" in tail else tail.split("?")[0]
            if name in captured:
                return
            if any(n in name for n in names) and resp.ok:
                try:
                    captured[name] = resp.json()
                except Exception:
                    pass

        self.page.on("response", on_response)
        self.page.goto(url, wait_until="domcontentloaded", timeout=60000)
        deadline = time.time() + wait
        while time.time() < deadline and not captured:
            self.page.mouse.wheel(0, 2000)
            self.page.wait_for_timeout(700)
        self.page.remove_listener("response", on_response)
        if not captured:
            return {"error": f"no response captured for {names} at {url}"}
        name, body = next(iter(captured.items()))
        return {"op": name, "body": body}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _send(self, obj, code=200):
        data = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path.rstrip("/") == "/health":
            self._send({"status": "ok"})
            return
        self._send({"error": "not found"}, 404)

    def do_POST(self):
        try:
            n = int(self.headers.get("Content-Length", "0"))
            payload = json.loads(self.rfile.read(n) or b"{}")
        except Exception:
            self._send({"error": "bad json"}, 400)
            return

        op = payload.get("op")
        if op not in ("feed", "profile", "inbox"):
            self._send({"error": "unknown op"}, 400)
            return

        result = queue.Queue()
        jobs.put((op, payload, result))
        try:
            status, value = result.get(timeout=90)
        except queue.Empty:
            self._send({"ok": False, "error": "timeout waiting for browser"}, 504)
            return
        if status == "err":
            self._send({"ok": False, "error": value})
            return
        if "error" in value:
            self._send({"ok": False, "error": value["error"]})
            return
        self._send({"ok": True, "op": value["op"], "body": value["body"]})


jobs = queue.Queue()
worker = BrowserWorker(jobs)
worker.start()

if __name__ == "__main__":
    worker.ready.wait(timeout=60)
    httpd = ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    print(f"[x_proxy] listening on 127.0.0.1:{PORT}", flush=True)
    httpd.serve_forever()