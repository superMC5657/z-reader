use crate::models::{Group, Item, Source};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.pragma_update(None, "foreign_keys", "ON").ok();
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), String> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version < 1 {
        conn.execute_batch(
            r#"
            CREATE TABLE groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                expanded INTEGER NOT NULL DEFAULT 1,
                sort INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE sources (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                description TEXT,
                favicon TEXT,
                group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
                last_fetched INTEGER,
                error_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
                guid TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                url TEXT,
                author TEXT,
                published_at INTEGER NOT NULL,
                content TEXT,
                summary TEXT,
                snippet TEXT,
                image TEXT,
                has_been_read INTEGER NOT NULL DEFAULT 0,
                starred INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                UNIQUE(source_id, guid)
            );
            CREATE INDEX idx_items_source_pub ON items(source_id, published_at DESC);
            CREATE INDEX idx_items_pub ON items(published_at DESC);
            CREATE INDEX idx_items_read ON items(has_been_read);
            CREATE INDEX idx_items_starred ON items(starred);
            "#,
        )
        .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "user_version", 1).ok();
    }
    Ok(())
}

fn row_to_group(row: &Row) -> rusqlite::Result<Group> {
    Ok(Group {
        id: row.get(0)?,
        name: row.get(1)?,
        expanded: row.get::<_, i64>(2)? != 0,
        sort: row.get(3)?,
    })
}

pub fn get_groups(conn: &Connection) -> Result<Vec<Group>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, expanded, sort FROM groups ORDER BY sort, id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_group)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn create_group(conn: &Connection, name: &str) -> Result<Group, String> {
    conn.execute("INSERT INTO groups (name) VALUES (?1)", params![name])
        .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(Group { id, name: name.into(), expanded: true, sort: 0 })
}

