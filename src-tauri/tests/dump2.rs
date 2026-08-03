use no_pysop_lib::http::HttpClient;
use no_pysop_lib::types::Credential;
use no_pysop_lib::ingestion::instagram::ensure_sessionid_prefix;

fn load_stored_credential() -> Credential {
    use base64::Engine;
    let path = dirs_next::data_dir().unwrap().join("no_pysop").join("cred_Instagram.json");
    let encoded = std::fs::read_to_string(&path).unwrap();
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).unwrap();
    serde_json::from_slice(&decoded).unwrap()
}

#[tokio::test]
#[ignore]
async fn dump_user_info() {
    let cred = load_stored_credential();
    let token = ensure_sessionid_prefix(&cred.session_token);
    let client = HttpClient::with_session(&token);
    let resp = client.client()
        .get("https://www.instagram.com/api/v1/users/60206365731/info/")
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.instagram.com/")
        .send().await.unwrap();
    let status = resp.status();
    let text = resp.text().await.unwrap();
    let head: String = text.chars().take(500).collect();
    println!("USER-INFO STATUS: {} BODY: {}", status.as_u16(), head);
}
