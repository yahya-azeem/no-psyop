use std::process::{Command, Stdio};
use std::time::Duration;

/// Client for the `sidecars/x_proxy.py` Playwright service.
///
/// x.com's GraphQL API returns HTTP 403 to plain HTTP/TLS clients (reqwest,
/// curl) behind its bot protection, so authenticated calls are made by a real
/// headless Chromium process and the API JSON is proxied back to us. Each op
/// applies the session cookies to the browser context before navigating.
pub struct XProxy;

fn port() -> u16 {
    std::env::var("XPROXY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8192)
}

fn base_url() -> String {
    format!("http://127.0.0.1:{}", port())
}

async fn health() -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    matches!(client.get(format!("{}/health", base_url())).send().await, Ok(r) if r.status().is_success())
}

fn spawn_proxy() {
    let script = format!("{}/../sidecars/x_proxy.py", env!("CARGO_MANIFEST_DIR"));
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mut candidates: Vec<String> = vec![
        format!("{}/.local/share/no_pysop/.venv/bin/python", home),
        "/tmp/opencode/.xvenv/bin/python".into(),
        "python3".into(),
        "python".into(),
    ];
    if let Ok(py) = std::env::var("XPROXY_PYTHON") {
        candidates.insert(0, py);
    }
    for py in candidates {
        if py.is_empty() {
            continue;
        }
        let mut cmd = Command::new(&py);
        cmd.arg(&script)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null());
        if cmd.spawn().is_ok() {
            return;
        }
    }
}

/// Make sure the proxy is running, starting it if necessary.
pub async fn ensure_running() -> Result<(), String> {
    if health().await {
        return Ok(());
    }
    spawn_proxy();
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if health().await {
            return Ok(());
        }
    }
    Err("x_proxy failed to start (needs python3 + playwright, see sidecars/x_proxy.py)".into())
}

/// Run one GraphQL op through the browser proxy and return its response body.
///
/// `cookies` is the raw `auth_token=...; ct0=...` session string; `username`
/// is only used by the `profile` op.
impl XProxy {
    pub async fn op(cookies: &str, op_name: &str, username: &str) -> Result<serde_json::Value, String> {
        ensure_running().await?;
        let body = serde_json::json!({
            "op": op_name,
            "username": username,
            "cookies": cookies,
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(100))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .post(format!("{}/op", base_url()))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("proxy http: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("proxy response: {}", e))?;
        if v["ok"].as_bool().unwrap_or(false) {
            Ok(v["body"].clone())
        } else {
            Err(v["error"].as_str().unwrap_or("proxy error").to_string())
        }
    }

    pub async fn feed(cookies: &str) -> Result<serde_json::Value, String> {
        Self::op(cookies, "feed", "").await
    }

    pub async fn profile(cookies: &str, username: &str) -> Result<serde_json::Value, String> {
        Self::op(cookies, "profile", username).await
    }

    pub async fn inbox(cookies: &str) -> Result<serde_json::Value, String> {
        Self::op(cookies, "inbox", "").await
    }

    pub async fn user_tweets(cookies: &str, username: &str) -> Result<serde_json::Value, String> {
        Self::op(cookies, "user_tweets", username).await
    }

    pub async fn search(cookies: &str, query: &str) -> Result<serde_json::Value, String> {
        Self::op(cookies, "search", query).await
    }

    /// Send a DM through the persistent X browser profile (which holds the
    /// encrypted-DM passcode) by driving the conversation composer.
    pub async fn send_dm(cookies: &str, conversation_id: &str, text: &str) -> Result<(), String> {
        Self::proxy_op("x_send", cookies, conversation_id, text).await.map(|_| ())
    }

    /// Send a LinkedIn message through the device-trusted profile browser.
    pub async fn linkedin_send(conversation_id: &str, text: &str) -> Result<(), String> {
        Self::proxy_op("linkedin_send", "", conversation_id, text).await.map(|_| ())
    }

    async fn proxy_op(op_name: &str, cookies: &str, conversation_id: &str, text: &str) -> Result<serde_json::Value, String> {
        ensure_running().await?;
        let body = serde_json::json!({
            "op": op_name,
            "username": "",
            "cookies": cookies,
            "conversation_id": conversation_id,
            "text": text,
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(150))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .post(format!("{}/op", base_url()))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("proxy http: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("proxy response: {}", e))?;
        if v["ok"].as_bool().unwrap_or(false) {
            Ok(v["body"].clone())
        } else {
            Err(v["error"].as_str().unwrap_or("proxy error").to_string())
        }
    }

    /// Scrape the LinkedIn home feed through the browser sidecar (LinkedIn
    /// blocks plain-HTTP clients). Returns `{"posts":[...]}`.
    pub async fn linkedin_feed(cookies: &str) -> Result<serde_json::Value, String> {
        ensure_running().await?;
        let body = serde_json::json!({
            "op": "linkedin_feed",
            "username": "",
            "cookies": cookies,
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .post(format!("{}/op", base_url()))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("proxy http: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("proxy response: {}", e))?;
        if v["ok"].as_bool().unwrap_or(false) {
            Ok(v["body"].clone())
        } else {
            Err(v["error"].as_str().unwrap_or("proxy error").to_string())
        }
    }

    /// Capture the LinkedIn messaging inbox through the trusted browser profile.
    pub async fn linkedin_messages() -> Result<serde_json::Value, String> {
        ensure_running().await?;
        let body = serde_json::json!({ "op": "linkedin_messages", "username": "", "cookies": "" });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .post(format!("{}/op", base_url()))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("proxy http: {}", e))?;
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("proxy response: {}", e))?;
        if v["ok"].as_bool().unwrap_or(false) {
            Ok(v["body"].clone())
        } else {
            Err(v["error"].as_str().unwrap_or("proxy error").to_string())
        }
    }
}