pub fn rename_group(conn: &Connection, id: i64, name: &str) -> Result<(), String> {
    conn.execute("UPDATE groups SET name=?1 WHERE id=?2", params![name, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_group(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM groups WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_group_expanded(conn: &Connection, id: i64, expanded: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE groups SET expanded=?1 WHERE id=?2",
        params![expanded as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn row_to_source(row: &Row) -> rusqlite::Result<Source> {
    Ok(Source {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        favicon: row.get(4)?,
        group_id: row.get(5)?,
        last_fetched: row.get(6)?,
        error_count: row.get(7)?,
        unread: row.get(8)?,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.url, s.title, s.description, s.favicon, s.group_id, s.last_fetched, s.error_count,
    (SELECT COUNT(*) FROM items i WHERE i.source_id = s.id AND i.has_been_read = 0) AS unread
    FROM sources s";

pub fn get_sources(conn: &Connection) -> Result<Vec<Source>, String> {
    let mut stmt = conn
        .prepare(&format!("{SOURCE_SELECT} ORDER BY s.id"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_source)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn get_source(conn: &Connection, id: i64) -> Result<Source, String> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.id = ?1"),
        params![id],
        row_to_source,
    )
    .map_err(|e| e.to_string())
}

pub fn get_source_by_url(conn: &Connection, url: &str) -> Result<Option<Source>, String> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.url = ?1"),
        params![url],
        row_to_source,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn insert_source(
    conn: &Connection,
    url: &str,
    title: &str,
    description: Option<&str>,
    group_id: Option<i64>,
) -> Result<Source, String> {
    conn.execute(
        "INSERT INTO sources (url, title, description, group_id) VALUES (?1, ?2, ?3, ?4)",
        params![url, title, description, group_id],
    )
    .map_err(|e| e.to_string())?;
    get_source(conn, conn.last_insert_rowid())
}

pub fn remove_source(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM sources WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_source_group(conn: &Connection, source_id: i64, group_id: Option<i64>) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET group_id=?1 WHERE id=?2",
        params![group_id, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn rename_source(conn: &Connection, source_id: i64, title: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET title=?1 WHERE id=?2",
        params![title, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_source_favicon(conn: &Connection, source_id: i64, path: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET favicon=?1 WHERE id=?2",
        params![path, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn mark_source_fetched(conn: &Connection, source_id: i64, ok: bool) -> Result<(), String> {
    if ok {
        conn.execute(
            "UPDATE sources SET last_fetched=?1, error_count=0 WHERE id=?2",
            params![crate::models::now_ts(), source_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE sources SET error_count = error_count + 1 WHERE id=?1",
            params![source_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

const ITEM_COLUMNS: &str = "id, source_id, guid, title, url, author, published_at, content, summary, snippet, image, has_been_read, starred";

fn row_to_item(row: &Row) -> rusqlite::Result<Item> {
    Ok(Item {
        id: row.get(0)?,
        source_id: row.get(1)?,
        guid: row.get(2)?,
        title: row.get(3)?,
        url: row.get(4)?,
        author: row.get(5)?,
        published_at: row.get(6)?,
        content: row.get(7)?,
        summary: row.get(8)?,
        snippet: row.get(9)?,
        image: row.get(10)?,
        has_been_read: row.get::<_, i64>(11)? != 0,
        starred: row.get::<_, i64>(12)? != 0,
    })
}

pub struct UpsertEntry<'a> {
    pub guid: &'a str,
    pub title: &'a str,
    pub url: Option<&'a str>,
    pub author: Option<&'a str>,
    pub published_at: i64,
    pub content: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub snippet: Option<&'a str>,
    pub image: Option<&'a str>,
}

/// Insert an entry, skipping if (source_id, guid) already exists.
/// Returns true when a new row was created.
pub fn insert_item(conn: &Connection, source_id: i64, e: &UpsertEntry) -> Result<bool, String> {
    let n = conn
        .execute(
            "INSERT OR IGNORE INTO items (source_id, guid, title, url, author, published_at, content, summary, snippet, image, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                source_id,
                e.guid,
                e.title,
                e.url,
                e.author,
                e.published_at,
                e.content,
                e.summary,
                e.snippet,
                e.image,
                crate::models::now_ts()
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

pub fn get_items(conn: &Connection, p: &crate::models::GetItemsParams) -> Result<Vec<Item>, String> {
    let mut sql = format!("SELECT {ITEM_COLUMNS_FULL} FROM items");
    let mut conditions: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    match p.scope.as_deref().unwrap_or("all") {
        "source" => {
            args.push(Box::new(p.scope_id.unwrap_or(-1)));
            conditions.push(format!("source_id = ?{}", args.len()));
        }
        "group" => {
            args.push(Box::new(p.scope_id.unwrap_or(-1)));
            conditions.push(format!(
                "source_id IN (SELECT id FROM sources WHERE group_id = ?{})",
                args.len()
            ));
        }
        _ => {}
    }
    match p.filter.unwrap_or(0) {
        1 => conditions.push("has_been_read = 0".into()),
        2 => conditions.push("starred = 1".into()),
        _ => {}
    }
    if let Some(q) = &p.search {
        if !q.is_empty() {
            args.push(Box::new(format!("%{q}%")));
            args.push(Box::new(format!("%{q}%")));
            conditions.push(format!(
                "(title LIKE ?{} OR content LIKE ?{})",
                args.len() - 1,
                args.len()
            ));
        }
    }
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY published_at DESC, id DESC");
    let limit = p.limit.unwrap_or(200).clamp(1, 2000);
    sql.push_str(&format!(" LIMIT {limit} OFFSET {}", p.offset.unwrap_or(0)));

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let refs: Vec<&dyn rusqlite::types::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt
        .query_map(refs.as_slice(), row_to_item)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

const ITEM_COLUMNS_FULL: &str = ITEM_COLUMNS;

pub fn get_item(conn: &Connection, id: i64) -> Result<Item, String> {
    conn.query_row(
        &format!("SELECT {ITEM_COLUMNS_FULL} FROM items WHERE id = ?1"),
        params![id],
        row_to_item,
    )
    .map_err(|e| e.to_string())
}

pub fn set_items_read(conn: &Connection, ids: &[i64], read: bool) -> Result<(), String> {
    let flag = read as i64;
    for id in ids {
        conn.execute(
            "UPDATE items SET has_been_read=?1 WHERE id=?2",
            params![flag, id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn mark_all_read(
    conn: &Connection,
    scope: Option<&str>,
    scope_id: Option<i64>,
) -> Result<usize, String> {
    match scope {
        Some("source") => {
            let id = scope_id.unwrap_or(-1);
            conn.execute(
                "UPDATE items SET has_been_read=1 WHERE source_id=?1",
                params![id],
            )
        }
        Some("group") => {
            let id = scope_id.unwrap_or(-1);
            conn.execute(
                "UPDATE items SET has_been_read=1 WHERE source_id IN (SELECT id FROM sources WHERE group_id=?1)",
                params![id],
            )
        }
        _ => conn.execute("UPDATE items SET has_been_read=1", []),
    }
    .map_err(|e| e.to_string())
}

pub fn set_item_starred(conn: &Connection, id: i64, starred: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE items SET starred=?1 WHERE id=?2",
        params![starred as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_item_content(conn: &Connection, id: i64, content: &str, snippet: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE items SET content=?1, snippet=?2 WHERE id=?3",
        params![content, snippet, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_all_read() {
        let conn = Connection::open_in_memory().expect("init in-memory db");
        migrate(&conn).expect("migrate");

        let group = create_group(&conn, "Test Group").expect("create group");
        let source1 = insert_source(&conn, "https://example.com/1", "Source 1", None, Some(group.id)).expect("insert source 1");
        let source2 = insert_source(&conn, "https://example.com/2", "Source 2", None, None).expect("insert source 2");

        // Insert unread items
        conn.execute(
            "INSERT INTO items (source_id, guid, title, published_at, content, snippet, has_been_read, starred, created_at) VALUES (?1, 'g1', 'Title 1', 100, 'Content', 'Snippet', 0, 0, 100)",
            params![source1.id],
        ).expect("insert item 1");
        conn.execute(
            "INSERT INTO items (source_id, guid, title, published_at, content, snippet, has_been_read, starred, created_at) VALUES (?1, 'g2', 'Title 2', 200, 'Content', 'Snippet', 0, 0, 200)",
            params![source2.id],
        ).expect("insert item 2");

        // Mark by source
        let count = mark_all_read(&conn, Some("source"), Some(source1.id)).expect("mark by source");
        assert_eq!(count, 1);

        // Mark by group
        let count = mark_all_read(&conn, Some("group"), Some(group.id)).expect("mark by group");
        assert_eq!(count, 1);

        // Mark all (global) - this was the broken branch
        let count = mark_all_read(&conn, None, None).expect("mark all read");
        assert_eq!(count, 2);
    }
}
