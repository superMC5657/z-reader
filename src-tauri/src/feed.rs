use crate::db;
use crate::models::html_to_text;
use feed_rs::parser;

/// 规则评估与文章存储所需的最小订阅源标识信息。
pub struct SourceCtx {
    pub id: i64,
    pub group_id: Option<i64>,
    pub url: String,
}

/// 存储已解析订阅源的结果。
pub struct StoreOutcome {
    pub inserted: usize,
    /// 匹配到“通知”规则的新增文章标题列表。
    pub notified: Vec<String>,
}

/// 转换为适合存入数据库的扁平化订阅源条目。
pub struct NewEntry {
    pub guid: String,
    pub title: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub published_at: i64,
    pub content: String,
    pub summary: Option<String>,
    pub snippet: String,
    pub image: Option<String>,
}

pub struct ParsedFeed {
    pub title: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub site_url: Option<String>,
    pub entries: Vec<NewEntry>,
}

/// 仅网络阶段：下载并解析订阅源。不持有任何数据库连接引用。
pub async fn fetch_and_parse(client: &reqwest::Client, url: &str) -> Result<ParsedFeed, String> {
    fetch_and_parse_with(client, url, "", None).await
}

/// 功能同 [`fetch_and_parse`]，但在网络日志行上标记 `cycle=` / `id=`。
/// 周期为空且订阅源 ID 为 None 时与旧调用方（命令层）保持行为一致。
pub async fn fetch_and_parse_with(
    client: &reqwest::Client,
    url: &str,
    cycle: &str,
    source_id: Option<i64>,
) -> Result<ParsedFeed, String> {
    let req = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30));
    let resp = crate::net::send_logged_with("feed", "GET", url, req, cycle, source_id)
        .await
        .map_err(|e| crate::net::http_err_reason("fetch failed", &e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let final_url = resp.url().as_str().to_string();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| crate::net::http_err_reason("body failed", &e))?;
    parse_feed_data(&bytes[..], Some(&final_url), url)
}

pub fn parse_feed_data(bytes: &[u8], base_url: Option<&str>, original_url: &str) -> Result<ParsedFeed, String> {
    let mut builder = parser::Builder::new();
    if let Some(base) = base_url {
        builder = builder.base_uri(Some(base));
    }
    let feed = builder.build().parse(bytes).map_err(|e| format!("parse failed: {e}"))?;

    let title = feed.title.as_ref().map(|t| t.content.clone()).unwrap_or_default();
    let description = feed.description.as_ref().map(|d| d.content.clone());
    let icon_url = feed.icon.as_ref().map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|l| l.uri.clone()));
    let site_url = feed.links.iter().find(|l| l.rel.as_deref() != Some("self")).map(|l| l.href.clone())
        .or_else(|| feed.links.first().map(|l| l.href.clone()));

    let mut entries = Vec::with_capacity(feed.entries.len());
    for entry in &feed.entries {
        let guid = if entry.id.is_empty() {
            format!("{}#{}", original_url, entry.title.as_ref().map(|t| t.content.as_str()).unwrap_or(""))
        } else {
            entry.id.clone()
        };
        let raw_html = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            .unwrap_or_default();
        let content = ammonia::clean(&raw_html);
        let summary = entry.summary.as_ref().map(|s| ammonia::clean(&s.content));
        let snippet = {
            let text = html_to_text(summary.as_deref().unwrap_or(&content));
            text.trim().chars().take(200).collect::<String>()
        };
        let entry_title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| "Untitled".into());
        let link = entry.links.first().map(|l| l.href.clone());
        let author = entry
            .authors
            .first()
            .map(|a| a.name.clone())
            .or_else(|| feed.authors.first().map(|a| a.name.clone()));
        let published_at = entry
            .published
            .or(entry.updated)
            .map(|d| d.timestamp())
            .unwrap_or_else(crate::models::now_ts);
        let image: Option<String> = entry
            .media
            .iter()
            .find_map(|m| m.thumbnails.first().map(|img| img.image.uri.clone()))
            .or_else(|| {
                entry.media.iter().find_map(|m| {
                    m.content
                        .iter()
                        .find(|c| {
                            c.content_type
                                .as_ref()
                                .map(|t| t.ty().as_str() == "image")
                                .unwrap_or(false)
                        })
                        .and_then(|c| c.url.as_ref().map(|u| u.to_string()))
                })
            });

        entries.push(NewEntry {
            guid,
            title: entry_title,
            url: link,
            author,
            published_at,
            content,
            summary,
            snippet,
            image,
        });
    }

    Ok(ParsedFeed { title, description, icon_url, site_url, entries })
}

