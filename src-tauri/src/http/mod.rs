use std::sync::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::cookie::Jar;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, ACCEPT, ACCEPT_LANGUAGE};

const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

pub struct HttpClient {
    client: reqwest::Client,
    rate_limiter: Mutex<RateLimiter>,
}

struct RateLimiter {
    max_per_hour: u32,
    counts: Vec<Instant>,
}

impl RateLimiter {
    fn new(max_per_hour: u32) -> Self {
        Self {
            max_per_hour,
            counts: Vec::with_capacity(max_per_hour as usize),
        }
    }

    fn check(&mut self) -> Result<(), String> {
        let now = Instant::now();
        self.counts.retain(|t| now.duration_since(*t).as_secs() < 3600);
        if self.counts.len() >= self.max_per_hour as usize {
            return Err("rate limit reached for this hour".into());
        }
        self.counts.push(now);
        Ok(())
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self::with_rate_limit(200)
    }

    pub fn with_rate_limit(max_per_hour: u32) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .cookie_store(true)
            .build()
            .expect("http client");

        Self {
            client,
            rate_limiter: Mutex::new(RateLimiter::new(max_per_hour)),
        }
    }

    pub fn with_session(cookies: &str) -> Self {
        let mut h = Self::new();
        h.set_cookies(cookies);
        h
    }

    pub fn set_cookies(&mut self, cookie_str: &str) {
        let jar = Jar::default();
        for pair in cookie_str.split(';') {
            let pair = pair.trim();
            if let Some((k, v)) = pair.split_once('=') {
                jar.add_cookie_str(
                    &format!("{}={}; Domain=.x.com; Path=/", k.trim(), v.trim()),
                    &"https://x.com".parse().unwrap(),
                );
                jar.add_cookie_str(
                    &format!("{}={}; Domain=.instagram.com; Path=/", k.trim(), v.trim()),
                    &"https://www.instagram.com".parse().unwrap(),
                );
                jar.add_cookie_str(
                    &format!("{}={}; Domain=.linkedin.com; Path=/", k.trim(), v.trim()),
                    &"https://www.linkedin.com".parse().unwrap(),
                );
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));

        self.client = reqwest::Client::builder()
            .default_headers(headers)
            .cookie_provider(Arc::new(jar))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client");
    }

    pub fn set_cookie_domain(&mut self, cookie_str: &str, domain: &str) {
        let jar = Jar::default();
        for pair in cookie_str.split(';') {
            let pair = pair.trim();
            if let Some((k, v)) = pair.split_once('=') {
                jar.add_cookie_str(
                    &format!("{}={}; Domain={}; Path=/", k.trim(), v.trim(), domain),
                    &format!("https://{}", domain).parse().unwrap(),
                );
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(UA));
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));

        self.client = reqwest::Client::builder()
            .default_headers(headers)
            .cookie_provider(Arc::new(jar))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client");
    }

    pub async fn get_json(&self, url: &str, referer: Option<&str>) -> Result<serde_json::Value, String> {
        {
            let mut limiter = self.rate_limiter.lock().map_err(|e| e.to_string())?;
            limiter.check()?;
        }

        let mut req = self.client.get(url);
        if let Some(ref_) = referer {
            req = req.header("Referer", ref_);
        }

        let resp = req.send().await.map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;

        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }

        serde_json::from_str(&text).map_err(|e| format!("json: {} (body: {})", e, text.chars().take(200).collect::<String>()))
    }

    pub async fn post_json(&self, url: &str, body: serde_json::Value, referer: Option<&str>) -> Result<serde_json::Value, String> {
        {
            let mut limiter = self.rate_limiter.lock().map_err(|e| e.to_string())?;
            limiter.check()?;
        }

        let mut req = self.client.post(url).json(&body);
        if let Some(ref_) = referer {
            req = req.header("Referer", ref_);
        }

        let resp = req.send().await.map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;

        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }

        serde_json::from_str(&text).map_err(|e| format!("json: {}", e))
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}
