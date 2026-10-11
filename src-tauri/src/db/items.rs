//! 文章条目持久化 Module：查询/写入/保留/整理的唯一 cohesive interface（ADR-0002）。
//!
//! 本模块拥有条目域的全部深度：查询（`get_items` 经 FTS5 + LIKE 回退）、
//! 写入（`insert_item` 新建、`upsert_remote_item` 远端对齐）、保留（`cleanup_retention`）
//! 与整理（`vacuum`）。SQL 语义逐字不变（含 Monotonic Hidden `CASE` 与 FTS/LIKE 回退）。
//! `db.rs` 仅为窄 re-export 壳（flat `crate::db::X`），深度均收敛于此；删除壳不带走任何实现。
//! 本地 SQLite 为 UI 唯一数据源（ADR-0002 local-SQLite-sole-UI-source，本模块加强之）。
//!
//! 零行为变更：由 db.rs 按域拆分而来，SQL 逐字不变。

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
/// 私有实现细节（cohesive interface 内部）：外部仅经 `get_items` 行为进入，绝不直调。
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

/// FTS 尝试 + LIKE 回退的内部实现：`allow_fts` 仅为 `get_items` 的内部开关，
/// 私有实现细节（cohesive interface 内部），SQL 语义逐字不变。
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

#[cfg(test)]
mod tests {
    use super::{cleanup_retention, get_items, insert_item, mark_all_read, upsert_remote_item, RemoteItemUpsert, UpsertEntry};
    use crate::models::GetItemsParams;
    use rusqlite::Connection;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().expect("mem db");
        crate::db::migrate_for_tests(&conn).expect("migrate");
        conn
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
    fn pagination_is_stable_through_get_items() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        for i in 1..=5 {
            insert_item(&conn, s.id, &item(&format!("g{i}"), "t", "x", i, false, false, false)).unwrap();
        }
        // 给定：按时间从新到旧排序（published_at 5..1）
        // 当：以每页 2 条经公共 get_items 分页获取时
        let page = |offset| {
            get_items(&conn, &GetItemsParams { limit: Some(2), offset: Some(offset), ..Default::default() }).unwrap()
        };
        let (p0, p1, p2) = (page(0), page(2), page(4));
        // 那么：各页平铺完整列表，无重叠且无遗漏
        let ids = |v: Vec<crate::models::Item>| v.into_iter().map(|i| i.guid).collect::<Vec<_>>();
        assert_eq!(ids(p0), vec!["g5".to_string(), "g4".to_string()]);
        assert_eq!(ids(p1), vec!["g3".to_string(), "g2".to_string()]);
        assert_eq!(ids(p2), vec!["g1".to_string()]);
    }

    #[test]
    fn like_fallback_matches_summary_and_author_through_get_items() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
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

        // 给定：FTS 路径不可用（缺失索引模拟 MATCH 真实拒绝）
        conn.execute("DROP TABLE items_fts", []).unwrap();
        // 当/那么：公共 get_items 经 warn 后回退到 LIKE，作者/摘要/正文/标题均命中
        for q in ["Austen", "unique-summary-xyz", "boring", "plain"] {
            let found = get_items(&conn, &GetItemsParams { search: Some(q.into()), ..Default::default() }).unwrap();
            assert_eq!(found.len(), 1, "fallback should match {q}");
        }
    }

    #[test]
    fn fts_search_and_hidden_filter_through_get_items() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");

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
        crate::db::set_item_content(&conn, found[0].id, "quantum computing", "quantum").unwrap();
        let found2 = get_items(&conn, &GetItemsParams { search: Some("quantum".into()), ..Default::default() }).unwrap();
        assert_eq!(found2.len(), 1);

        // 删除操作通过触发器同步清除 FTS 索引（从订阅源级联删除）
        crate::db::remove_source(&conn, s.id).unwrap();
        let found3 = get_items(&conn, &GetItemsParams { search: Some("quantum".into()), ..Default::default() }).unwrap();
        assert!(found3.is_empty());
    }

    #[test]
    fn fts_quoting_is_literal_through_get_items() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        insert_item(&conn, s.id, &item("g1", "hello world guide", "plain body", 100, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("g2", "other", "unrelated", 200, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("q2", "say \"hi\" to me", "other", 300, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("g3", "spaced out guide", "plain", 400, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("qb", "a\"b token", "plain", 500, false, false, false)).unwrap();

        // 分词以 AND 连接：多词经公共路径仅命中同时含两词者
        let anded = get_items(&conn, &GetItemsParams { search: Some("hello world".into()), ..Default::default() }).unwrap();
        assert_eq!(anded.len(), 1);
        assert_eq!(anded[0].guid, "g1");

        // 含引号的输入经字面命中（转义后不破坏 MATCH）
        let quoted = get_items(&conn, &GetItemsParams { search: Some("\"hi\"".into()), ..Default::default() }).unwrap();
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].guid, "q2");

        // 裸引号分词整体转义为字面短语：FTS5 unicode61 不索引分隔符，
        // 故按短语匹配为空（不报错、不注入）；含引号的字面命中由上组 `"hi"` 覆盖
        let bare = get_items(&conn, &GetItemsParams { search: Some("\"".into()), ..Default::default() }).unwrap();
        assert!(bare.is_empty(), "bare quote should be neutralized, not injected");

        // 内嵌引号分词整体转义：a"b 按字面命中
        let inner = get_items(&conn, &GetItemsParams { search: Some("a\"b".into()), ..Default::default() }).unwrap();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].guid, "qb");

        // 首尾与连续空白被归一：多余空格仍按 AND 命中
        let spaced = get_items(&conn, &GetItemsParams { search: Some("  spaced   out ".into()), ..Default::default() }).unwrap();
        assert_eq!(spaced.len(), 1);
        assert_eq!(spaced[0].guid, "g3");

        // 注入被中和为字面短语：OR 注入命中 0 且不报错
        let inj = get_items(&conn, &GetItemsParams { search: Some("hello\" OR \"1\"=\"1".into()), ..Default::default() }).unwrap();
        assert!(inj.is_empty(), "injection should be neutralized");

        // 空输入无表达式：返回常规列表而不报错
        let empty = get_items(&conn, &GetItemsParams { search: Some("".into()), ..Default::default() }).unwrap();
        assert_eq!(empty.len(), 5);
        // 纯空白输入不报错、不注入：LIKE 字面无命中
        let blank = get_items(&conn, &GetItemsParams { search: Some("   ".into()), ..Default::default() }).unwrap();
        assert!(blank.is_empty());
    }

    #[test]
    fn fts_rejection_falls_back_to_like_through_get_items() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        insert_item(&conn, s.id, &item("q1", "tokio deep dive", "plain body", 100, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("q2", "say \"hi\" to me", "other", 200, false, false, false)).unwrap();

        // 给定：FTS 引号转义使任何用户输入都无法破坏 MATCH（见上），
        // 以缺失索引模拟 MATCH 真实拒绝；公共路径经 warn 后回退到 LIKE，结果正确
        conn.execute("DROP TABLE items_fts", []).unwrap();
        let found = get_items(&conn, &GetItemsParams { search: Some("tokio".into()), ..Default::default() }).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].guid, "q1");
        // 含引号的输入同样经 LIKE 字面命中
        let quoted = get_items(&conn, &GetItemsParams { search: Some("\"hi\"".into()), ..Default::default() }).unwrap();
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].guid, "q2");
    }

    #[test]
    fn retention_cleanup_through_public_surface() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
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
    fn upsert_remote_lww_and_monotonic_hidden_through_public_surface() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");

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

        // 隐藏单调：置1后 quiet 重放（hidden=false）不回清
        let hide = upsert_remote_item(
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
                hidden: true,
            },
        )
        .unwrap();
        assert!(!hide);
        assert!(get_items(&conn, &GetItemsParams::default()).unwrap().is_empty());
        assert_eq!(
            get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap().len(),
            1
        );
        let replay = upsert_remote_item(
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
        assert!(!replay);
        // quiet 重放不回清：隐藏列表仍为 1
        assert_eq!(
            get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap().len(),
            1
        );

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
    }

    #[test]
    fn mark_all_read_through_get_items() {
        let conn = mem_db();
        let group = crate::db::create_group(&conn, "Test Group").expect("create group");
        let source1 = crate::db::insert_source(&conn, "https://example.com/1", "Source 1", None, Some(group.id)).expect("insert source 1");
        let source2 = crate::db::insert_source(&conn, "https://example.com/2", "Source 2", None, None).expect("insert source 2");

        insert_item(&conn, source1.id, &item("g1", "Title 1", "Content", 100, false, false, false)).unwrap();
        insert_item(&conn, source2.id, &item("g2", "Title 2", "Content", 200, false, false, false)).unwrap();

        // 按订阅源标记已读
        let count = mark_all_read(&conn, Some("source"), Some(source1.id)).expect("mark by source");
        assert_eq!(count, 1);

        // 按分组标记已读
        let count = mark_all_read(&conn, Some("group"), Some(group.id)).expect("mark by group");
        assert_eq!(count, 1);

        // 全部标记已读（全局）
        let count = mark_all_read(&conn, None, None).expect("mark all read");
        assert_eq!(count, 2);
        // 行为断言经公共 get_items：未读过滤为空
        let unread = get_items(&conn, &GetItemsParams { filter: Some(1), ..Default::default() }).unwrap();
        assert!(unread.is_empty());
    }
}
