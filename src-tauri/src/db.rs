use crate::models::{Group, Item, RuleActionType, RuleSourceScope, RuleTargetField, Source, SyncAction};
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
    if version == 0 {
        conn.execute_batch(
            r#"
            CREATE TABLE groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                expanded INTEGER NOT NULL DEFAULT 1,
                sort INTEGER NOT NULL DEFAULT 0,
                remote_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_groups_remote ON groups(remote_id) WHERE remote_id IS NOT NULL;

            CREATE TABLE sources (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                description TEXT,
                favicon TEXT,
                group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL,
                last_fetched INTEGER,
                error_count INTEGER NOT NULL DEFAULT 0,
                remote_id TEXT,
                last_error TEXT
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
                hidden INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                remote_id TEXT,
                UNIQUE(source_id, guid)
            );
            CREATE INDEX idx_items_source_pub ON items(source_id, published_at DESC);
            CREATE INDEX idx_items_pub ON items(published_at DESC);
            CREATE INDEX idx_items_read ON items(has_been_read);
            CREATE INDEX idx_items_starred ON items(starred);
            CREATE INDEX idx_items_hidden ON items(hidden);
            CREATE INDEX idx_items_remote ON items(remote_id) WHERE remote_id IS NOT NULL;

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
        conn.pragma_update(None, "user_version", CURRENT_VERSION).ok();
        return Ok(());
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
    if version < 4 {
        conn.execute_batch(
            r#"
            ALTER TABLE groups ADD COLUMN remote_id TEXT;
            CREATE INDEX IF NOT EXISTS idx_groups_remote ON groups(remote_id) WHERE remote_id IS NOT NULL;
            "#,
        )
        .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "user_version", 4).ok();
    }
    if version < 5 {
        conn.execute_batch("ALTER TABLE sources ADD COLUMN last_error TEXT;")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "user_version", 5).ok();
    }
    Ok(())
}

/// 当前数据库架构版本；若在上方追加迁移块请同步递增此版本号。
pub const CURRENT_VERSION: i64 = 5;

/// 测试辅助函数，供其他模块的单元测试构建完成全部迁移的内存数据库。
#[cfg(test)]
pub fn migrate_for_tests(conn: &Connection) -> Result<(), String> {
    migrate(conn)
}

impl rusqlite::ToSql for RuleTargetField {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl rusqlite::types::FromSql for RuleTargetField {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let text = value.as_str()?;
        text.parse().map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }
}

impl rusqlite::ToSql for RuleActionType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl rusqlite::types::FromSql for RuleActionType {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let text = value.as_str()?;
        text.parse().map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }
}

impl rusqlite::ToSql for RuleSourceScope {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.to_string().into())
    }
}

impl rusqlite::types::FromSql for RuleSourceScope {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let text = value.as_str()?;
        text.parse().map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }
}

impl rusqlite::ToSql for SyncAction {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl rusqlite::types::FromSql for SyncAction {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let text = value.as_str()?;
        text.parse().map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }
}

fn row_to_group(row: &Row) -> rusqlite::Result<Group> {
    Ok(Group {
        id: row.get(0)?,
        name: row.get(1)?,
        expanded: row.get::<_, i64>(2)? != 0,
        sort: row.get(3)?,
        remote_id: row.get(4)?,
    })
}

pub fn get_groups(conn: &Connection) -> Result<Vec<Group>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, expanded, sort, remote_id FROM groups ORDER BY sort, id")
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
    Ok(Group { id, name: name.into(), expanded: true, sort: 0, remote_id: None })
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
        last_error: row.get(10)?,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.url, s.title, s.description, s.favicon, s.group_id, s.last_fetched, s.error_count,
    (SELECT COUNT(*) FROM items i WHERE i.source_id = s.id AND i.has_been_read = 0) AS unread, s.remote_id, s.last_error
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

