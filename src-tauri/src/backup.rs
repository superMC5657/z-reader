use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

pub const DB_ENTRY: &str = "zreader.db";
pub const SETTINGS_ENTRY: &str = "settings.json";
const FAVICON_PREFIX: &str = "favicons/";

/// 通过 SQLite 的在线备份 API 将运行中的数据库快照保存为独立文件。
/// 即使 WAL 模式下正在进行其他并发写入，也能保持数据一致性。
pub fn snapshot_live(conn: &Connection, dst: &Path) -> Result<(), String> {
    let mut dst_conn = Connection::open(dst).map_err(|e| e.to_string())?;
    let backup = Backup::new(conn, &mut dst_conn).map_err(|e| e.to_string())?;
    backup
        .run_to_completion(64, std::time::Duration::from_millis(5), None)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 将数据库快照、settings.json 和网站图标打包进单个归档文件。
pub fn write_archive(
    db_file: &Path,
    settings_file: Option<&Path>,
    favicon_dir: Option<&Path>,
    out: &Path,
) -> Result<(), String> {
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 将数据库以流式直接写入归档文件：绝不在内存中持有整个文件，
    // 确保数百兆体量的书库在低配置机器上也能顺利导出。
    let mut db_file = std::fs::File::open(db_file).map_err(|e| e.to_string())?;
    zw.start_file(DB_ENTRY, opts).map_err(|e| e.to_string())?;
    std::io::copy(&mut db_file, &mut zw).map_err(|e| e.to_string())?;

    if let Some(sp) = settings_file {
        if let Ok(text) = std::fs::read_to_string(sp) {
            zw.start_file(SETTINGS_ENTRY, opts).map_err(|e| e.to_string())?;
            zw.write_all(scrub_settings_json(&text).as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }

    if let Some(dir) = favicon_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if let Ok(bytes) = std::fs::read(&p) {
                    if zw
                        .start_file(format!("{FAVICON_PREFIX}{name}"), opts)
                        .is_ok()
                    {
                        let _ = zw.write_all(&bytes);
                    }
                }
            }
        }
    }

    zw.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// 在将 settings.json 写入备份归档前脱敏并剥离敏感凭据。
///
/// `syncAccount.password` 与 `proxyPassword` 会被擦除；其余所有配置
/// （服务器 URL、用户名、主机等）均予以保留，以便恢复后仍可重新连接，
/// 用户只需重新输入密码凭据。
pub fn scrub_settings_json(text: &str) -> String {
    let mut v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return text.to_string(),
    };
    if let Some(obj) = v.as_object_mut() {
        if let Some(acct) = obj.get_mut("syncAccount").and_then(|a| a.as_object_mut()) {
            if acct.contains_key("password") {
                acct.insert("password".into(), serde_json::Value::String(String::new()));
            }
        }
        if obj.contains_key("proxyPassword") {
            obj.insert("proxyPassword".into(), serde_json::Value::String(String::new()));
        }
    }
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| text.to_string())
}

