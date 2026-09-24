use crate::models::html_to_text;

/// 仅网络阶段：抓取网页并提取经净化处理的正文文章 HTML。
pub async fn extract_from_url(client: &reqwest::Client, link: &str) -> Result<String, String> {
    let req = client
        .get(link)
        .timeout(std::time::Duration::from_secs(30));
    let resp = crate::net::send_logged("extractor", "GET", link, req)
        .await
        .map_err(|e| crate::net::http_err_reason("fetch failed", &e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let final_url = resp.url().to_string();
    let html = resp
        .text()
        .await
        .map_err(|e| crate::net::http_err_reason("body failed", &e))?;

    let mut readability = dom_smoothie::Readability::new(html.as_str(), Some(&final_url), None)
        .map_err(|e| format!("extract failed: {e}"))?;
    let article = readability
        .parse()
        .map_err(|e| format!("extract failed: {e}"))?;

    Ok(ammonia::clean(article.content.as_ref()))
}

/// 命令层使用的同步辅助函数，用于提取并生成纯文本摘要。
pub fn snippet_of(content: &str) -> String {
    let text = html_to_text(content);
    text.trim().chars().take(200).collect::<String>()
}
