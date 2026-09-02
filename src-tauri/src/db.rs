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
    if version < 2 {
        conn.execute_batch(
            r#"
            ALTER TABLE items ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
            CREATE INDEX idx_items_hidden ON items(hidden);
            CREATE TABLE IF NOT EXISTS regex_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                pattern TEXT NOT NULL,
                target_field TEXT NOT NULL,
                action_type TEXT NOT NULL,
                is_case_sensitive INTEGER NOT NULL DEFAULT 0,
                is_enabled INTEGER NOT NULL DEFAULT 1,
                source_scope TEXT NOT NULL DEFAULT 'all',
                created_at INTEGER NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
                title, summary, content,
                content='items', content_rowid='id', tokenize='unicode61'
            );
            CREATE TRIGGER IF NOT EXISTS items_fts_ai AFTER INSERT ON items BEGIN
                INSERT INTO items_fts(rowid, title, summary, content)
                VALUES (new.id, new.title, new.summary, new.content);
            END;
            CREATE TRIGGER IF NOT EXISTS items_fts_ad AFTER DELETE ON items BEGIN
                INSERT INTO items_fts(items_fts, rowid, title, summary, content)
                VALUES ('delete', old.id, old.title, old.summary, old.content);
            END;
            CREATE TRIGGER IF NOT EXISTS items_fts_au AFTER UPDATE OF title, summary, content ON items BEGIN
                INSERT INTO items_fts(items_fts, rowid, title, summary, content)
                VALUES ('delete', old.id, old.title, old.summary, old.content);
                INSERT INTO items_fts(rowid, title, summary, content)
                VALUES (new.id, new.title, new.summary, new.content);
            END;
            INSERT INTO items_fts(items_fts) VALUES('rebuild');
            "#,
        )
        .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "user_version", 2).ok();
    }
    if version < 3 {
        conn.execute_batch(
            r#"
            ALTER TABLE sources ADD COLUMN remote_id TEXT;
            ALTER TABLE items ADD COLUMN remote_id TEXT;
            CREATE INDEX idx_items_remote ON items(remote_id) WHERE remote_id IS NOT NULL;
            CREATE TABLE IF NOT EXISTS sync_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sync_state (
                key TEXT PRIMARY KEY,
                value TEXT
            );
            "#,
        )
        .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "user_version", 3).ok();
    }
    Ok(())
}

/// Current schema version; bump when adding a migration block above.
pub const CURRENT_VERSION: i64 = 3;

/// Test helper so other modules' tests can build a fully-migrated in-memory DB.
#[cfg(test)]
pub fn migrate_for_tests(conn: &Connection) -> Result<(), String> {
    migrate(conn)
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
        remote_id: row.get(9)?,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.url, s.title, s.description, s.favicon, s.group_id, s.last_fetched, s.error_count,
    (SELECT COUNT(*) FROM items i WHERE i.source_id = s.id AND i.has_been_read = 0) AS unread, s.remote_id
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

const ITEM_COLUMNS: &str = "id, source_id, guid, title, url, author, published_at, content, summary, snippet, image, has_been_read, starred, hidden, remote_id";

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
        hidden: row.get::<_, i64>(13)? != 0,
        remote_id: row.get(14)?,
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
    /// Pre-applied rule-engine flags, applied only when a new row is created.
    pub has_been_read: bool,
    pub starred: bool,
    pub hidden: bool,
}