pub fn mark_source_fetched(
    conn: &Connection,
    source_id: i64,
    ok: bool,
    err: Option<&str>,
) -> Result<(), String> {
    if ok {
        conn.execute(
            "UPDATE sources SET last_fetched=?1, error_count=0, last_error=NULL WHERE id=?2",
            params![crate::models::now_ts(), source_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        let msg: String = err.unwrap_or("fetch failed").chars().take(500).collect();
        conn.execute(
            "UPDATE sources SET error_count = error_count + 1, last_error=?1 WHERE id=?2",
            params![msg, source_id],
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
    /// 预先应用的规则引擎标志位，仅在新建数据行时生效。
    pub has_been_read: bool,
    pub starred: bool,
    pub hidden: bool,
}

/// 插入一条文章，若 (source_id, guid) 已存在则跳过。
/// 若创建了新数据行则返回 true。
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
    // 用户输入的 FTS5 MATCH 语法可能会被拒绝；发生任何错误时回退到 LIKE 模糊查询。
    match get_items_impl(conn, p, true) {
        Ok(items) => Ok(items),
        Err(_) => {
            // 仅使用静态日志消息：被拒绝的 MATCH 文本属于用户输入，
            // 绝不记入日志。
            log::warn!("items fts match rejected, like fallback");
            get_items_impl(conn, p, false)
        }
    }
}

/// 构建安全的 FTS5 MATCH 表达式：每个空格分词均被双引号包裹，并用 AND 连接。
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
        // 3 = 仅隐藏文章的审查列表，供规则编辑器使用
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
                // LIKE 回退查询镜像了 FTS 索引字段（标题/摘要/正文）
                // 并补充了作者字段，确保在 FTS 拒绝查询语法时
                // 搜索结果保持一致。
                for _ in 0..4 {
                    args.push(Box::new(format!("%{q}%")));
                }
                let base = args.len() - 3;
                conditions.push(format!(
                    "(title LIKE ?{base} OR summary LIKE ?{} OR content LIKE ?{} OR author LIKE ?{})",
                    base + 1,
                    base + 2,
                    base + 3
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

// ---------- 正则规则 ----------

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

// ---------- 统计与数据保留 ----------

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

/// 应用数据保留策略：清理超出保留时间窗口的未加星标文章，
/// 随后限制每个订阅源的未加星标历史文章数量。返回已删除的文章数量。
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
        // 针对所有订阅源的单条语句：在各订阅源内部按时间倒序排列，
        // 删除超出单源上限的未加星标数据行。星标文章绝不删除，
        // 即使它们位于保留范围之外。
        deleted += tx
            .execute(
                "DELETE FROM items WHERE starred = 0 AND id IN (
                    SELECT id FROM (
                        SELECT id, ROW_NUMBER() OVER (
                            PARTITION BY source_id ORDER BY published_at DESC, id DESC
                        ) AS rn FROM items
                    ) WHERE rn > ?1)",
                params![max_per_source as i64],
            )
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(deleted)
}

pub fn vacuum(conn: &Connection) -> Result<(), String> {
    conn.execute("VACUUM", [])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------- 云端同步 ----------

pub fn get_group(conn: &Connection, id: i64) -> Result<Option<Group>, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort, remote_id FROM groups WHERE id = ?1",
        params![id],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn get_group_by_remote_id(conn: &Connection, remote_id: &str) -> Result<Option<Group>, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort, remote_id FROM groups WHERE remote_id = ?1",
        params![remote_id],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_group_remote(conn: &Connection, group_id: i64, remote_id: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE groups SET remote_id=?1 WHERE id=?2",
        params![remote_id, group_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn find_or_create_group(conn: &Connection, name: &str) -> Result<Group, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort, remote_id FROM groups WHERE name = ?1",
        params![name],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .map(Ok)
    .unwrap_or_else(|| create_group(conn, name))
}

pub fn find_or_create_group_by_remote(
    conn: &Connection,
    remote_id: &str,
    name: &str,
) -> Result<Group, String> {
    if let Some(mut g) = get_group_by_remote_id(conn, remote_id)? {
        if g.name != name {
            rename_group(conn, g.id, name)?;
            g.name = name.to_string();
        }
        return Ok(g);
    }
    let existing = conn
        .query_row(
            "SELECT id, name, expanded, sort, remote_id FROM groups WHERE name = ?1",
            params![name],
            row_to_group,
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(mut g) = existing {
        set_group_remote(conn, g.id, Some(remote_id))?;
        g.remote_id = Some(remote_id.to_string());
        Ok(g)
    } else {
        conn.execute(
            "INSERT INTO groups (name, remote_id) VALUES (?1, ?2)",
            params![name, remote_id],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        Ok(Group {
            id,
            name: name.to_string(),
            expanded: true,
            sort: 0,
            remote_id: Some(remote_id.to_string()),
        })
    }
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

/// 插入或对齐单篇远端文章。匹配顺序：优先按远端 ID，其次按 (source, url)
/// 匹配在绑定同步前已从本地抓取的数据行。匹配成功时远端已读/加星状态
/// 将覆盖本地（服务端优先 / LWW 规则），并与调用方的规则引擎标志位进行逻辑或
/// （本地规则的标记已读/加星/隐藏叠加生效）；若未匹配则插入新数据行。
/// 若创建了新数据行则返回 true。
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
    /// 规则引擎“隐藏”标志（从常规列表中排除）。
    pub hidden: bool,
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
                "UPDATE items SET has_been_read=?1, starred=?2, hidden=?3, remote_id=?4 WHERE id=?5",
                params![r.has_been_read as i64, r.starred as i64, r.hidden as i64, r.remote_id, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(false)
        }
        None => {
            conn.execute(
                "INSERT INTO items (source_id, guid, title, url, author, published_at, content, summary, snippet, remote_id, has_been_read, starred, hidden, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    r.source_id,
                    r.remote_id, // guid：单个订阅源内稳定唯一
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
                    r.hidden as i64,
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
    pub action: SyncAction,
    pub target: String,
}

/// 为每篇文章加入一条操作队列记录，跳过没有远端 ID 的文章。
/// 返回入队的操作条数。
pub fn enqueue_item_actions(conn: &Connection, item_ids: &[i64], action: SyncAction) -> Result<usize, String> {
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

/// 入队一个流级别的操作（例如对整个 feed / label / reading-list 全部标记为已读）。
pub fn enqueue_stream_action(conn: &Connection, action: SyncAction, target: &str) -> Result<(), String> {
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
    use crate::models::{GetItemsParams, SyncAction};

    #[test]
    fn test_items_pagination_is_stable() {
        let conn = Connection::open_in_memory().expect("init db");
        migrate(&conn).expect("migrate");
        let s = insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        for i in 1..=5 {
            insert_item(&conn, s.id, &item(&format!("g{i}"), "t", "x", i, false, false, false)).unwrap();
        }
        // 给定：按时间从新到旧排序（published_at 5..1）
        // 当：以每页 2 条进行分页获取时
        let page = |offset| {
            get_items(&conn, &GetItemsParams { limit: Some(2), offset: Some(offset), ..Default::default() }).unwrap()
        };
        let (p0, p1, p2) = (page(0), page(2), page(4));
        // 那么：各页平铺完整列表，无重叠且无遗漏
        let ids = |v: Vec<Item>| v.into_iter().map(|i| i.guid).collect::<Vec<_>>();
        assert_eq!(ids(p0), vec!["g5".to_string(), "g4".to_string()]);
        assert_eq!(ids(p1), vec!["g3".to_string(), "g2".to_string()]);
        assert_eq!(ids(p2), vec!["g1".to_string()]);
    }

    #[test]
    fn test_like_fallback_covers_summary_and_author() {
        let conn = Connection::open_in_memory().expect("init db");
        migrate(&conn).expect("migrate");
        let s = insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        insert_item(
            &conn,
            s.id,
            &UpsertEntry {
                guid: "g1",
                title: "plain title",
                url: None,
                author: Some("Austen"),
                published_at: 100,
                content: Some("boring body"),
                summary: Some("unique-summary-xyz"),
                snippet: Some("s"),
                image: None,
                has_been_read: false,
                starred: false,
                hidden: false,
            },
        )
        .unwrap();

        // 给定：FTS 路径不可用（模拟被拒绝的 MATCH 查询）
        // 当/那么：通过 LIKE 回退仍然能匹配到作者和摘要
        for q in ["Austen", "unique-summary-xyz", "boring", "plain"] {
            let found = get_items_impl(
                &conn,
                &GetItemsParams { search: Some(q.into()), ..Default::default() },
                false,
            )
            .unwrap();
            assert_eq!(found.len(), 1, "fallback should match {q}");
        }
    }

    #[test]
    fn test_mark_source_fetched_records_error() {
        let conn = Connection::open_in_memory().expect("init in-memory db");
        migrate(&conn).expect("migrate");

        let s = insert_source(&conn, "https://example.com/1", "Source 1", None, None).expect("insert");

        // 给定：一次附带原因的失败抓取
        mark_source_fetched(&conn, s.id, false, Some("HTTP 503")).expect("mark failed");
        // 那么：错误计数递增且错误原因被持久化
        let fetched = get_source(&conn, s.id).expect("get source");
        assert_eq!(fetched.error_count, 1);
        assert_eq!(fetched.last_error.as_deref(), Some("HTTP 503"));

        // 给定：后续成功抓取
        mark_source_fetched(&conn, s.id, true, None).expect("mark ok");
        // 那么：计数器清零且过期的错误原因被清除
        let fetched = get_source(&conn, s.id).expect("get source");
        assert_eq!(fetched.error_count, 0);
        assert_eq!(fetched.last_error, None);
    }

    #[test]
    fn test_mark_all_read() {
        let conn = Connection::open_in_memory().expect("init in-memory db");
        migrate(&conn).expect("migrate");

        let group = create_group(&conn, "Test Group").expect("create group");
        let source1 = insert_source(&conn, "https://example.com/1", "Source 1", None, Some(group.id)).expect("insert source 1");
        let source2 = insert_source(&conn, "https://example.com/2", "Source 2", None, None).expect("insert source 2");

        // 插入未读文章
        conn.execute(
            "INSERT INTO items (source_id, guid, title, published_at, content, snippet, has_been_read, starred, created_at) VALUES (?1, 'g1', 'Title 1', 100, 'Content', 'Snippet', 0, 0, 100)",
            params![source1.id],
        ).expect("insert item 1");
        conn.execute(
            "INSERT INTO items (source_id, guid, title, published_at, content, snippet, has_been_read, starred, created_at) VALUES (?1, 'g2', 'Title 2', 200, 'Content', 'Snippet', 0, 0, 200)",
            params![source2.id],
        ).expect("insert item 2");

        // 按订阅源标记已读
        let count = mark_all_read(&conn, Some("source"), Some(source1.id)).expect("mark by source");
        assert_eq!(count, 1);

        // 按分组标记已读
        let count = mark_all_read(&conn, Some("group"), Some(group.id)).expect("mark by group");
        assert_eq!(count, 1);

        // 全部标记已读（全局）
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

        // FTS 通过 AND 组合分词跨标题和正文匹配
        let found = get_items(&conn, &GetItemsParams { search: Some("tokio dive".into()), ..Default::default() }).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].guid, "g1");

        // 隐藏文章排除在常规列表之外，但在 filter=3 时展示
        let all = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(all.len(), 1);
        let hidden = get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].guid, "g2");

        // 全文更新通过触发器同步更新到 FTS 索引
        set_item_content(&conn, found[0].id, "quantum computing", "quantum").unwrap();
        let found2 = get_items(&conn, &GetItemsParams { search: Some("quantum".into()), ..Default::default() }).unwrap();
        assert_eq!(found2.len(), 1);

        // 删除操作通过触发器同步清除 FTS 索引（从订阅源级联删除）
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

        // 30天保留策略：旧的未加星标文章（已读或隐藏）被删除，星标文章保留
        let deleted = cleanup_retention(&conn, 30, 0).unwrap();
        assert_eq!(deleted, 2);
        let hidden_list = get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap();
        assert!(hidden_list.is_empty());
        let remaining = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(remaining.len(), 2);
        assert!(remaining.iter().any(|i| i.guid == "b"));
        assert!(remaining.iter().any(|i| i.guid == "c"));

        // 单源数量上限仅保留最新的 N 篇未加星标文章
        for i in 0..4 {
            insert_item(&conn, s.id, &item(&format!("n{i}"), "recent", "x", now - i * 3600, false, false, false)).unwrap();
        }
        // 未加星标集合 = c (10天前) + n0..n3 → 上限 3 保留 n0..n2，丢弃 n3 和 c
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

        // 首次 upsert 插入带有远端已读状态的数据行
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
                hidden: false,
            },
        )
        .unwrap();
        assert!(inserted);

        // 第二次拉取且服务端状态发生变化：LWW 覆盖，不产生新行
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
                hidden: false,
            },
        )
        .unwrap();
        assert!(!again);
        let items = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert!(!items[0].has_been_read);
        assert!(items[0].starred);

        // 本地既有数据行（无远端 ID）通过 (source, url) 成功关联匹配
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
                hidden: false,
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

        // 同步队列：无远端 ID 的文章操作被跳过
        let with_remote = items[0].id; // 匹配远端 ID "abc"
        insert_item(&conn, s.id, &item("plain", "never synced", "x", 40, false, false, false)).unwrap();
        let no_remote: i64 = conn
            .query_row("SELECT id FROM items WHERE guid='plain'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(enqueue_item_actions(&conn, &[with_remote], SyncAction::MarkUnread).unwrap(), 1);
        assert_eq!(enqueue_item_actions(&conn, &[no_remote], SyncAction::MarkRead).unwrap(), 0);
        enqueue_stream_action(&conn, SyncAction::MarkAllRead, "user/-/state/com.google/reading-list").unwrap();
        let queue = queue_fetch(&conn, 100).unwrap();
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].action, SyncAction::MarkUnread);
        assert_eq!(queue[1].action, SyncAction::MarkAllRead);
        queue_delete(&conn, &[queue[0].id]).unwrap();
        assert_eq!(queue_len(&conn).unwrap(), 1);
        queue_clear(&conn).unwrap();
        assert_eq!(queue_len(&conn).unwrap(), 0);

        // sync_state 游标往返测试
        assert_eq!(get_state(&conn, "greader.last_sync").unwrap(), None);
        set_state(&conn, "greader.last_sync", "123").unwrap();
        set_state(&conn, "greader.last_sync", "456").unwrap();
        assert_eq!(get_state(&conn, "greader.last_sync").unwrap().as_deref(), Some("456"));

        // find_or_create_group 按名称具有幂等性
        let g1 = find_or_create_group(&conn, "Tech").unwrap();
        let g2 = find_or_create_group(&conn, "Tech").unwrap();
        assert_eq!(g1.id, g2.id);

        // find_or_create_group_by_remote 关联既有同名分组或创建带有 remote_id 的新分组
        let g3 = find_or_create_group_by_remote(&conn, "user/-/label/Tech", "Tech").unwrap();
        assert_eq!(g3.id, g1.id);
        assert_eq!(g3.remote_id.as_deref(), Some("user/-/label/Tech"));
        assert_eq!(get_group_by_remote_id(&conn, "user/-/label/Tech").unwrap().map(|g| g.id), Some(g1.id));

        // 远端重命名同步更新本地名称
        let g4 = find_or_create_group_by_remote(&conn, "user/-/label/Tech", "Technology").unwrap();
        assert_eq!(g4.id, g1.id);
        assert_eq!(g4.name, "Technology");
    }
}