/// 仅数据库阶段：插入已解析条目，跳过已存在的 GUID。在插入前对新条目
/// 应用规则引擎（标记已读 / 加星标 / 隐藏 / 通知）。绝不修改已有数据行。
pub fn store(
    conn: &rusqlite::Connection,
    source: &SourceCtx,
    parsed: &ParsedFeed,
    engine: Option<&crate::rules::RuleEngine>,
) -> Result<StoreOutcome, String> {
    let mut out = StoreOutcome { inserted: 0, notified: Vec::new() };
    for e in &parsed.entries {
        let mut has_been_read = false;
        let mut starred = false;
        let mut hidden = false;
        if let Some(engine) = engine.filter(|eng| !eng.is_empty()) {
            let content_text = html_to_text(&e.content);
            let summary_text = e.summary.as_deref().map(html_to_text).unwrap_or_default();
            let full_text = format!("{content_text} {summary_text}");
            let ctx = crate::rules::ArticleEvalContext {
                source_id: source.id,
                group_id: source.group_id,
                source_url: &source.url,
                title: &e.title,
                content_text: &full_text,
                author: e.author.as_deref(),
                url: e.url.as_deref(),
            };
            let outcome = engine.evaluate(&ctx);
            has_been_read = outcome.mark_read || outcome.hide;
            starred = outcome.star;
            hidden = outcome.hide;
            if outcome.notify {
                out.notified.push(e.title.clone());
            }
        }
        if db::insert_item(
            conn,
            source.id,
            &db::UpsertEntry {
                guid: &e.guid,
                title: &e.title,
                url: e.url.as_deref(),
                author: e.author.as_deref(),
                published_at: e.published_at,
                content: Some(&e.content),
                summary: e.summary.as_deref(),
                snippet: Some(&e.snippet),
                image: e.image.as_deref(),
                has_been_read,
                starred,
                hidden,
            },
        )? {
            out.inserted += 1;
        }
    }
    Ok(out)
}

/// 通过魔数识别图片格式。返回用于存储的文件扩展名，若负载未知则返回 `None`。
/// 故意拒绝 SVG 格式：网站图标直接在 UI 中渲染，内联 SVG 可能携带动态脚本代码。
pub fn sniff_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        Some("ico")
    } else if bytes.len() >= 12 && bytes[0..4] == *b"RIFF" && bytes[8..12] == *b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub async fn fetch_favicon(
    client: &reqwest::Client,
    feed_url: &str,
    icon_url: Option<&str>,
    site_url: Option<&str>,
    favicon_dir: &std::path::Path,
    source_id: i64,
    allow_third_party: bool,
) -> Option<std::path::PathBuf> {
    fetch_favicon_with(client, feed_url, icon_url, site_url, favicon_dir, source_id, allow_third_party, "").await
}

