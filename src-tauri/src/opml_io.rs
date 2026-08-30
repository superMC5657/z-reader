use crate::db;
use opml::Outline;

pub struct ImportResult {
    pub groups_added: usize,
    pub sources_added: usize,
    pub sources_existing: usize,
}

pub fn import(conn: &rusqlite::Connection, text: &str) -> Result<ImportResult, String> {
    let doc = opml::OPML::from_str(text).map_err(|e| format!("invalid OPML: {e}"))?;
    let mut result = ImportResult { groups_added: 0, sources_added: 0, sources_existing: 0 };
    for outline in &doc.body.outlines {
        walk_outline(conn, outline, None, &mut result)?;
    }
    Ok(result)
}

fn walk_outline(
    conn: &rusqlite::Connection,
    outline: &Outline,
    group_id: Option<i64>,
    result: &mut ImportResult,
) -> Result<(), String> {
    let has_feed = outline.xml_url.is_some();
    let is_folder = !outline.outlines.is_empty();

    if is_folder {
        // A folder that itself carries a feed URL is treated as a source with children.
        let gid = if group_id.is_some() && !has_feed {
            group_id
        } else {
            None
        };
        let current_group = if !outline.text.trim().is_empty() && !has_feed {
            match existing_group(conn, &outline.text)? {
                Some(id) => Some(id),
                None => {
                    let g = db::create_group(conn, &outline.text)?;
                    result.groups_added += 1;
                    Some(g.id)
                }
            }
        } else {
            gid
        };
        for child in &outline.outlines {
            walk_outline(conn, child, current_group, result)?;
        }
        return Ok(());
    }

    if let Some(xml_url) = &outline.xml_url {
        match db::get_source_by_url(conn, xml_url)? {
            Some(_) => result.sources_existing += 1,
            None => {
                let title = if outline.text.trim().is_empty() { xml_url } else { &outline.text };
                db::insert_source(conn, xml_url, title, None, group_id)?;
                result.sources_added += 1;
            }
        }
    }
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
    let mut head = opml::Head::default();
    head.title = Some("ZReader subscriptions".into());
    doc.head = Some(head);
    for g in &groups {
        let mut folder = Outline::default();
        folder.text = g.name.clone();
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
    let mut o = Outline::default();
    o.text = title.into();
    o.r#type = Some("rss".into());
    o.xml_url = Some(url.into());
    o
}
