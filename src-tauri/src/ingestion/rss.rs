use crate::types::{Platform, Post};

fn entry_ts(ts: &Option<chrono::DateTime<chrono::Utc>>) -> u64 {
    ts.map(|d| d.timestamp().max(0) as u64).unwrap_or(0)
}

/// Parse a single RSS/Atom feed into posts. The source title becomes the
/// author (source) name. Entries with neither a title nor a summary are dropped.
pub fn parse_feed(source: &feed_rs::model::Feed) -> Vec<Post> {
    let source_title = source
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .unwrap_or_else(|| "RSS".to_string());
    let source_id = source
        .id
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("rss")
        .to_string();

    source
        .entries
        .iter()
        .filter_map(|e| post_from_entry(e, &source_title, &source_id))
        .collect()
}

fn post_from_entry(e: &feed_rs::model::Entry, source: &str, source_id: &str) -> Option<Post> {
    let title = e
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .unwrap_or_default();
    let summary = e
        .summary
        .as_ref()
        .map(|s| s.content.trim().to_string())
        .unwrap_or_default();
    if title.is_empty() && summary.is_empty() {
        return None;
    }

    let mut content = title;
    let text = strip_html(&summary);
    if !text.is_empty() {
        content.push('\n');
        content.push_str(&text);
    }
    let content = content.trim().to_string();

    let id = {
        let link = links_primary(&e.links);
        if !link.is_empty() {
            link
        } else {
            e.id.clone()
        }
    };

    let author = e
        .authors
        .iter()
        .find_map(|a| if a.name.trim().is_empty() { None } else { Some(a.name.clone()) })
        .unwrap_or_else(|| source.to_string());

    let mut media_urls: Vec<String> = Vec::new();
    let mut poster: Option<String> = None;
    for m in &e.media {
        for content in &m.content {
            if let Some(u) = &content.url {
                let u = u.to_string();
                if u.starts_with("http") && !media_urls.iter().any(|x| x == &u) {
                    media_urls.push(u.clone());
                    if poster.is_none() {
                        poster = Some(u.clone());
                    }
                }
            }
        }
    }
    if media_urls.is_empty() {
        if let Some(img) = find_image(&e.links) {
            media_urls.push(img.clone());
            poster = Some(img);
        }
    }

    let timestamp = entry_ts(&e.updated).max(entry_ts(&e.published));

    Some(Post {
        id,
        platform: Platform::Rss,
        author_id: source_id.to_string(),
        author_username: author,
        content,
        media_urls,
        poster_url: poster,
        liker_ids: vec![],
        commenter_ids: vec![],
        timestamp,
        is_video: false,
        author_is_mutual: None,
        author_is_close_friend: None,
        engagement_score: None,
        is_synthetic: None,
        vector_embedding: None,
    })
}

fn links_primary(links: &[feed_rs::model::Link]) -> String {
    links
        .iter()
        .find(|l| l.rel.as_deref().unwrap_or("alternate") == "alternate")
        .map(|l| l.href.clone())
        .or_else(|| links.first().map(|l| l.href.clone()))
        .unwrap_or_default()
}

fn find_image(links: &[feed_rs::model::Link]) -> Option<String> {
    links
        .iter()
        .find(|l| {
            l.media_type
                .as_deref()
                .map(|t| t.starts_with("image"))
                .unwrap_or(false)
        })
        .and_then(|l| Some(l.href.clone()))
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' if !in_tag => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fetch a list of feed URLs in parallel and concatenate the parsed posts.
pub async fn fetch_all(feeds: &[String]) -> Vec<Post> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; no-pysop/1.0)")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut tasks = Vec::new();
    for url in feeds {
        let client = client.clone();
        let url = url.clone();
        tasks.push(async move {
            let ok = client.get(&url).send().await;
            let xml = match ok {
                Ok(r) if r.status().is_success() => r.text().await.ok(),
                _ => None,
            };
            match xml {
                Some(body) => parse(body.as_bytes()).unwrap_or_default(),
                None => Vec::new(),
            }
        });
    }

    let mut posts = Vec::new();
    for t in tasks {
        posts.extend(t.await);
    }
    posts
}

/// Parse raw feed bytes into posts (empty on parse error).
pub fn parse(bytes: &[u8]) -> Result<Vec<Post>, String> {
    let feed = feed_rs::parser::parse(bytes).map_err(|e| e.to_string())?;
    Ok(parse_feed(&feed))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Tech Daily</title>
    <link>https://tech.example.com</link>
    <description>daily tech</description>
    <item>
      <title>Rust 1.0 released</title>
      <link>https://tech.example.com/rust</link>
      <guid>https://tech.example.com/rust</guid>
      <pubDate>Wed, 01 Jan 2025 10:00:00 GMT</pubDate>
      <description>Hello <b>world</b> feature</description>
      <enclosure url="https://tech.example.com/img.png" type="image/png"/>
    </item>
    <item>
      <link>https://tech.example.com/untitled</link>
      <guid>https://tech.example.com/untitled</guid>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parse_parses_entries_and_extracts_text_and_media() {
        let posts = parse(RSS.as_bytes()).expect("parses");
        assert_eq!(posts.len(), 1);
        let p = &posts[0];
        assert_eq!(p.platform, Platform::Rss);
        assert_eq!(p.author_username, "Tech Daily");
        assert!(p.content.contains("Rust"));
        assert!(!p.content.contains("<b>"), "html stripped: {}", p.content);
        assert!(p.media_urls.contains(&"https://tech.example.com/img.png".to_string()));
        assert_eq!(p.poster_url.as_deref(), Some("https://tech.example.com/img.png"));
        assert!(p.timestamp > 0);
        assert_eq!(p.id, "https://tech.example.com/rust");
    }

    #[test]
    fn strip_html_removes_tags_and_collapses_whitespace() {
        assert_eq!(strip_html("a <b>x</b> c"), "a x c");
    }
}