/// 功能同 [`fetch_favicon`]，但在网络日志行上标记 `cycle=`（网站图标
/// 候选请求保持 debug 级别，绝不升为 INFO）。
pub async fn fetch_favicon_with(
    client: &reqwest::Client,
    feed_url: &str,
    icon_url: Option<&str>,
    site_url: Option<&str>,
    favicon_dir: &std::path::Path,
    source_id: i64,
    allow_third_party: bool,
    cycle: &str,
) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();

    // 1. RSS/Atom 中显式指定的订阅源图标 URL
    if let Some(u) = icon_url {
        let u = u.trim();
        if !u.is_empty() {
            candidates.push(u.to_string());
        }
    }

    // 2. 派生源地址（Origin）和域名（Domain）
    let target = site_url.unwrap_or(feed_url);
    if let Ok(parsed) = url::Url::parse(target) {
        let origin = parsed.origin().ascii_serialization();
        let host = parsed.host_str().unwrap_or("").to_string();

        candidates.push(format!("{origin}/favicon.ico"));
        candidates.push(format!("{origin}/favicon.png"));
        candidates.push(format!("{origin}/apple-touch-icon.png"));
        candidates.push(format!("{origin}/apple-touch-icon-precomposed.png"));

        // 第三方回退会将订阅域名暴露给 Google/DuckDuckGo；
        // 仅在用户显式开启时才纳入候选。
        if allow_third_party && !host.is_empty() {
            candidates.push(format!("https://www.google.com/s2/favicons?domain={host}&sz=64"));
            candidates.push(format!("https://icons.duckduckgo.com/ip2/{host}.ico"));
        }
    }

    if let Ok(parsed) = url::Url::parse(feed_url) {
        let origin = parsed.origin().ascii_serialization();
        let ico = format!("{origin}/favicon.ico");
        if !candidates.contains(&ico) {
            candidates.push(ico);
        }
    }

    for candidate in &candidates {
        let req = client.get(candidate).timeout(std::time::Duration::from_secs(8));
        if let Ok(resp) = crate::net::send_logged_with("favicon", "GET", candidate, req, cycle, Some(source_id)).await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    if bytes.len() > 80 && bytes.len() < 2_000_000 {
                        // 扩展名来自文件内容负载而非 URL：
                        // 跳过错误标注或非图片的响应体。
                        if let Some(ext) = sniff_image_ext(&bytes) {
                            let path = favicon_dir.join(format!("{source_id}.{ext}"));
                            if tokio::fs::write(&path, &bytes).await.is_ok() {
                                let (host, _) = crate::net::sanitize_url(candidate);
                                log::debug!("[NET] kind=favicon host={host} saved ext={ext}");
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sniff_image_ext() {
        // 给定：规范的魔数文件头
        // 当/那么：识别各有效图片格式，拒绝 SVG 和无效垃圾数据
        assert_eq!(sniff_image_ext(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]), Some("png"));
        assert_eq!(sniff_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]), Some("jpg"));
        assert_eq!(sniff_image_ext(b"GIF89a\x01\x00"), Some("gif"));
        assert_eq!(sniff_image_ext(&[0x00, 0x00, 0x01, 0x00, 0x01]), Some("ico"));
        assert_eq!(sniff_image_ext(b"RIFF\x00\x00\x00\x00WEBP"), Some("webp"));
        assert_eq!(sniff_image_ext(b"<svg xmlns='http://www.w3.org/2000/svg'>"), None);
        assert_eq!(sniff_image_ext(b"<html>not an image</html>"), None);
        assert_eq!(sniff_image_ext(&[]), None);
    }

    #[test]
    fn test_feed_parse_with_base_uri() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
            <title>Test Feed</title>
            <link href="/feed.xml" rel="self" />
            <link href="/index.html" />
            <icon>/favicon.ico</icon>
            <entry>
                <id>entry-1</id>
                <title>Post 1</title>
                <link href="/posts/1.html" />
                <summary>Short summary</summary>
            </entry>
        </feed>"#;

        let parsed = parse_feed_data(xml.as_bytes(), Some("https://example.com/blog/feed.xml"), "https://example.com/blog/feed.xml").expect("parse atom");
        assert_eq!(parsed.site_url.as_deref(), Some("https://example.com/index.html"));
        assert_eq!(parsed.icon_url.as_deref(), Some("https://example.com/favicon.ico"));
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].url.as_deref(), Some("https://example.com/posts/1.html"));
    }
}
