use crate::db;
use crate::models::html_to_text;
use feed_rs::parser;

/// Minimal source identity needed for rule evaluation and storage.
pub struct SourceCtx {
    pub id: i64,
    pub group_id: Option<i64>,
    pub url: String,
}

/// Result of storing a parsed feed.
pub struct StoreOutcome {
    pub inserted: usize,
    /// Titles of new articles matched by a "notify" rule.
    pub notified: Vec<String>,
}

/// A feed entry converted to plain DB-ready values.
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

/// Network-only stage: download and parse a feed. Holds no DB references.
pub async fn fetch_and_parse(client: &reqwest::Client, url: &str) -> Result<ParsedFeed, String> {
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let final_url = resp.url().as_str().to_string();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
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

/// DB-only stage: insert parsed entries, skipping known guids. Applies the
/// rule engine to new entries before insertion (mark read / star / hide /
/// notify). Existing rows are never touched.
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
            let outcome = engine.evaluate(
                source.id,
                source.group_id,
                &source.url,
                &e.title,
                &format!("{content_text} {summary_text}"),
                e.author.as_deref(),
                e.url.as_deref(),
            );
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

pub async fn fetch_favicon(
    client: &reqwest::Client,
    feed_url: &str,
    icon_url: Option<&str>,
    site_url: Option<&str>,
    favicon_dir: &std::path::PathBuf,
    source_id: i64,
) -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();

    // 1. Explicit feed icon URL from RSS/Atom
    if let Some(u) = icon_url {
        let u = u.trim();
        if !u.is_empty() {
            candidates.push(u.to_string());
        }
    }

    // 2. Derive origins and domains
    let target = site_url.unwrap_or(feed_url);
    if let Ok(parsed) = url::Url::parse(target) {
        let origin = parsed.origin().ascii_serialization();
        let host = parsed.host_str().unwrap_or("").to_string();

        candidates.push(format!("{origin}/favicon.ico"));
        candidates.push(format!("{origin}/favicon.png"));
        candidates.push(format!("{origin}/apple-touch-icon.png"));
        candidates.push(format!("{origin}/apple-touch-icon-precomposed.png"));

        if !host.is_empty() {
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
        if let Ok(resp) = req.send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    if bytes.len() > 80 && bytes.len() < 2_000_000 {
                        let ext = if candidate.contains(".png") || candidate.contains("google.com") {
                            "png"
                        } else if candidate.contains(".svg") {
                            "svg"
                        } else {
                            "ico"
                        };
                        let path = favicon_dir.join(format!("{source_id}.{ext}"));
                        if tokio::fs::write(&path, &bytes).await.is_ok() {
                            return Some(path);
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
