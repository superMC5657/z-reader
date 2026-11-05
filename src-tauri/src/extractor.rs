/// 仅网络阶段：抓取网页并提取经净化处理的正文文章 HTML（无周期上下文的单发请求）。
pub async fn extract_from_url(client: &reqwest::Client, link: &str) -> Result<String, String> {
    let req = client.get(link).timeout(std::time::Duration::from_secs(30));
    let html = crate::net::fetch_text(crate::net::RequestContext::bare("extractor"), "GET", link, req)
        .await
        .map_err(String::from)?;

    let mut readability = dom_smoothie::Readability::new(html.as_str(), Some(link), None)
        .map_err(|e| format!("extract failed: {e}"))?;
    let article = readability
        .parse()
        .map_err(|e| format!("extract failed: {e}"))?;

    Ok(ammonia::clean(article.content.as_ref()))
}
