//! 云同步队列与状态游标（零行为变更：由 db.rs 按域拆分而来，SQL 逐字不变）。

use crate::models::SyncAction;
use rusqlite::{params, Connection, OptionalExtension};

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
