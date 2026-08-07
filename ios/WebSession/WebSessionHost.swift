//
//  WebSessionHost.swift
//  OneMedia — iOS host for the in-app platform scraper.
//
//  The app opens a dedicated WKWebView on the *platform's own origin*
//  (x.com, instagram.com, linkedin.com). Because it runs on that origin with a
//  real logged-in session, the scraper (`web-session.js`, built from
//  src/lib/scrape via `bun run build:session`) can fetch the platform's own
//  JSON endpoints without any API key or CORS workaround.
//
//  Flow:
//    1. login(to:)  -> the user signs in once on the platform.
//    2. Cookies persist in the WKWebView default data store.
//    3. scrape(_:_:completion:) -> evaluateJavaScript drives the injected
//       `window.__onemedia.scrape(...)`; the envelope returns over a
//       WKScriptMessageHandler ("onemedia"). No APIs, scrapers only.
//
//  Note: requires WebKit/Cocoa — compiles only inside the iOS target.
import Foundation
import WebKit

public enum WebSessionPlatform: String, CaseIterable {
    case twitter = "Twitter"
    case instagram = "Instagram"
    case linkedin = "LinkedIn"

    public var origin: String {
        switch self {
        case .twitter: return "https://x.com"
        case .instagram: return "https://www.instagram.com"
        case .linkedin: return "https://www.linkedin.com"
        }
    }

    public var loginPath: String {
        switch self {
        case .twitter: return "/login"
        case .instagram: return "/"
        case .linkedin: return "/login"
        }
    }
}

/// Receives envelopes captured by the scraper while it runs inside the WebView.
public protocol WebSessionDelegate: AnyObject {
    func session(_ host: WebSessionHost, didCapture envelope: [String: Any], for platform: WebSessionPlatform)
    func session(_ host: WebSessionHost, didFail platform: WebSessionPlatform, error: String)
}

public final class WebSessionHost: NSObject, WKScriptMessageHandler {
    public weak var delegate: WebSessionDelegate?

    private let webView: WKWebView
    private var pendingCompletion: ((Result<[String: Any], Error>) -> Void)?
    private var pendingPlatform: WebSessionPlatform?

    public override init() {
        let jsc = WKUserContentController()
        let config = WKWebViewConfiguration()
        config.userContentController = jsc
        config.allowsInlineMediaPlayback = true
        config.websiteDataStore = .default() // persistent login session

        webView = WKWebView(frame: .zero, configuration: config)

        super.init()

        // Capture bridge + engine are added as user scripts below.
        webView.configuration.userContentController.add(self, name: "onemedia")
        let bridge = "window.__onemediaHost || (window.__onemediaHost={ capture:e => { try { window.webkit.messageHandlers.onemedia.postMessage(JSON.stringify(e)) } catch {} } });"
        webView.configuration.userContentController.addUserScript(
            WKUserScript(source: bridge, injectionTime: .atDocumentStart, forMainFrameOnly: true)
        )
        webView.navigationDelegate = self
    }

    /// Ask the user to sign in once. The persistent data store keeps the session.
    public func login(platform: WebSessionPlatform) {
        let url = URL(string: platform.origin + platform.loginPath)!
        webView.load(URLRequest(url: url))
    }

    /// Present (or hide) the web view on screen, e.g. for the one-time login.
    /// It works off-screen too — presentation is only needed while signing in.
    public func attach(to view: UIView? = nil) {
        guard let view = view else { return }
        webView.frame = view.bounds
        webView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(webView)
    }

    public func detach() {
        webView.removeFromSuperview()
    }

    /// Load the scraper engine into the current page (idempotent). Reads the
    /// bundled `web-session.js` from the main bundle so `bun run build:session`
    /// can be re-run without a Swift recompile.
    public func ensureEngine(bundle: Bundle = .main) {
        let url = bundle.url(forResource: "web-session", withExtension: "js", subdirectory: "WebSession")
            ?? bundle.url(forResource: "web-session", withExtension: "js")
        guard let url = url, let src = try? String(contentsOf: url, encoding: .utf8) else {
            delegate?.session(self, didFail: .twitter, error: "web-session.js missing from bundle")
            return
        }
        let script = WKUserScript(source: src, injectionTime: .atDocumentStart, forMainFrameOnly: true)
        webView.configuration.userContentController.addUserScript(script)
    }

    /// True when the current page is the platform origin and the engine is live.
    public func originIs(platform: WebSessionPlatform, completion: @escaping (Bool) -> Void) {
        webView.evaluateJavaScript(
            "!!window.__onemedia && window.__onemedia.isOk('\(platform.rawValue)')"
        ) { result, _ in
            completion(result as? Bool ?? false)
        }
    }

    /// Drive the in-page scraper. Resolves when the envelope is posted back
    /// over the "onemedia" message handler.
    public func scrape(_ platform: WebSessionPlatform, kind: String, completion: @escaping (Result<[String: Any], Error>) -> Void) {
        pendingCompletion = completion
        pendingPlatform = platform
        let js = "window.__onemedia && window.__onemedia.scrape('\(platform.rawValue)', '\(kind)')"
        webView.evaluateJavaScript(js) { [weak self] _, error in
            guard let self = self else { return }
            if let error = error {
                self.delegate?.session(self, didFail: platform, error: error.localizedDescription)
            }
        }
    }

    /// Return the cookies the WebView holds for the platform domain.
    public func harvestCookies(platform: WebSessionPlatform, completion: @escaping ([String: String]) -> Void) {
        let domain = URL(string: platform.origin)!.host!
        webView.configuration.websiteDataStore.httpCookieStore.getAllCookies { cookies in
            var out: [String: String] = [:]
            for cookie in cookies where cookie.domain.contains(domain) {
                out[cookie.name] = cookie.value
            }
            completion(out)
        }
    }

    public func clear(platform: WebSessionPlatform) {
        webView.configuration.websiteDataStore.removeData(
            ofTypes: [WKWebsiteDataTypeCookies, WKWebsiteDataTypeLocalStorage],
            modifiedSince: .distantPast
        ) { _ in }
    }

    // MARK: - WKScriptMessageHandler

    @objc public func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.name == "onemedia",
              let raw = message.body as? String,
              let data = raw.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return
        }
        let completion = pendingCompletion
        let platform = pendingPlatform
        pendingCompletion = nil
        pendingPlatform = nil
        completion?(.success(parsed))
        if let platform = platform {
            delegate?.session(self, didCapture: parsed, for: platform)
        }
    }
}

extension WebSessionHost: WKNavigationDelegate {
    @objc public func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        delegate?.session(self, didFail: .twitter, error: error.localizedDescription)
    }
}
