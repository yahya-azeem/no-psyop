use no_pysop_lib::http::HttpClient;
use no_pysop_lib::types::Credential;
use no_pysop_lib::ingestion::instagram::ensure_sessionid_prefix;

fn load_stored_credential() -> Credential {
    use base64::Engine;
    let path = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("no_pysop")
        .join("cred_Instagram.json");
    let encoded = std::fs::read_to_string(&path).unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).unwrap();
    serde_json::from_slice(&decoded).unwrap()
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn dump_reel_media() {
    let cred = load_stored_credential();
    let token = ensure_sessionid_prefix(&cred.session_token);
    let client = HttpClient::with_session(&token);

    let url = "https://www.instagram.com/api/v1/feed/user/24746140607/reel_media/";
    let resp = client
        .client()
        .get(url)
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.instagram.com/")
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    println!("REEL_MEDIA STATUS: {}", status.as_u16());
    println!("REEL_MEDIA BODY: {}", text.chars().take(1800).collect::<String>());
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn dump_comments() {
    let cred = load_stored_credential();
    let token = ensure_sessionid_prefix(&cred.session_token);
    let client = HttpClient::with_session(&token);

    let url = "https://www.instagram.com/api/v1/media/3946311266234209654_40243796564/comments/?can_support_threading=true";
    let resp = client
        .client()
        .get(url)
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.instagram.com/")
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    println!("COMMENTS STATUS: {}", status.as_u16());
    println!("COMMENTS BODY: {}", text.chars().take(1500).collect::<String>());
}
