/// 仅网络阶段：抓取网页并提取经净化处理的正文文章 HTML。
pub async fn extract_from_url(client: &reqwest::Client, link: &str) -> Result<String, String> {
    let req = client
        .get(link)
        .timeout(std::time::Duration::from_secs(30));
    let html = crate::net::send_and_check_text("extractor", "GET", link, req).await?;

    let mut readability = dom_smoothie::Readability::new(html.as_str(), Some(link), None)
        .map_err(|e| format!("extract failed: {e}"))?;
    let article = readability
        .parse()
        .map_err(|e| format!("extract failed: {e}"))?;

    Ok(ammonia::clean(article.content.as_ref()))
}

/// 命令层使用的同步辅助函数，用于提取并生成纯文本摘要。
pub fn snippet_of(content: &str) -> String {
    crate::services::article::snippet_of(content)
}
