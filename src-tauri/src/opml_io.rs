use crate::db;
use opml::Outline;
use std::collections::HashSet;

pub struct ImportResult {
    pub groups_added: usize,
    pub sources_added: usize,
    pub sources_existing: usize,
}

/// Normalize a feed URL for duplicate detection: trim, default to https,
/// lowercase scheme + host (via the URL parser), drop credentials/fragment
/// and trailing slashes. Unparseable input falls back to the trimmed string.
pub fn normalize_feed_url(raw: &str) -> String {
    let trimmed = raw.trim();
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let Ok(mut url) = url::Url::parse(&with_scheme) else {
        return trimmed.to_string();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    let mut s = url.to_string();
    while s.ends_with('/') {
        s.pop();
    }
    s
}

pub fn import(conn: &rusqlite::Connection, text: &str) -> Result<ImportResult, String> {
    let doc = opml::OPML::from_str(text).map_err(|e| format!("invalid OPML: {e}"))?;
    let mut result = ImportResult { groups_added: 0, sources_added: 0, sources_existing: 0 };
    // Seed the dedup set with normalized existing URLs (exact-match lookups
    // alone miss http/https or trailing-slash variants of the same feed).
    let mut seen: HashSet<String> = db::get_sources(conn)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|s| normalize_feed_url(&s.url))
        .collect();
    for outline in &doc.body.outlines {
        walk_outline(conn, outline, None, &mut seen, &mut result)?;
    }
    Ok(result)
}

fn walk_outline(
    conn: &rusqlite::Connection,
    outline: &Outline,
    parent_group: Option<i64>,
    seen: &mut HashSet<String>,
    result: &mut ImportResult,
) -> Result<(), String> {
    let feed_url = outline.xml_url.as_deref().filter(|u| !u.trim().is_empty());

    if outline.outlines.is_empty() {
        if let Some(url) = feed_url {
            import_source(conn, url, &outline.text, parent_group, seen, result)?;
        }
        return Ok(());
    }

    // A folder maps to its own group, so nested folders keep their structure
    // (one group per level; the DB has no nesting). A folder that also
    // carries a feed URL imports that feed under the *parent* group.
    if let Some(url) = feed_url {
        import_source(conn, url, &outline.text, parent_group, seen, result)?;
    }
    let name = outline.text.trim();
    let group_id = if name.is_empty() {
        parent_group
    } else {
        match existing_group(conn, name)? {
            Some(id) => Some(id),
            None => {
                let g = db::create_group(conn, name)?;
                result.groups_added += 1;
                Some(g.id)
            }
        }
    };
    for child in &outline.outlines {
        walk_outline(conn, child, group_id, seen, result)?;
    }
    Ok(())
}

fn import_source(
    conn: &rusqlite::Connection,
    xml_url: &str,
    title: &str,
    group_id: Option<i64>,
    seen: &mut HashSet<String>,
    result: &mut ImportResult,
) -> Result<(), String> {
    let key = normalize_feed_url(xml_url);
    if !seen.insert(key) {
        result.sources_existing += 1;
        return Ok(());
    }
    let title = if title.trim().is_empty() { xml_url.trim() } else { title.trim() };
    db::insert_source(conn, xml_url.trim(), title, None, group_id)?;
    result.sources_added += 1;
    Ok(())
}

fn existing_group(conn: &rusqlite::Connection, name: &str) -> Result<Option<i64>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM groups WHERE name = ?1")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([name]).map_err(|e| e.to_string())?;
    match rows.next().map_err(|e| e.to_string())? {
        Some(row) => Ok(Some(row.get(0).map_err(|e| e.to_string())?)),
        None => Ok(None),
    }
}

pub fn export(conn: &rusqlite::Connection) -> Result<String, String> {
    let groups = db::get_groups(conn)?;
    let sources = db::get_sources(conn)?;

    let mut doc = opml::OPML::default();
    let head = opml::Head { title: Some("ZReader subscriptions".into()), ..Default::default() };
    doc.head = Some(head);
    for g in &groups {
        let mut folder = Outline { text: g.name.clone(), ..Default::default() };
        for s in sources.iter().filter(|s| s.group_id == Some(g.id)) {
            folder.outlines.push(feed_outline(&s.title, &s.url));
        }
        doc.body.outlines.push(folder);
    }
    for s in sources.iter().filter(|s| s.group_id.is_none()) {
        doc.body.outlines.push(feed_outline(&s.title, &s.url));
    }
    doc.to_string().map_err(|e| e.to_string())
}

fn feed_outline(title: &str, url: &str) -> Outline {
    Outline {
        text: title.into(),
        r#type: Some("rss".into()),
        xml_url: Some(url.into()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        conn
    }

    #[test]
    fn normalize_unifies_variants() {
        // Given: textual variants of the same feed
        // When/Then: all normalize identically
        let a = normalize_feed_url("https://Example.COM/feed.xml/");
        let b = normalize_feed_url("https://example.com/feed.xml");
        let c = normalize_feed_url("example.com/feed.xml");
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(c, "https://example.com/feed.xml");
    }

    #[test]
    fn nested_folders_keep_structure_and_dedup() {
        // Given: two-level folders plus a URL duplicate with trailing slash
        let conn = mem_db();
        let doc = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0"><head><title>subs</title></head><body>
<outline text="Tech"><outline text="Rust" xmlUrl="https://a.example/feed.xml"/>
<outline text="Go"><outline text="Go blog" xmlUrl="https://b.example/rss"/></outline></outline>
<outline text="Tech" xmlUrl="https://a.example/feed.xml/"/>
<outline text="Loose" xmlUrl="https://c.example/feed"/>
</body></opml>"#;
        // When: imported
        let r = import(&conn, doc).unwrap();
        // Then: each folder level became a group, feeds landed, slash-variant deduped
        assert_eq!(r.groups_added, 2);
        assert_eq!(r.sources_added, 3);
        assert_eq!(r.sources_existing, 1);
        let groups = crate::db::get_groups(&conn).unwrap();
        assert!(groups.iter().any(|g| g.name == "Tech"));
        assert!(groups.iter().any(|g| g.name == "Go"));
        let sources = crate::db::get_sources(&conn).unwrap();
        let go = sources.iter().find(|s| s.title == "Go blog").unwrap();
        let go_group = groups.iter().find(|g| g.name == "Go").unwrap();
        assert_eq!(go.group_id, Some(go_group.id));
        // The nested feed sits one level down from the top folder, not flattened.
        let tech_group = groups.iter().find(|g| g.name == "Tech").unwrap();
        let rust = sources.iter().find(|s| s.url == "https://a.example/feed.xml").unwrap();
        assert_eq!(rust.group_id, Some(tech_group.id));
    }
}
