//! 云同步账户会话顺序 Module（无 AppHandle）。
//!
//! 收敛 `sync_login` / `sync_logout` 的持久化与内存会话顺序：
//! login 在 subscriptions 成功后才 save；logout 按
//! save(None)→clear_sync_secret→clear_session→queue_clear 全序执行。
//! 入参仅为 settings 路径 + Connection + 会话钩子，便于无 AppHandle 单测。

#![allow(dead_code)]

/// login 收尾：subscriptions 成功后才保存账户（调用方保证先网络后 save 的顺序）。
pub fn complete_login(
    settings_path: &std::path::Path,
    acct: crate::models::SyncAccount,
) -> Result<(), String> {
    let mut s = crate::settings::load(settings_path);
    s.sync_account = Some(acct);
    crate::settings::save(settings_path, &s)
}

/// logout 全序：save(None)→clear_sync_secret→clear_session→queue_clear。
pub fn apply_logout(
    settings_path: &std::path::Path,
    conn: &rusqlite::Connection,
    clear_session: impl FnOnce(),
) -> Result<(), String> {
    let mut s = crate::settings::load(settings_path);
    s.sync_account = None;
    crate::settings::save(settings_path, &s)?;
    // 尽力而为：退出登录后绝不能残留过期的账户密码。
    let _ = crate::settings::clear_sync_secret(settings_path);
    clear_session();
    crate::db::queue_clear(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zreader-syncauth-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    fn login_acct(password: &str) -> crate::models::SyncAccount {
        crate::models::SyncAccount {
            provider: "greader".into(),
            server_url: "https://sync.example/api".into(),
            username: "u".into(),
            password: password.into(),
        }
    }

    #[test]
    fn login_saves_account_after_subscriptions() {
        // 给定：空设置文件（subscriptions 已在调用方成功）
        let path = temp_settings("login");
        // 当：收尾保存账户时
        complete_login(&path, login_acct("hunter2")).unwrap();
        // 那么：账户落盘且凭据可经 load 回填（磁盘 JSON 本体脱敏）
        let loaded = crate::settings::load(&path);
        let acct = loaded.sync_account.expect("account saved");
        assert_eq!(acct.username, "u");
        assert_eq!(acct.password, "hunter2");
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("hunter2"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn logout_order_clears_account_secret_session_queue() {
        // 给定：临时设置（含账户凭据）+ 内存库预置队列
        let path = temp_settings("logout");
        crate::settings::save(&path, &crate::models::Settings {
            sync_account: Some(login_acct("hunter2")),
            ..crate::models::Settings::default()
        })
        .unwrap();
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let s = crate::db::insert_source(&conn, "https://e.example", "E", None, None).unwrap();
        let inserted = crate::db::upsert_remote_item(
            &conn,
            &crate::db::RemoteItemUpsert {
                remote_id: "00000000175c6d3b",
                source_id: s.id,
                title: "t",
                url: Some("https://e.example/a"),
                author: None,
                published_at: 100,
                content: Some("x"),
                summary: None,
                snippet: Some("x"),
                has_been_read: false,
                starred: false,
                hidden: false,
            },
        )
        .unwrap();
        assert!(inserted);
        let item_id: i64 = conn
            .query_row("SELECT id FROM items WHERE remote_id='00000000175c6d3b'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            crate::db::enqueue_item_actions(&conn, &[item_id], crate::models::SyncAction::MarkRead).unwrap(),
            1
        );
        assert_eq!(crate::db::queue_len(&conn).unwrap(), 1);
        // 当：按全序退出时
        let fired = std::cell::Cell::new(false);
        apply_logout(&path, &conn, || fired.set(true)).unwrap();
        // 那么：账户 None、secret 清、会话清、队列 0
        assert!(crate::settings::load(&path).sync_account.is_none());
        let secrets_raw = std::fs::read_to_string(path.with_file_name("secrets.json")).unwrap();
        assert!(!secrets_raw.contains("hunter2"));
        assert!(fired.get());
        assert_eq!(crate::db::queue_len(&conn).unwrap(), 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
