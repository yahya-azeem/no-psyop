#!/usr/bin/env python3
"""
no-psyop x.com proxy.

A persistent headless-Chromium service (via Playwright) that drives the real
x.com web app and returns the JSON of the GraphQL operations it performs.
Plain HTTP/TLS clients (reqwest, curl) get 403 from x.com's bot protection, so
we let an actual browser runtime make the authenticated calls and capture the
API responses.

A persistent browser profile (per device, in the app data dir) stores the
X Chat passcode enrollment so the encrypted-DM inbox loads without the
Create-Passcode onboarding wall.

Endpoints (HTTP on 127.0.0.1:XPROXY_PORT or 8192):
  GET  /health            -> {"status":"ok"}
  POST /op                -> body: {"op":"feed"|"profile"|"inbox","username":"...","cookies":"auth_token=A; ct0=B; ..."}
  POST /op (linkedin)     -> body: {"op":"linkedin_login"}  (headed window; user signs in once)
                             body: {"op":"linkedin_feed"}   (scrape feed from the trusted profile)
                               returns: {"ok":true,"op":"...","body":{...}} or {"ok":false,"error":"..."}

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

DM_LIST_OPS = [
    "DMPinnedInboxQuery",
    "DmInboxTimeline",
    "DmAllSearchSlice",
    "ConversationTimeline",
    "InboxConversation",
]
DM_ANY_OPS = ["Dm", "Chat", "Conversation", "Inbox", "Message"]


def profile_dir():
    data = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
    return os.path.join(data, "no_pysop", "xchat_profile")


def linkedin_profile_dir():
    """Dedicated Chromium profile for LinkedIn.

    li_at cookies are device-bound: LinkedIn rejects the session when a brand
    new browser fingerprint presents it. This profile is where the user logs
    into LinkedIn once (headed), binding the session to *this* device, after
    which the DOM feed scraper can run reliably for as long as the session is
    valid.
    """
    data = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
    return os.path.join(data, "no_pysop", "linkedin_profile")


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


def _linkedin_cookie_list(cookie_str):
    """Seed every cookie from the passed session for LinkedIn.

    Direct-HTTP clients that authenticate with just li_at get rejected by
    LinkedIn's bot protection; issuing the *full* browser cookie set (li_at,
    JSESSIONID, bscookie, ...) on the real domain lets the browser pass.
    """
    out = []
    for pair in (cookie_str or "").split(";"):
        pair = pair.strip()
        if "=" not in pair:
            continue
        k, v = pair.split("=", 1)
        k, v = k.strip(), v.strip()
        if not k or not v:
            continue
        out.append({"name": k, "value": v, "domain": ".linkedin.com", "path": "/"})
    return out


class BrowserWorker(threading.Thread):
    """Owns the Playwright browser; runs operations sequentially.

    The persistent profile browser (which needs the full Chromium resident)
    is only launched on demand for the `inbox` op and is closed again once it
    has sat idle for a while. All other ops use short-lived ephemeral browsers
    that close themselves after each attempt, so a normal feed/login/search
    session never keeps a browser resident in memory.
    """

    # Seconds of `inbox` quiet before the persistent browser is torn down.
    PERSIST_IDLE = 120

    def __init__(self, jobs):
        super().__init__(daemon=True)
        self.jobs = jobs
        self.ready = threading.Event()
        self.ctx = None
        self.page = None
        self._last_ctx_use = 0.0
        self.li_ctx = None
        self.li_page = None
        self._last_li_ctx_use = 0.0

    def run(self):
        pw = None
        try:
            pw = sync_playwright().start()
        except Exception as e:
            print(f"[x_proxy] playwright init failed: {e}", flush=True)
            self.ready.set()
            return
        self.pw = pw
        self.ready.set()
        print(f"[x_proxy] playwright ready (no browser resident until needed)", flush=True)

        while True:
            try:
                item = self.jobs.get(timeout=2.0)
            except queue.Empty:
                self._reclaim_if_idle()
                continue
            if item is None:
                break
            op, payload, result = item
            try:
                result.put(("ok", self._dispatch(op, payload)))
            except Exception as e:
                result.put(("err", f"{type(e).__name__}: {e}"))
            self._reclaim_if_idle()
            self._reclaim_li_if_idle()

    def _ensure_persistent(self):
        """Launch the persistent-profile context on first use (inbox access)."""
        if self.ctx is None:
            self.ctx = self.pw.chromium.launch_persistent_context(
                profile_dir(),
                headless=True,
                args=[
                    "--disable-blink-features=AutomationControlled",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                ],
                locale="en-US",
                timezone_id="UTC",
            )
            self.page = self.ctx.new_page()
            print(f"[x_proxy] persistent browser launched (profile {profile_dir()})", flush=True)
        self._last_ctx_use = time.time()
        return self.page

    def _reclaim_if_idle(self):
        """Close the resident persistent browser when it has been idle too long."""
        if self.ctx is None:
            return
        if time.time() - self._last_ctx_use > self.PERSIST_IDLE:
            try:
                self.ctx.close()
            except Exception:
                pass
            self.ctx = None
            self.page = None
            print("[x_proxy] closed idle persistent browser", flush=True)

    def _fresh_page(self, ua=None):
        """A brand-new ephemeral context + page, seeded with request cookies.
        x.com blocks the long-lived profile; a tabula-risa context issued the
        same auth cookies reliably passes GraphQL. A caller may pass a real
        desktop `ua` to keep sites like LinkedIn from bot-flagging the default
        headless Chrome UA.
        """
        kwargs = {"locale": "en-US", "timezone_id": "UTC"}
        if ua:
            kwargs["user_agent"] = ua
        browser = self.pw.chromium.launch(
            headless=True,
            args=[
                "--disable-blink-features=AutomationControlled",
                "--no-sandbox",
                "--disable-dev-shm-usage",
            ],
        )
        ctx = browser.new_context(**kwargs)
        page = ctx.new_page()
        return browser, ctx, page

    def _ensure_linkedin_browser(self, headed=False):
        """A persistent, device-trusted Chromium profile for LinkedIn.

        LinkedIn device-binds the session cookie, so only a profile the user
        has actually logged into on *this* machine will be accepted. The first
        connection is `headed=True` so the user can complete the login; after
        that the profile holds the session and scraping is headless.
        """
        if self.li_ctx is None:
            self.li_ctx = self.pw.chromium.launch_persistent_context(
                linkedin_profile_dir(),
                headless=not headed,
                args=[
                    "--disable-blink-features=AutomationControlled",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                ],
                locale="en-US",
                timezone_id="UTC",
                user_agent=("Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                            "AppleWebKit/537.36 (KHTML, like Gecko) "
                            "Chrome/120.0.0.0 Safari/537.36"),
            )
            self.li_page = self.li_ctx.new_page()
            print(f"[x_proxy] linkedin persistent browser launched (profile {linkedin_profile_dir()})",
                  flush=True)
        self._last_li_ctx_use = time.time()
        return self.li_page

    def _reclaim_li_if_idle(self):
        """Close the resident LinkedIn profile browser when idle too long."""
        if self.li_ctx is None:
            return
        if time.time() - self._last_li_ctx_use > self.PERSIST_IDLE:
            try:
                self.li_ctx.close()
            except Exception:
                pass
            self.li_ctx = None
            self.li_page = None
            print("[x_proxy] closed idle linkedin browser", flush=True)

    def _dispatch(self, op, payload):
        cookie_str = payload.get("cookies", "")
        if op in ("feed", "profile", "user_tweets", "search"):
            last_err = None
            for _ in range(3):
                browser, ctx, page = self._fresh_page()
                if cookie_str:
                    ctx.add_cookies(_cookie_list(cookie_str))
                try:
                    if op == "feed":
                        result = self._capture(page, "https://x.com/home", ["HomeTimeline"], 15, scroll=True)
                    elif op == "profile":
                        result = self._capture(
                            page,
                            f"https://x.com/{payload.get('username', '')}",
                            ["UserByScreenName"],
                            15,
                        )
                    elif op == "search":
                        result = self._capture_search(
                            page,
                            payload.get("username", ""),
                        )
                    else:
                        result = self._capture_user_tweets(
                            page,
                            f"https://x.com/{payload.get('username', '')}",
                        )
                    if isinstance(result, dict) and "error" in result:
                        last_err = result["error"]
                        continue
                    return result
                except Exception as e:
                    last_err = e
                finally:
                    browser.close()
            raise ValueError(f"all fresh-browser attempts failed: {last_err or 'no response captured'}")
        if op == "linkedin_feed":
            last_err = None
            for attempt in range(2):
                page = self._ensure_linkedin_browser(headed=False)
                try:
                    result = self._scrape_linkedin_feed(page)
                except Exception as e:
                    last_err = str(e)
                    time.sleep(2)
                    continue
                if isinstance(result, dict) and "error" in result:
                    last_err = result["error"]
                    time.sleep(1 + attempt)
                    continue
                return result
            raise ValueError(last_err or "linkedin feed failed")
        if op == "linkedin_messages":
            page = self._ensure_linkedin_browser(headed=False)
            captured = {}
            pref = "/voyager/api/voyagerMessagingGraphQL/graphql"

            def on_resp(resp):
                if pref not in resp.url or not resp.ok:
                    return
                tail = resp.url.split("queryId=")[-1].split("&")[0]
                key = tail.split(".")[0]
                if key.lower().startswith("messengerconversations"):
                    try:
                        captured["body"] = resp.json()
                    except Exception:
                        pass

            page.on("response", on_resp)
            try:
                page.goto("https://www.linkedin.com/messaging/", wait_until="domcontentloaded", timeout=60000)
            except Exception:
                pass
            page.wait_for_timeout(8000)
            page.remove_listener("response", on_resp)
            if "body" not in captured:
                return {"error": "no messengerConversations response captured"}
            return {"op": "linkedin_messages", "body": captured["body"]}
            diag = page.evaluate("""() => {
              const out = { title: document.title, url: location.href, imgs: document.querySelectorAll('img').length };
              const anchor = document.querySelector('a[href*="/in/"]');
              if (anchor) {
                let el = anchor; const chain = [];
                for (let i = 0; el && i < 12; i++, el = el.parentElement) {
                  chain.push({ tag: el.tagName, cls: (el.className||'').toString().slice(0,140), id: el.id || null, role: el.getAttribute('role')||null });
                }
                out.chain = chain;
                out.authorText = (anchor.textContent||'').trim().slice(0,60);
                out.actorHref = anchor.getAttribute('href');
              } else { out.chain = null; }
              const classes = new Set();
              document.querySelectorAll('[class]').forEach(el => (el.className||'').toString().split(/\\s+/).forEach(c => { if(/update|feed|actor|commentary|social-actions|card/i.test(c)) classes.add(c); }));
              out.feedClasses = [...classes].slice(0,50);
              out.actAnchors = document.querySelectorAll('a[href*="activity"]').length;
              out.perm = (document.querySelector('a[href*="activity"]')||{}).getAttribute ? document.querySelector('a[href*="activity"]').getAttribute('href') : null;
              out.postAnchors = document.querySelectorAll('a[href*="/posts/"]').length;
              out.btnAria = [...new Set([...document.querySelectorAll('[aria-label]')].map(e => (e.getAttribute('aria-label')||'').slice(0,30)))].slice(0,15);
              out.dataAttrs = Object.keys((document.querySelector('a[href*="activity"]')||{}).dataset || {});
              out.dtTime = document.querySelectorAll('time').length;
              out.dtHidden = document.querySelectorAll('time, [class*="time"]').length;
              return out;
            }""")

            hits = []

            def on_resp(resp):
                if len(hits) >= 60:
                    return
                u = resp.url
                if any(k in u for k in ("graphql", "feed", "srpc", "suiteFacingBff", "restli")):
                    try:
                        ct = resp.headers.get("content-type", "")
                        hits.append({
                            "url": u[:150],
                            "type": resp.request.resource_type,
                            "ct": ct[:40],
                            "status": resp.status,
                        })
                    except Exception:
                        pass

            page.on("response", on_resp)
            try:
                page.reload(wait_until="domcontentloaded", timeout=60000)
            except Exception:
                pass
            page.wait_for_timeout(8000)
            page.remove_listener("response", on_resp)
            diag["network"] = hits
            posts = page.evaluate("""() => {
                const out = [];
                // author links on a post card's own line
                const authorAnchors = document.querySelectorAll('a[href*="/posts/"]');
                const seen = new Set();
                for (const a of authorAnchors) {
                  if (out.length >= 25) break;
                  // climb to the card = the ancestor containing author + text + actions
                  let el = a;
                  let card = null;
                  for (let i = 0; el && i < 10; i++, el = el.parentElement) {
                    if (el.tagName === 'DIV' && el.querySelector('a[href*="/posts/"]') && el.querySelector('button, img')) {
                      card = el; break;
                    }
                  }
                  if (!card) continue;
                  const card2 = card.closest('div[class*=" "]'); 
                  let node = card;
                  for (let i = 0; node && i < 6; i++, node = node.parentElement) {
                    if (node.tagName === 'DIV' && node.querySelectorAll('a[href*="/posts/"]').length >= 2) { break; }
                    card = node;
                  }
                  const urn = (card.querySelector('a[href*="/posts/"]')||{}).href || '';
                  if (urn && seen.has(urn)) continue;
                  if (urn) seen.add(urn);
                  const nameLink = card.querySelector('a[href*="/in/"]');
                  const author = nameLink ? (nameLink.textContent||'').trim() : '';
                  const txtEl = card.querySelector('[class*="--anything"]') || card;
                  let text = '';
                  const menc = card.querySelectorAll('div');
                  for (const d of menc) {
                    if (d.childElementCount === 0 && d.textContent.trim().length > 40) { text = d.textContent.trim(); break; }
                  }
                  if (!text) {
                    const paragraphs = card.querySelectorAll('p, span');
                    for (const p of paragraphs) { if (p.childElementCount===0 && p.textContent.trim().length>25) { text=p.textContent.trim(); break; } }
                  }
                  const img = card.querySelector('img[src*="media"]');
                  const video = card.querySelector('video');
                  out.push({ author: author, urn: urn, text: text.slice(0,200), img: img ? img.getAttribute('src') : null, vid: video ? true : false, cls: card.className.toString().slice(0,90) });
                }
                return { n: out.length, posts: out };
            }""")
            diag["extract"] = posts
            anchors = page.evaluate("""() => {
              const counts = {};
              const patterns = [/feed\\/update/, /urn:li:activity/, /\\/posts\\//, /\\/in\\//, /recent-activity/];
              for (const p of patterns) {
                const els = [...document.querySelectorAll('a[href]')].filter(a => p.test(a.getAttribute('href')));
                counts[p.source] = els.length;
              }
              const inLinks = [...document.querySelectorAll('a[href*=\"/in/\"]')].slice(0,8).map(a => ({ href: (a.getAttribute('href')||'').slice(0,70), text: (a.textContent||'').trim().slice(0,40), cls: (a.className||'').toString().slice(0,60) }));
              const updLinks = [...document.querySelectorAll('a[href*=\"/feed/update/\"]')].slice(0,5).map(a => ({ href: (a.getAttribute('href')||'').slice(0,90), text: (a.textContent||'').trim().slice(0,30) }));
              const imgs = [...document.querySelectorAll('img[src*=\"media\"]')].slice(0,5).map(i => (i.getAttribute('src')||'').slice(0,90));
              return { counts, inLinks, updLinks, imgs };
            }""")
            diag["anchors"] = anchors
            extr = page.evaluate("""() => {
                const cards = [];
                const seen = new Set();
                const perms = document.querySelectorAll('a[href*="/posts/"]');
                for (const perm of perms) {
                  const href = (perm.getAttribute('href') || '');
                  if (seen.has(href)) continue;
                  seen.add(href);
                  let el = perm; let card = null;
                  // climb to the post card: an ancestor containing the permalink AND a name link, but not the whole column
                  for (let i = 0; el && i < 10; i++, el = el.parentElement) {
                    const nL = el.querySelector('a[href*="/in/"]');
                    if (nL && (nL.textContent||'').trim().length > 2) {
                      const hasPostCol = el.parentElement && el.parentElement.querySelectorAll('a[href*="/posts/"]').length >= 2;
                      if (!hasPostCol) { card = el; break; }
                    }
                  }
                  if (!card) continue;
                  const nL = card.querySelector('a[href*="/in/"]');
                  const author = (nL.textContent || '').trim();
                  // post body: longest standalone text block
                  let text = '';
                  let best = 0;
                  for (const t of card.querySelectorAll('div, span, p')) {
                    if (t.childElementCount === 0) {
                      const s = (t.textContent || '').trim();
                      if (s.length > best) { best = s.length; text = s; }
                    }
                  }
                  const degM = author.match(/•\\s*([0-9a-z]+)/i);
                  const degree = degM ? degM[1] : null;
                  const imgs = [...card.querySelectorAll('img[src*="media"]')]
                    .map(i => i.getAttribute('src') || '')
                    .filter(s => !/profile-displayphoto|company-logo|profile-displaybackground/i.test(s));
                  const vEl = card.querySelector('video');
                  const vid = !!vEl || !!card.querySelector('img[class*="video"]') || !!card.querySelector('[data-test-id*="video"]');
                  const tsEl = card.querySelector('time');
                  const ts = tsEl ? tsEl.getAttribute('datetime') : (card.querySelector('[class*="time"]')||{}).textContent || null;
                  cards.push({ author, degree, href, text: text.slice(0,200), is_video: vid, img: imgs[0] || null, imgs: imgs.length, ts });
                }
                return { n: cards.length, cards };
            }""")
            diag["extr"] = extr
            return {"op": "linkedin_debug", "body": diag}
        if op == "linkedin_login":
            page = self._ensure_linkedin_browser(headed=True)
            try:
                page.goto("https://www.linkedin.com/", wait_until="domcontentloaded", timeout=60000)
            except Exception:
                pass
            deadline = time.time() + 240
            has_session = False
            while time.time() < deadline:
                page.wait_for_timeout(1500)
                try:
                    cookies = self.li_ctx.cookies()
                    has_session = any(c["name"] == "li_at" for c in cookies)
                except Exception:
                    pass
                if has_session:
                    break
            cookies = []
            try:
                cookies = self.li_ctx.cookies()
            except Exception:
                pass
            if not has_session:
                raise ValueError("linkedin login not detected within timeout")
            session = "; ".join(f"{c['name']}={c['value']}" for c in cookies)
            return {"op": "linkedin_login", "body": {"session_token": session}}
        if op == "inbox":
            page = self._ensure_persistent()
            if cookie_str:
                self.ctx.add_cookies(_cookie_list(cookie_str))
            return self._inbox(page)
        raise ValueError(f"unknown op {op}")

    def _capture(self, page, url, names, wait, scroll=False):
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

        page.on("response", on_response)
        attempts = 0
        while attempts < 3 and not captured:
            attempts += 1
            try:
                page.goto(url, wait_until="domcontentloaded", timeout=60000)
            except Exception:
                pass
            deadline = time.time() + wait
            while time.time() < deadline and not captured:
                if scroll:
                    page.mouse.wheel(0, 2000)
                page.wait_for_timeout(700)
            if not captured:
                print(
                    f"[x_proxy] attempt {attempts} failed url={page.url} title={page.title()}",
                    flush=True,
                )
        page.remove_listener("response", on_response)
        if not captured:
            return {"error": f"no response captured for {names} at {url}"}
        name, body = next(iter(captured.items()))
        return {"op": name, "body": body}

    def _capture_user_tweets(self, page, url):
        """Collect a user's profile timeline tweets, scrolling to pull in more
        (incl. media-bearing) posts beyond the first lazy `UserTweets` page.
        Merges all captured `UserTweets` responses into one timeline body.
        """
        pref = "/i/api/graphql/"
        responses = []

        def on_response(resp):
            u = resp.url
            if pref not in u:
                return
            tail = u.split(pref)[-1]
            name = tail.split("/")[1].split("?")[0] if "/" in tail else tail.split("?")[0]
            if name != "UserTweets" or not resp.ok:
                return
            try:
                responses.append(resp.json())
            except Exception:
                pass

        page.on("response", on_response)
        page.goto(url, wait_until="domcontentloaded", timeout=60000)
        deadline = time.time() + 15
        while time.time() < deadline and not responses:
            page.wait_for_timeout(700)
        more = time.time() + 14
        while time.time() < more and len(responses) < 5:
            page.mouse.wheel(0, 3000)
            page.wait_for_timeout(900)
        page.remove_listener("response", on_response)
        if not responses:
            return {"error": f"no response captured for ['UserTweets'] at {url}"}

        merged = json.loads(json.dumps(responses[0]))
        seen = set()
        combined = []
        for r in responses:
            for ib in r["data"]["user"]["result"]["timeline"]["timeline"].get("instructions", []):
                for entry in ib.get("entries", []):
                    eid = entry.get("content", {}).get("entryId", "")
                    if eid and eid in seen:
                        continue
                    if eid:
                        seen.add(eid)
                    combined.append(entry)
        timeline = merged["data"]["user"]["result"]["timeline"]["timeline"]
        timeline["instructions"] = [{"type": "TimelineAddEntries", "entries": combined}]
        return {"op": "UserTweets", "body": merged}

    def _capture_search(self, page, query):
        """Collect live search results for `query`. Drives the x.com search
        page (`f=live` tab) and merges all captured `SearchTimeline` responses
        into one timeline body, mirroring `_capture_user_tweets`.
        """
        from urllib.parse import quote
        url = "https://x.com/search?q={}&f=live".format(quote(query))
        pref = "/i/api/graphql/"
        responses = []

        def on_response(resp):
            u = resp.url
            if pref not in u:
                return
            tail = u.split(pref)[-1]
            name = tail.split("/")[1].split("?")[0] if "/" in tail else tail.split("?")[0]
            if name != "SearchTimeline" or not resp.ok:
                return
            try:
                responses.append(resp.json())
            except Exception:
                pass

        page.on("response", on_response)
        page.goto(url, wait_until="domcontentloaded", timeout=60000)
        deadline = time.time() + 15
        while time.time() < deadline and not responses:
            page.wait_for_timeout(700)
        more = time.time() + 14
        while time.time() < more and len(responses) < 5:
            page.mouse.wheel(0, 3000)
            page.wait_for_timeout(900)
        page.remove_listener("response", on_response)
        if not responses:
            return {"error": f"no response captured for ['SearchTimeline'] at {url}"}

        merged = json.loads(json.dumps(responses[0]))
        seen = set()
        combined = []
        for r in responses:
            timeline = r["data"]["search_by_raw_query"]["search_timeline"]["timeline"]
            for entry in timeline.get("instructions", []):
                for e in entry.get("entries", []):
                    eid = e.get("content", {}).get("entryId", "")
                    if eid and eid in seen:
                        continue
                    if eid:
                        seen.add(eid)
                    combined.append(e)
        merged["data"]["search_by_raw_query"]["search_timeline"]["timeline"]["instructions"] = [
            {"type": "TimelineAddEntries", "entries": combined}
        ]
        return {"op": "SearchTimeline", "body": merged}

    def _scrape_linkedin_feed(self, page):
        """Load LinkedIn's home feed in the trusted profile browser and read the
        rendered post cards off the DOM.

        LinkedIn's current frontend is server-driven (SDUI): it has no stable
        feed GraphQL endpoint and its CSS class names are hashed, so we anchor
        on the semantic author links (`a[href*=\"/in/\"]`) that carry the author
        name *and* the connection badge (\"Name  • 1st\", \"Name  • Following\").
        The device-trusted persistent profile is what authenticates; direct-HTTP
        and brand-new browser fingerprints get 429/redirect-looped.
        """
        url = "https://www.linkedin.com/feed/"
        # A session can be briefly challenged with a blank / redirect page. Don't
        # let page.goto block for 60s per attempt; keep navigation short, detect
        # the unloaded page, reload once, then bail out quickly instead of hanging.
        for attempt in range(2):
            try:
                page.goto(url, wait_until="domcontentloaded", timeout=20000)
            except Exception:
                pass
            page.wait_for_timeout(2500)
            if not page.title() or "feed" not in (page.url or ""):
                if attempt == 0:
                    try:
                        page.goto(url, wait_until="domcontentloaded", timeout=15000)
                    except Exception:
                        pass
                    page.wait_for_timeout(3000)
                if not page.title() or "feed" not in (page.url or ""):
                    continue
            posts = self._collect_linkedin_posts(page)
            if posts:
                return {"op": "linkedin_feed", "body": {"posts": posts}}
        return {"error": f"no linkedin feed posts scraped at {url} (page.title={page.title()!r})"}

    def _collect_linkedin_posts(self, page):
        deadline = time.time() + 15
        posts = []
        while time.time() < deadline:
            try:
                posts = self._eval_linkedin_posts(page)
            except Exception:
                return []
            if len(posts) >= 4:
                break
            try:
                page.mouse.wheel(0, 2600)
            except Exception:
                pass
            page.wait_for_timeout(1000)
        return posts

    def _eval_linkedin_posts(self, page):
        js = """
        () => {
          const nameLinks = [...document.querySelectorAll('a[href*="/in/"]')]
            .filter(a => (a.textContent || '').trim().length > 2);
          const cards = [];
          const seen = new Set();
          for (const a of nameLinks) {
            const href = (a.getAttribute('href') || '');
            if (seen.has(href)) continue;
            seen.add(href);
            let el = a;
            let card = null;
            // climb to the full post card: keep climbing as long as the
            // container still looks like a single card (few /in/ links). Stop
            // once it balloons into the feed column / suggestion rail (a
            // handful of cards), and use the deepest single-card element.
            for (; el && el.parentElement; ) {
              el = el.parentElement;
              const inLinks = el.querySelectorAll('a[href*="/in/"]').length;
              if (inLinks >= 8) break;
              if (inLinks > 0) card = el;
            }
            if (!card) continue;
            const author = (a.textContent || '').trim().split(/\\u2022|\\n/)[0].replace(/\\s+/g, ' ').trim();
            const badge = (a.textContent || '').match(/\\u2022\\s*(1st|2nd|3rd|Following|Followed)/i);
            const badgeText = badge ? badge[1] : '';
            const isConnection = /1st|following|followed/i.test(badgeText);
            // clean handle: strip the /in/ prefix and any locale suffix
            const slug = ((href.split('/in/')[1] || href).split('?')[0].split('/')[0] || '').replace(/[\\/\\s]+$/g, '');
            // post body: the actor header ends at the "…"/Follow row near the
            // top; anything after that, before the reaction counts, is the real
            // caption. The author bio/headline lives INSIDE the header, so it is
            // dropped here (there is no bio shown for corporate posts).
            const blocks = [];
            for (const el of card.querySelectorAll('div, span, p')) {
              if (el.childElementCount > 0) continue;
              const s = (el.textContent || '').trim();
              if (s) blocks.push(s);
            }
            let text = '';
            let inHeader = true;
            for (const t of blocks) {
              const low = t.toLowerCase();
              if (inHeader) {
                if (low === 'follow' || low === 'more' || t === '\\u2026' ||
                    /^(open control|hide|report|copy link)/.test(low)) { inHeader = false; }
                continue;
              }
              if (/^[\\u200b\\u200c\\u200d\\s]+$/.test(t)) continue;
              if (/^\\d+\\/\\d+$/.test(t) || /reactions?$/i.test(low)) continue;
              if (['like', 'comment', 'repost', 'share', 'send', 'follow', 'more', '\\u2026'].includes(low)) continue;
              if (t.length > text.length) text = t;
            }
            // media (exclude avatar / logo / profile crops)
            const imgs = [...card.querySelectorAll('img[src*="media"]')]
              .map(i => i.getAttribute('src') || '')
              .filter(s => !/profile-displayphoto|profile-framedphoto|profile-displaybackground|company-logo|profile-display/i.test(s) && /media\\.licdn.com\\/dms|media\\/dms/i.test(s));
            const video = card.querySelector('video');
            const isVideo = !!video || !!card.querySelector('img[src*="video"]') || !!card.querySelector('[class*="video"] img');
            const tsEl = card.querySelector('time');
            let timestamp = 0;
            if (tsEl) {
              const d = Date.parse(tsEl.getAttribute('datetime'));
              if (!isNaN(d)) timestamp = Math.floor(d / 1000);
            }
            // real LinkedIn feed updates render with a leading "Feed post"
            // screen-reader label; profile shells / "people you may know"
            // / suggestion cards do not. Skip anything that isn't a real post.
            const cardText = (card.textContent || '');
            if (cardText.indexOf('Feed post') === -1) continue;
            // a post must have media OR an actual caption; drop empty leftovers
            if (!imgs.length && !video && !text) continue;
            const id = href || (author + '|' + text.slice(0, 60));
            cards.push({ id: id, author: author, username: slug, href: href, text: text, media: imgs, is_video: isVideo, is_connection: isConnection, timestamp: timestamp });
          }
          // shared posts appear once per actor link - collapse to a single card
          // keyed on first media URL (or body text when text-only).
          const uniq = new Map();
          for (const c of cards) {
            const key = (c.media[0] || c.text.slice(0, 120)).toLowerCase();
            if (!uniq.has(key)) uniq.set(key, c);
          }
          return [...uniq.values()];
        }
        """
        try:
            return page.evaluate(js)
        except Exception:
            return []

    def _inbox(self, page):
        self._last_ctx_use = time.time()
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
            if (any(n in name for n in DM_LIST_OPS) or any(n in name for n in DM_ANY_OPS)) and resp.ok:
                try:
                    captured[name] = resp.json()
                except Exception:
                    pass

        page.on("response", on_response)
        page.goto("https://x.com/messages", wait_until="domcontentloaded", timeout=60000)
        deadline = time.time() + 15
        while time.time() < deadline and not captured:
            page.mouse.wheel(0, 2000)
            page.wait_for_timeout(700)
        page.remove_listener("response", on_response)
        if captured:
            name, body = next(iter(captured.items()))
            return {"op": name, "body": body}

        body_text = ""
        try:
            body_text = page.inner_text("body")
        except Exception:
            pass
        if "Empty inbox" in body_text or "Start Conversation" in body_text:
            return {"op": "empty_inbox", "body": {"empty_inbox": True}}
        return {"error": "no DM response captured and inbox not detected"}


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
        if op not in ("feed", "profile", "user_tweets", "search", "linkedin_feed", "linkedin_login", "linkedin_messages", "linkedin_debug", "inbox"):
            self._send({"error": "unknown op"}, 400)
            return

        result = queue.Queue()
        jobs.put((op, payload, result))
        try:
            status, value = result.get(timeout=300)
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