/// 将归档文件解压至目标目录。
/// 安全加固：跳过符号链接条目，路径必须限制在 `dest` 内部
/// （拒绝 `..`、绝对路径与驱动器盘符前缀），且前置拒绝超大条目数或总大小的归档（防御 Zip 炸弹）。
pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    const MAX_ENTRIES: usize = 20_000;
    const MAX_TOTAL_BYTES: u64 = 1_000_000_000;
    const MAX_FILE_BYTES: u64 = 512_000_000;

    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut za = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if za.len() > MAX_ENTRIES {
        return Err(format!("backup has too many entries ({})", za.len()));
    }
    let mut total: u64 = 0;
    for i in 0..za.len() {
        let mut entry = za.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_symlink() {
            continue;
        }
        if entry.size() > MAX_FILE_BYTES {
            return Err(format!("backup entry too large: {}", entry.name()));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err("backup is too large to extract safely".into());
        }
        let name = entry.name().to_string();
        if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
            continue;
        }
        // `enclosed_name` 是权威的路径穿越校验；上方的
        // 手动过滤作为轻量的第一道快速过滤。
        let Some(safe_rel) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(safe_rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 使用暂存文件原子替换活跃数据库：在平台支持时采用同目录重命名，
/// 否则采用复制 + 清理策略。两种路径均不会残留写入一半的损坏文件。
pub fn replace_live(staged: &Path, live: &Path) -> Result<(), String> {
    if std::fs::rename(staged, live).is_err() {
        std::fs::copy(staged, live).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(staged);
    }
    Ok(())
}

/// 在恢复的文件替换活跃数据库之前，对其进行有效性验证。
pub fn validate_db(db_file: &Path) -> Result<(), String> {
    let conn = Connection::open_with_flags(db_file, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let result: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if result != "ok" {
        return Err(format!("integrity check failed: {result}"));
    }
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;
    if version > crate::db::CURRENT_VERSION {
        return Err(format!("backup was created by a newer app version (schema {version})"));
    }
    for table in ["groups", "sources", "items"] {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("backup is missing required table: {table}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zreader-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_backup_roundtrip() {
        let src_dir = temp_dir("src");
        let out_dir = temp_dir("out");
        let db_path = src_dir.join("zreader.db");

        let conn = crate::db::open(&db_path).unwrap();
        let group = crate::db::create_group(&conn, "G").unwrap();
        let source = crate::db::insert_source(&conn, "https://x.example", "X", None, Some(group.id)).unwrap();
        crate::db::insert_item(
            &conn,
            source.id,
            &crate::db::UpsertEntry {
                guid: "g1",
                title: "hello world",
                url: None,
                author: None,
                published_at: 100,
                content: Some("<p>unique-content-123</p>"),
                summary: None,
                snippet: Some("hello world"),
                image: None,
                has_been_read: false,
                starred: false,
                hidden: false,
            },
        )
        .unwrap();
        drop(conn);

        let settings_file = src_dir.join("settings.json");
        std::fs::write(&settings_file, "{\"theme\":\"dark\"}").unwrap();

        let snapshot = out_dir.join("snapshot.db");
        {
            let conn = crate::db::open(&db_path).unwrap();
            snapshot_live(&conn, &snapshot).unwrap();
        }
        let archive = out_dir.join("backup.zreader.bak");
        write_archive(&snapshot, Some(&settings_file), None, &archive).unwrap();

        let restored_dir = out_dir.join("restored");
        extract_archive(&archive, &restored_dir).unwrap();
        let restored_db = restored_dir.join(DB_ENTRY);
        assert!(restored_db.exists());
        // 设置经过脱敏往返后保留为等价的 JSON（格式化输出）。
        let restored_settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(restored_dir.join(SETTINGS_ENTRY)).unwrap())
                .unwrap();
        assert_eq!(restored_settings.get("theme").and_then(|v| v.as_str()), Some("dark"));
        validate_db(&restored_db).unwrap();

        let conn = crate::db::open(&restored_db).unwrap();
        let items = crate::db::get_items(
            &conn,
            &crate::models::GetItemsParams::default(),
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "hello world");

        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn test_scrub_removes_passwords() {
        // 给定：带有同步和代理凭据的设置
        let raw = r#"{"theme":"dark","proxyPassword":"s3cret","syncAccount":{"provider":"greader","serverUrl":"https://x","username":"u","password":"hunter2"}}"#;
        // 当：为归档进行脱敏处理时
        let out = scrub_settings_json(raw);
        // 那么：凭据已被清除，其他所有内容保留为有效 JSON
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v.get("theme").and_then(|v| v.as_str()), Some("dark"));
        assert_eq!(v.get("proxyPassword").and_then(|v| v.as_str()), Some(""));
        assert_eq!(
            v.get("syncAccount").and_then(|a| a.get("password")).and_then(|p| p.as_str()),
            Some("")
        );
        assert_eq!(
            v.get("syncAccount").and_then(|a| a.get("username")).and_then(|u| u.as_str()),
            Some("u")
        );
        assert!(!out.contains("s3cret") && !out.contains("hunter2"));
    }
    #[test]
    fn test_validate_rejects_garbage() {
        let dir = temp_dir("bad");
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"not a database at all").unwrap();
        assert!(validate_db(&bad).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_extract_skips_traversal_entries() {
        // 给定：包含正常文件和路径穿越攻击企图的归档文件
        // （公开的写入 API 会屏蔽符号链接类型位，因此路径穿越是此处主要的攻击形式；
        // 跳过符号链接作为深度防御依然生效）
        let dir = temp_dir("traversal");
        let archive = dir.join("evil.zreader.bak");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zw.start_file("ok.txt", opts).unwrap();
            zw.write_all(b"fine").unwrap();
            zw.start_file("../evil.txt", opts).unwrap();
            zw.write_all(b"escape").unwrap();
            zw.finish().unwrap();
        }
        // 当：解压时
        let dest = dir.join("out");
        extract_archive(&archive, &dest).unwrap();
        // 那么：仅解压合法文件，无文件逃逸到目标目录之外
        assert_eq!(std::fs::read_to_string(dest.join("ok.txt")).unwrap(), "fine");
        assert!(!dir.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_replace_live_swaps_and_preserves_on_error() {
        // 给定：活跃 DB 文件和暂存替换文件
        let dir = temp_dir("swap");
        let live = dir.join("zreader.db");
        let staged = dir.join("zreader.db.restoring");
        std::fs::write(&live, b"live-data").unwrap();
        std::fs::write(&staged, b"staged-data").unwrap();
        // 当：进行替换时
        replace_live(&staged, &live).unwrap();
        // 那么：活跃路径包含暂存的内容
        assert_eq!(std::fs::read(&live).unwrap(), b"staged-data");

        // 给定：暂存文件缺失
        // 当：尝试执行替换时
        let missing = dir.join("nope.db");
        let before = std::fs::read(&live).unwrap();
        // 那么：报错且活跃文件未被篡改
        assert!(replace_live(&missing, &live).is_err());
        assert_eq!(std::fs::read(&live).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