/// Insert an entry, skipping if (source_id, guid) already exists.
/// Returns true when a new row was created.
pub fn insert_item(conn: &Connection, source_id: i64, e: &UpsertEntry) -> Result<bool, String> {
    let n = conn
        .execute(
            "INSERT OR IGNORE INTO items (source_id, guid, title, url, author, published_at, content, summary, snippet, image, has_been_read, starred, hidden, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                e.has_been_read as i64,
                e.starred as i64,
                e.hidden as i64,
                crate::models::now_ts()
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

pub fn get_items(conn: &Connection, p: &crate::models::GetItemsParams) -> Result<Vec<Item>, String> {
    // FTS5 MATCH syntax from user input can be rejected; fall back to LIKE on any error.
    match get_items_impl(conn, p, true) {
        Ok(items) => Ok(items),
        Err(_) => get_items_impl(conn, p, false),
    }
}

/// Build a safe FTS5 MATCH expression: each whitespace token is quoted, joined with AND.
fn fts_match_query(q: &str) -> Option<String> {
    let tokens: Vec<String> = q
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}

fn get_items_impl(
    conn: &Connection,
    p: &crate::models::GetItemsParams,
    allow_fts: bool,
) -> Result<Vec<Item>, String> {
    let mut sql = format!("SELECT {ITEM_COLUMNS} FROM items");
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
        1 => conditions.push("has_been_read = 0 AND hidden = 0".into()),
        2 => conditions.push("starred = 1 AND hidden = 0".into()),
        // 3 = hidden-only review list used by the rules editor
        3 => conditions.push("hidden = 1".into()),
        _ => conditions.push("hidden = 0".into()),
    }
    if let Some(q) = &p.search {
        if !q.is_empty() {
            let mut matched = false;
            if allow_fts {
                if let Some(match_q) = fts_match_query(q) {
                    args.push(Box::new(match_q));
                    conditions.push(format!(
                        "id IN (SELECT rowid FROM items_fts WHERE items_fts MATCH ?{})",
                        args.len()
                    ));
                    matched = true;
                }
            }
            if !matched {
                args.push(Box::new(format!("%{q}%")));
                args.push(Box::new(format!("%{q}%")));
                conditions.push(format!(
                    "(title LIKE ?{} OR content LIKE ?{})",
                    args.len() - 1,
                    args.len()
                ));
            }
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

pub fn get_item(conn: &Connection, id: i64) -> Result<Item, String> {
    conn.query_row(
        &format!("SELECT {ITEM_COLUMNS} FROM items WHERE id = ?1"),
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

pub fn set_item_hidden(conn: &Connection, id: i64, hidden: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE items SET hidden=?1 WHERE id=?2",
        params![hidden as i64, id],
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

// ---------- Regex rules ----------

fn row_to_rule(row: &Row) -> rusqlite::Result<crate::models::Rule> {
    Ok(crate::models::Rule {
        id: row.get(0)?,
        name: row.get(1)?,
        pattern: row.get(2)?,
        target_field: row.get(3)?,
        action_type: row.get(4)?,
        is_case_sensitive: row.get::<_, i64>(5)? != 0,
        is_enabled: row.get::<_, i64>(6)? != 0,
        source_scope: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub fn get_rules(conn: &Connection) -> Result<Vec<crate::models::Rule>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, pattern, target_field, action_type, is_case_sensitive, is_enabled, source_scope, created_at
             FROM regex_rules ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_rule)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn create_rule(conn: &Connection, r: &crate::models::RuleInput) -> Result<crate::models::Rule, String> {
    conn.execute(
        "INSERT INTO regex_rules (name, pattern, target_field, action_type, is_case_sensitive, is_enabled, source_scope, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            r.name,
            r.pattern,
            r.target_field,
            r.action_type,
            r.is_case_sensitive as i64,
            r.is_enabled as i64,
            r.source_scope,
            crate::models::now_ts()
        ],
    )
    .map_err(|e| e.to_string())?;
    get_rule(conn, conn.last_insert_rowid())
}

pub fn get_rule(conn: &Connection, id: i64) -> Result<crate::models::Rule, String> {
    conn.query_row(
        "SELECT id, name, pattern, target_field, action_type, is_case_sensitive, is_enabled, source_scope, created_at
         FROM regex_rules WHERE id = ?1",
        params![id],
        row_to_rule,
    )
    .map_err(|e| e.to_string())
}

pub fn update_rule(conn: &Connection, id: i64, r: &crate::models::RuleInput) -> Result<(), String> {
    conn.execute(
        "UPDATE regex_rules SET name=?1, pattern=?2, target_field=?3, action_type=?4, is_case_sensitive=?5, is_enabled=?6, source_scope=?7 WHERE id=?8",
        params![
            r.name,
            r.pattern,
            r.target_field,
            r.action_type,
            r.is_case_sensitive as i64,
            r.is_enabled as i64,
            r.source_scope,
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM regex_rules WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- Stats & retention ----------

pub fn total_unread(conn: &Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM items WHERE has_been_read = 0",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

pub fn item_count(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

/// Apply the retention policy: drop unstarred articles past the retention
/// window, then cap each source's unstarred history. Returns deleted count.
pub fn cleanup_retention(
    conn: &Connection,
    retention_days: u32,
    max_per_source: u32,
) -> Result<usize, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    let mut deleted = 0usize;
    if retention_days > 0 {
        let cutoff = crate::models::now_ts() - (retention_days as i64) * 86_400;
        deleted += tx
            .execute(
                "DELETE FROM items WHERE starred = 0 AND published_at < ?1 AND (has_been_read = 1 OR hidden = 1)",
                params![cutoff],
            )
            .map_err(|e| e.to_string())?;
    }
    if max_per_source > 0 {
        let source_ids: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM sources").map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        };
        for sid in source_ids {
            deleted += tx
                .execute(
                    "DELETE FROM items WHERE source_id = ?1 AND starred = 0 AND id NOT IN (
                        SELECT id FROM items WHERE source_id = ?1
                        ORDER BY published_at DESC, id DESC LIMIT ?2)",
                    params![sid, max_per_source as i64],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(deleted)
}

pub fn vacuum(conn: &Connection) -> Result<(), String> {
    conn.execute("VACUUM", [])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------- Cloud sync ----------

pub fn get_group(conn: &Connection, id: i64) -> Result<Option<Group>, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort FROM groups WHERE id = ?1",
        params![id],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn find_or_create_group(conn: &Connection, name: &str) -> Result<Group, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort FROM groups WHERE name = ?1",
        params![name],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .map(Ok)
    .unwrap_or_else(|| create_group(conn, name))
}

pub fn get_source_by_remote_id(conn: &Connection, remote_id: &str) -> Result<Option<Source>, String> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.remote_id = ?1"),
        params![remote_id],
        row_to_source,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_source_remote(conn: &Connection, source_id: i64, remote_id: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET remote_id=?1 WHERE id=?2",
        params![remote_id, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Insert or reconcile one remote article. Matching order: by remote id, then
/// by (source, url) for rows that were fetched locally before linking. On
/// match the remote read/starred state overwrites local (server wins / LWW);
/// otherwise a new row is inserted. Returns true when a new row was created.
pub struct RemoteItemUpsert<'a> {
    pub remote_id: &'a str,
    pub source_id: i64,
    pub title: &'a str,
    pub url: Option<&'a str>,
    pub author: Option<&'a str>,
    pub published_at: i64,
    pub content: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub snippet: Option<&'a str>,
    pub has_been_read: bool,
    pub starred: bool,
}

pub fn upsert_remote_item(conn: &Connection, r: &RemoteItemUpsert) -> Result<bool, String> {
    let mut existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM items WHERE remote_id = ?1",
            params![r.remote_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if existing.is_none() {
        if let Some(url) = r.url.filter(|u| !u.is_empty()) {
            existing = conn
                .query_row(
                    "SELECT id FROM items WHERE source_id = ?1 AND url = ?2 AND remote_id IS NULL",
                    params![r.source_id, url],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
        }
    }
    match existing {
        Some(id) => {
            conn.execute(
                "UPDATE items SET has_been_read=?1, starred=?2, remote_id=?3 WHERE id=?4",
                params![r.has_been_read as i64, r.starred as i64, r.remote_id, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(false)
        }
        None => {
            conn.execute(
                "INSERT INTO items (source_id, guid, title, url, author, published_at, content, summary, snippet, remote_id, has_been_read, starred, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    r.source_id,
                    r.remote_id, // guid: stable per source
                    r.title,
                    r.url,
                    r.author,
                    r.published_at,
                    r.content,
                    r.summary,
                    r.snippet,
                    r.remote_id,
                    r.has_been_read as i64,
                    r.starred as i64,
                    crate::models::now_ts()
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(true)
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueueEntry {
    pub id: i64,
    pub action: String,
    pub target: String,
}

/// Queue one action per item, skipping items without a remote id.
pub fn enqueue_item_actions(conn: &Connection, item_ids: &[i64], action: &str) -> Result<usize, String> {
    let mut n = 0usize;
    for id in item_ids {
        let remote: Option<String> = conn
            .query_row("SELECT remote_id FROM items WHERE id=?1", params![id], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if let Some(target) = remote.filter(|t| !t.is_empty()) {
            conn.execute(
                "INSERT INTO sync_queue (action, target, created_at) VALUES (?1, ?2, ?3)",
                params![action, target, crate::models::now_ts()],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

/// Queue a stream-level action (e.g. mark-all-read for feed/label/reading-list).
pub fn enqueue_stream_action(conn: &Connection, action: &str, target: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO sync_queue (action, target, created_at) VALUES (?1, ?2, ?3)",
        params![action, target, crate::models::now_ts()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn queue_fetch(conn: &Connection, limit: i64) -> Result<Vec<QueueEntry>, String> {
    let mut stmt = conn
        .prepare("SELECT id, action, target FROM sync_queue ORDER BY id LIMIT ?1")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(QueueEntry {
                id: row.get(0)?,
                action: row.get(1)?,
                target: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn queue_delete(conn: &Connection, ids: &[i64]) -> Result<(), String> {
    for id in ids {
        conn.execute("DELETE FROM sync_queue WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn queue_len(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM sync_queue", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

pub fn queue_clear(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM sync_queue", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn get_state(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM sync_state WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_state(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GetItemsParams;

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

    #[allow(clippy::too_many_arguments)]
    fn item<'a>(
        guid: &'a str,
        title: &'a str,
        content: &'a str,
        published_at: i64,
        read: bool,
        starred: bool,
        hidden: bool,
    ) -> UpsertEntry<'a> {
        UpsertEntry {
            guid,
            title,
            url: None,
            author: None,
            published_at,
            content: Some(content),
            summary: None,
            snippet: Some(content),
            image: None,
            has_been_read: read,
            starred,
            hidden,
        }
    }

    #[test]
    fn test_fts_search_and_hidden_filter() {
        let conn = Connection::open_in_memory().expect("init db");
        migrate(&conn).expect("migrate");
        let s = insert_source(&conn, "https://e.example", "E", None, None).expect("source");

        insert_item(&conn, s.id, &item("g1", "Rust async guide", "<p>tokio runtime deep dive</p>", 100, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("g2", "cooking blog", "pasta recipe", 200, false, false, true)).unwrap();

        // FTS matches across title and content with AND-combined tokens
        let found = get_items(&conn, &GetItemsParams { search: Some("tokio dive".into()), ..Default::default() }).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].guid, "g1");

        // hidden items stay out of the normal list but show up under filter=3
        let all = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(all.len(), 1);
        let hidden = get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].guid, "g2");

        // full-content updates flow into the FTS index via trigger
        set_item_content(&conn, found[0].id, "quantum computing", "quantum").unwrap();
        let found2 = get_items(&conn, &GetItemsParams { search: Some("quantum".into()), ..Default::default() }).unwrap();
        assert_eq!(found2.len(), 1);

        // deletes flow into the FTS index via trigger (cascade from source)
        remove_source(&conn, s.id).unwrap();
        let found3 = get_items(&conn, &GetItemsParams { search: Some("quantum".into()), ..Default::default() }).unwrap();
        assert!(found3.is_empty());
    }

    #[test]
    fn test_retention_cleanup() {
        let conn = Connection::open_in_memory().expect("init db");
        migrate(&conn).expect("migrate");
        let s = insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        let now = crate::models::now_ts();
        let day: i64 = 86_400;

        insert_item(&conn, s.id, &item("a", "old read", "x", now - 100 * day, true, false, false)).unwrap();
        insert_item(&conn, s.id, &item("b", "old starred", "x", now - 100 * day, false, true, false)).unwrap();
        insert_item(&conn, s.id, &item("c", "recent read", "x", now - 10 * day, true, false, false)).unwrap();
        insert_item(&conn, s.id, &item("d", "old hidden", "x", now - 100 * day, false, false, true)).unwrap();

        // 30-day retention: old unstarred (read or hidden) go, starred stays
        let deleted = cleanup_retention(&conn, 30, 0).unwrap();
        assert_eq!(deleted, 2);
        let hidden_list = get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap();
        assert!(hidden_list.is_empty());
        let remaining = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|i| i.guid == "b"));
        assert!(remaining.iter().any(|i| i.guid == "c"));

        // per-source cap keeps only the newest N unstarred items
        for i in 0..4 {
            insert_item(&conn, s.id, &item(&format!("n{i}"), "recent", "x", now - i * 3600, false, false, false)).unwrap();
        }
        // unstarred set = c (now-10d) + n0..n3 → cap 3 keeps n0..n2, drops n3 and c
        let deleted = cleanup_retention(&conn, 0, 3).unwrap();
        assert_eq!(deleted, 2);
        let remaining = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(remaining.len(), 4);
        assert!(!remaining.iter().any(|i| i.guid == "c"));
        assert!(remaining.iter().any(|i| i.guid == "b"));
    }

    #[test]
    fn test_sync_upsert_queue_and_state() {
        let conn = Connection::open_in_memory().expect("init db");
        migrate(&conn).expect("migrate");
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);

        let s = insert_source(&conn, "https://e.example", "E", None, None).unwrap();
        set_source_remote(&conn, s.id, Some("feed/2")).unwrap();
        let found = get_source_by_remote_id(&conn, "feed/2").unwrap().unwrap();
        assert_eq!(found.id, s.id);

        // first upsert inserts a row carrying the remote read state
        let inserted = upsert_remote_item(
            &conn,
            &RemoteItemUpsert {
                remote_id: "abc",
                source_id: s.id,
                title: "t1",
                url: Some("https://e.example/a"),
                author: None,
                published_at: 100,
                content: Some("<p>c</p>"),
                summary: None,
                snippet: Some("c"),
                has_been_read: true,
                starred: false,
            },
        )
        .unwrap();
        assert!(inserted);

        // second pull with changed server state: LWW overwrite, no new row
        let again = upsert_remote_item(
            &conn,
            &RemoteItemUpsert {
                remote_id: "abc",
                source_id: s.id,
                title: "t1",
                url: Some("https://e.example/a"),
                author: None,
                published_at: 100,
                content: Some("<p>c</p>"),
                summary: None,
                snippet: Some("c"),
                has_been_read: false,
                starred: true,
            },
        )
        .unwrap();
        assert!(!again);
        let items = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert!(!items[0].has_been_read);
        assert!(items[0].starred);

        // a pre-existing local row (no remote id) is matched by (source, url)
        insert_item(&conn, s.id, &item("local", "local row", "x", 50, false, false, false)).unwrap();
        conn.execute(
            "UPDATE items SET url='https://e.example/b' WHERE guid='local'",
            [],
        )
        .unwrap();
        let matched = upsert_remote_item(
            &conn,
            &RemoteItemUpsert {
                remote_id: "def",
                source_id: s.id,
                title: "local row",
                url: Some("https://e.example/b"),
                author: None,
                published_at: 50,
                content: Some("x"),
                summary: None,
                snippet: Some("x"),
                has_been_read: true,
                starred: false,
            },
        )
        .unwrap();
        assert!(!matched);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let rid: Option<String> = conn
            .query_row("SELECT remote_id FROM items WHERE guid='local'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rid.as_deref(), Some("def"));

        // queue: actions for items without a remote id are skipped
        let with_remote = items[0].id; // matched by remote id "abc"
        insert_item(&conn, s.id, &item("plain", "never synced", "x", 40, false, false, false)).unwrap();
        let no_remote: i64 = conn
            .query_row("SELECT id FROM items WHERE guid='plain'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(enqueue_item_actions(&conn, &[with_remote], "mark_unread").unwrap(), 1);
        assert_eq!(enqueue_item_actions(&conn, &[no_remote], "mark_read").unwrap(), 0);
        enqueue_stream_action(&conn, "mark_all_read", "user/-/state/com.google/reading-list").unwrap();
        let queue = queue_fetch(&conn, 100).unwrap();
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].action, "mark_unread");
        assert_eq!(queue[1].action, "mark_all_read");
        queue_delete(&conn, &[queue[0].id]).unwrap();
        assert_eq!(queue_len(&conn).unwrap(), 1);
        queue_clear(&conn).unwrap();
        assert_eq!(queue_len(&conn).unwrap(), 0);

        // sync_state cursor roundtrip
        assert_eq!(get_state(&conn, "greader.last_sync").unwrap(), None);
        set_state(&conn, "greader.last_sync", "123").unwrap();
        set_state(&conn, "greader.last_sync", "456").unwrap();
        assert_eq!(get_state(&conn, "greader.last_sync").unwrap().as_deref(), Some("456"));

        // find_or_create_group is idempotent by name
        let g1 = find_or_create_group(&conn, "Tech").unwrap();
        let g2 = find_or_create_group(&conn, "Tech").unwrap();
        assert_eq!(g1.id, g2.id);
    }
}
