use crate::db;
use crate::models::html_to_text;
use feed_rs::parser;

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
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    let feed = parser::parse(&bytes[..]).map_err(|e| format!("parse failed: {e}"))?;

    let title = feed.title.as_ref().map(|t| t.content.clone()).unwrap_or_default();
    let description = feed.description.as_ref().map(|d| d.content.clone());
    let icon_url = feed.icon.as_ref().map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|l| l.uri.clone()));
    let site_url = feed.links.iter().find(|l| l.rel.as_deref() != Some("self")).map(|l| l.href.clone())
        .or_else(|| feed.links.first().map(|l| l.href.clone()));

    let mut entries = Vec::with_capacity(feed.entries.len());
    for entry in &feed.entries {
        let guid = if entry.id.is_empty() {
            format!("{}#{}", url, entry.title.as_ref().map(|t| t.content.as_str()).unwrap_or(""))
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

/// DB-only stage: insert parsed entries, skipping known guids.
pub fn store(conn: &rusqlite::Connection, source_id: i64, parsed: &ParsedFeed) -> Result<usize, String> {
    let mut inserted = 0usize;
    for e in &parsed.entries {
        if db::insert_item(
            conn,
            source_id,
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
            },
        )? {
            inserted += 1;
        }
    }
    Ok(inserted)
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
