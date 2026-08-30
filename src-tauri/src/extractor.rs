use crate::models::html_to_text;

/// Network-only stage: fetch the page and extract main article HTML (sanitized).
pub async fn extract_from_url(client: &reqwest::Client, link: &str) -> Result<String, String> {
    let resp = client
        .get(link)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let html = resp.text().await.map_err(|e| e.to_string())?;

    let base_url = url::Url::parse(link).map_err(|e| e.to_string())?.to_string();
    let mut readability = dom_smoothie::Readability::new(html.as_str(), Some(&base_url), None)
        .map_err(|e| format!("extract failed: {e}"))?;
    let article = readability
        .parse()
        .map_err(|e| format!("extract failed: {e}"))?;

    Ok(ammonia::clean(article.content.as_ref()))
}

/// Sync helper used by the command layer to compute the plain-text snippet.
pub fn snippet_of(content: &str) -> String {
    let text = html_to_text(content);
    text.trim().chars().take(200).collect::<String>()
}
