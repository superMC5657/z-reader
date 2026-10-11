//! 存储与数据保留服务。
//!
//! 包含历史文章清理与保留策略（retention policy）以及数据库压缩整理（vacuum）逻辑。

#![allow(dead_code)]

/// 执行数据保留策略清理并根据阈值决定是否执行数据库压缩（VACUUM）。
///
/// 对齐 `lib.rs::refresh_all_sources`、`commands.rs::sync_now` 及
/// `commands.rs::cleanup_now` 三处的清理与压缩逻辑：
/// 1. 按保留天数 `days`（`0` 表示跳过时间窗口清理）结合单源条目上限 `max_items`
///   （`0` 表示不设上限），清理未加星标的已读/隐藏文章；
/// 2. 若删除的文章总数**严格大于** `vacuum_threshold`，则触发 SQLite `VACUUM`
///    整理数据库空间（后台刷新/同步传 `200`，手动立即清理传 `0`，阈值语义不变）；
/// 3. 返回本次清理删除的文章总数；清理失败时记录 warn 并返回 `0`。
/// Retention seam Adapter: refresh cycle enters via `services::source::apply_refresh_retention` (threshold 200);
/// manual cleanup enters directly with threshold 0. Threshold semantics unchanged.
/// 存储入口经窄壳 flat `crate::db::{cleanup_retention, vacuum}` 进入条目 cohesive interface
///（`db::items` 拥有查询/upsert/保留/整理全部深度，SQL 逐字不变；ADR-0002 唯一源加强）。
pub fn run_retention_policy(
    conn: &rusqlite::Connection,
    days: u32,
    max_items: u32,
    vacuum_threshold: usize,
) -> usize {
    let deleted = match crate::db::cleanup_retention(conn, days, max_items) {
        Ok(n) => n,
        Err(e) => {
            log::warn!("retention failed reason {}", crate::utils::short_reason(&e));
            return 0;
        }
    };
    if deleted > vacuum_threshold {
        if let Err(e) = crate::db::vacuum(conn) {
            log::warn!("vacuum failed reason {}", crate::utils::short_reason(&e));
        }
    }
    if deleted > 0 {
        log::info!("retention done deleted={deleted}");
    } else {
        log::debug!("retention done deleted=0");
    }
    deleted
}

/// 执行数据保留策略清理（`run_retention_policy` 的同语义别名，接收数值型天数）。
pub fn run_retention_policy_days(
    conn: &rusqlite::Connection,
    days: u32,
    max_items: u32,
    vacuum_threshold: usize,
) -> usize {
    run_retention_policy(conn, days, max_items, vacuum_threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("mem db");
        crate::db::migrate_for_tests(&conn).expect("migrate");
        conn
    }

    fn seed_item(conn: &rusqlite::Connection, source_id: i64, guid: &str, published_at: i64, read: bool, starred: bool, hidden: bool) {
        crate::db::insert_item(
            conn,
            source_id,
            &crate::db::UpsertEntry {
                guid,
                title: guid,
                url: None,
                author: None,
                published_at,
                content: Some("x"),
                summary: None,
                snippet: Some("x"),
                image: None,
                has_been_read: read,
                starred,
                hidden,
            },
        )
        .expect("insert");
    }

    #[test]
    fn deletes_expired_and_caps_per_source() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        let now = crate::models::now_ts();
        let day: i64 = 86_400;
        seed_item(&conn, s.id, "old-read", now - 100 * day, true, false, false);
        seed_item(&conn, s.id, "old-star", now - 100 * day, false, true, false);
        seed_item(&conn, s.id, "recent-read", now - 10 * day, true, false, false);

        // 30 天窗口：仅删除超期的未加星标已读 1 篇；阈值取极大值避免 VACUUM。
        let deleted = run_retention_policy_days(&conn, 30, 0, usize::MAX);
        assert_eq!(deleted, 1);
        assert_eq!(crate::db::item_count(&conn).unwrap(), 2);
    }

    #[test]
    fn zero_days_skips_time_expiry_and_zero_threshold_vacuums() {
        let conn = mem_db();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).expect("source");
        let now = crate::models::now_ts();
        seed_item(&conn, s.id, "a", now, false, false, false);
        seed_item(&conn, s.id, "b", now, false, false, false);
        seed_item(&conn, s.id, "c", now, false, false, false);
        seed_item(&conn, s.id, "d", now, false, false, false);

        // 天数 0 表示不按时间清理，仅按单源上限 3 清理 1 篇。
        // 阈值 0：deleted(1) > 0 触发 VACUUM（内存库上为无操作但不应失败）。
        let deleted = run_retention_policy(&conn, 0, 3, 0);
        assert_eq!(deleted, 1);

        // 阈值极大：同样删除但跳过 VACUUM，行为一致（经由别名入口覆盖）。
        let deleted2 = run_retention_policy_days(&conn, 0, 2, usize::MAX);
        assert_eq!(deleted2, 1);
    }
}
