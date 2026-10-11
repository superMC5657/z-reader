//! 文章条目（含 FTS5 查询与 LIKE 回退、远端 upsert 对齐、统计与保留策略；
//! 零行为变更：由 db.rs 按域拆分而来，SQL 逐字不变）。

use crate::models::Item;
use rusqlite::{params, Connection, OptionalExtension, Row};

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
/// 规则标志位（`has_been_read/starred/hidden`）须来自
/// `services::article::verdict_for_local` 的一次裁决，仅在新建行时写入。
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
pub(crate) fn fts_match_query(q: &str) -> Option<String> {
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

pub(crate) fn get_items_impl(
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

/// 插入或对齐单篇远端文章。匹配顺序：优先按远端 ID，其次按 (source, url)
/// 匹配在绑定同步前已从本地抓取的数据行。调用方标志位须来自
/// `services::article::verdict_for_remote` 的一次裁决（单一推导入口，hide→已读
/// 折叠与服务端 OR 已在模块内完成）。匹配成功时远端已读/加星状态
/// 将覆盖本地（服务端优先 / LWW 规则），规则加星在其上叠加；隐藏单调只置1不清0
///（`hidden=CASE WHEN ?3<>0 THEN 1 ELSE hidden END`，quiet 重放不回清）；若未匹配则插入新数据行。
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
            // hidden 单调：只置1不清0；read/star 保持 LWW 覆盖。
            conn.execute(
                "UPDATE items SET has_been_read=?1, starred=?2, hidden=CASE WHEN ?3 <> 0 THEN 1 ELSE hidden END, remote_id=?4 WHERE id=?5",
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
