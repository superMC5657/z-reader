//! 数据库 schema 与迁移链（零行为变更：由 db.rs 按域拆分而来，SQL 逐字不变）。
//!
//! 另含 rusqlite 与规则/同步枚举之间的类型转换 plumbing。

use crate::models::{RuleActionType, RuleSourceScope, RuleTargetField, SyncAction};
use rusqlite::Connection;
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

pub(crate) fn migrate(conn: &Connection) -> Result<(), String> {
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
