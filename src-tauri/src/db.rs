//! 数据库访问层（按域拆分的外壳：零行为变更，SQL 逐字不变）。
//!
//! - `schema`：open/migrate/版本与 rusqlite 类型转换；
//! - `catalog`：分组与订阅源目录（含远端对齐 lookup）；
//! - `items`：文章条目、FTS5 查询与 LIKE 回退、远端 upsert 对齐、统计与保留策略；
//! - `rules`：正则规则 CRUD；
//! - `sync`：同步队列与状态游标。
//!
//! 调用方 `crate::db::X` / `db::X` 路径经由重导出保持可用。

pub mod catalog;
pub mod items;
pub mod rules;
pub mod schema;
pub mod sync;

// 重导出兼容：调用方 `crate::db::X` / `db::X` 路径保持可用，SQL 语义逐字不变。
// glob 形式避免私有模块内未使用重导出的告警；域名下无重名符号。
pub use catalog::*;
pub use items::*;
pub use rules::*;
pub use schema::{open, CURRENT_VERSION};
#[cfg(test)]
pub use schema::migrate_for_tests;
pub use sync::*;
#[cfg(test)]
pub(crate) use items::{fts_match_query, get_items_impl};
#[cfg(test)]
pub(crate) use schema::migrate;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{GetItemsParams, Item, SyncAction};
    use rusqlite::{params, Connection};

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

    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zreader-dbmig-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("seed.db")
    }

    #[test]
    fn migration_chain_v1_seed_upgrades_to_current() {
        // 给定：v1 时代的手工 schema（含一行分组/源/文章），user_version=1
        let path = temp_db_path("v1");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE groups (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, expanded INTEGER NOT NULL DEFAULT 1, sort INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE sources (id INTEGER PRIMARY KEY AUTOINCREMENT, url TEXT NOT NULL UNIQUE, title TEXT NOT NULL DEFAULT '', description TEXT, favicon TEXT, group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL, last_fetched INTEGER, error_count INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE, guid TEXT NOT NULL, title TEXT NOT NULL DEFAULT '', url TEXT, author TEXT, published_at INTEGER NOT NULL, content TEXT, summary TEXT, snippet TEXT, image TEXT, has_been_read INTEGER NOT NULL DEFAULT 0, starred INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL, UNIQUE(source_id, guid));
                INSERT INTO groups (name) VALUES ('Legacy');
                INSERT INTO sources (url, title) VALUES ('https://legacy.example/feed', 'Legacy Feed');
                INSERT INTO items (source_id, guid, title, published_at, content, snippet, has_been_read, starred, created_at) VALUES (1, 'legacy-g1', 'legacy article', 100, 'legacy body', 'legacy', 0, 0, 100);
                PRAGMA user_version = 1;
                "#,
            )
            .unwrap();
        }
        // 当：经 open 逐版迁移时
        let conn = open(&path).unwrap();
        // 那么：版本升到 CURRENT，旧数据可用，各版产物就绪
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(v, CURRENT_VERSION);
        let sources = get_sources(&conn).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "Legacy Feed");
        let items = get_items(&conn, &GetItemsParams::default()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].guid, "legacy-g1");
        // v2 产物：hidden 列可用
        set_item_hidden(&conn, items[0].id, true).unwrap();
        let hidden = get_items(&conn, &GetItemsParams { filter: Some(3), ..Default::default() }).unwrap();
        assert_eq!(hidden.len(), 1);
        // v3 产物：remote_id 列 + 同步队列/状态表可用
        set_source_remote(&conn, sources[0].id, Some("feed/9")).unwrap();
        assert_eq!(
            get_source_by_remote_id(&conn, "feed/9").unwrap().map(|s| s.id),
            Some(sources[0].id)
        );
        enqueue_stream_action(&conn, SyncAction::MarkAllRead, "x").unwrap();
        assert_eq!(queue_len(&conn).unwrap(), 1);
        // v4 产物：groups.remote_id 列可用
        set_group_remote(&conn, 1, Some("user/-/label/L")).unwrap();
        // v5 产物：sources.last_error 列可用
        mark_source_fetched(&conn, sources[0].id, false, Some("boom")).unwrap();
        assert_eq!(
            get_source(&conn, sources[0].id).unwrap().last_error.as_deref(),
            Some("boom")
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn fts_match_query_quotes_and_injection() {
        // 分词以 AND 连接
        assert_eq!(
            fts_match_query("hello world").as_deref(),
            Some("\"hello\" AND \"world\"")
        );
        // 双引号经加倍转义，注入被中和为字面短语
        assert_eq!(fts_match_query("a\"b").as_deref(), Some("\"a\"\"b\""));
        // 空/纯空白输入无表达式
        assert_eq!(fts_match_query(""), None);
        assert_eq!(fts_match_query("   "), None);
        // 首尾与连续空白被归一
        assert_eq!(
            fts_match_query("  spaced   out ").as_deref(),
            Some("\"spaced\" AND \"out\"")
        );
        // 裸引号分词整体转义（外层包裹 + 内层加倍）
        assert_eq!(fts_match_query("\"").as_deref(), Some("\"\"\"\""));
    }

    #[test]
    fn fts_match_rejection_falls_back_to_like() {
        let conn = Connection::open_in_memory().expect("init db");
        migrate(&conn).expect("migrate");
        let s = insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        insert_item(&conn, s.id, &item("q1", "tokio deep dive", "plain body", 100, false, false, false)).unwrap();
        insert_item(&conn, s.id, &item("q2", "say \"hi\" to me", "other", 200, false, false, false)).unwrap();

        // 给定：FTS 引号转义使任何用户输入都无法破坏 MATCH（见 probe 实测），
        // 以缺失索引模拟 MATCH 真实拒绝
        conn.execute("DROP TABLE items_fts", []).unwrap();
        // 当/那么：直接断言拒绝真实发生
        assert!(get_items_impl(
            &conn,
            &GetItemsParams { search: Some("tokio".into()), ..Default::default() },
            true,
        )
        .is_err());

        // 公共路径经 warn 后回退到 LIKE，结果正确
        let found = get_items(&conn, &GetItemsParams { search: Some("tokio".into()), ..Default::default() }).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].guid, "q1");
        // 含引号的输入同样经 LIKE 字面命中
        let quoted = get_items(&conn, &GetItemsParams { search: Some("\"hi\"".into()), ..Default::default() }).unwrap();
        assert_eq!(quoted.len(), 1);
        assert_eq!(quoted[0].guid, "q2");
    }
}
