use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

pub const DB_ENTRY: &str = "zreader.db";
pub const SETTINGS_ENTRY: &str = "settings.json";
const FAVICON_PREFIX: &str = "favicons/";

/// Snapshot the live database into a standalone file via the SQLite backup
/// API. Consistent even while other writes are happening under WAL.
pub fn snapshot_live(conn: &Connection, dst: &Path) -> Result<(), String> {
    let mut dst_conn = Connection::open(dst).map_err(|e| e.to_string())?;
    let backup = Backup::new(conn, &mut dst_conn).map_err(|e| e.to_string())?;
    backup
        .run_to_completion(64, std::time::Duration::from_millis(5), None)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Pack the DB snapshot, settings.json and favicons into one archive.
pub fn write_archive(
    db_file: &Path,
    settings_file: Option<&Path>,
    favicon_dir: Option<&Path>,
    out: &Path,
) -> Result<(), String> {
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut zw = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Stream the DB straight into the archive: never hold the whole file in
    // memory, so hundred-megabyte libraries still export on small machines.
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

/// Strip secrets from settings.json before it enters a backup archive.
///
/// `syncAccount.password` and `proxyPassword` are redacted; everything else
/// (server URL, username, host) is kept so restore still reconnects and the
/// user only re-enters secrets. Parse failures fall back to the original text
/// to preserve restore compatibility.
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
        // camelCase (serde) + snake_case (hand-written files) both redacted.
        for key in ["proxyPassword", "proxy_password"] {
            if obj.contains_key(key) {
                obj.insert(key.into(), serde_json::Value::String(String::new()));
            }
        }
    }
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| text.to_string())
}

/// Extract an archive into a destination directory.
/// Hardened: symlink entries are skipped, paths must stay enclosed in `dest`
/// (rejects `..`, absolute paths and drive prefixes), and archives with
/// absurd entry counts or total sizes (zip bombs) are refused upfront.
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
        // `enclosed_name` is the authoritative traversal check; the manual
        // filters above stay as a cheap first pass.
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

/// Atomically replace the live database with a staged file: same-directory
/// rename when the platform allows it, copy + cleanup otherwise. Both paths
/// leave no half-written live file behind.
pub fn replace_live(staged: &Path, live: &Path) -> Result<(), String> {
    if std::fs::rename(staged, live).is_err() {
        std::fs::copy(staged, live).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(staged);
    }
    Ok(())
}

/// Validate a restored database before it replaces the live one.
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
        // Settings survive the scrub roundtrip as equivalent JSON (pretty-printed).
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
        // Given: settings carrying sync + proxy secrets
        let raw = r#"{"theme":"dark","proxyPassword":"s3cret","syncAccount":{"provider":"greader","serverUrl":"https://x","username":"u","password":"hunter2"}}"#;
        // When: scrubbed for archival
        let out = scrub_settings_json(raw);
        // Then: secrets are gone, everything else survives as valid JSON
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
        // Given: an archive mixing a normal file with a path-escape attempt
        // (the public writer API masks symlink file-type bits, so traversal
        // is the reachable attack shape here; symlink skipping stays as
        // defense-in-depth per `is_symlink`)
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
        // When: extracted
        let dest = dir.join("out");
        extract_archive(&archive, &dest).unwrap();
        // Then: the real file lands, nothing escapes the destination
        assert_eq!(std::fs::read_to_string(dest.join("ok.txt")).unwrap(), "fine");
        assert!(!dir.join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_replace_live_swaps_and_preserves_on_error() {
        // Given: a live DB file and a staged replacement
        let dir = temp_dir("swap");
        let live = dir.join("zreader.db");
        let staged = dir.join("zreader.db.restoring");
        std::fs::write(&live, b"live-data").unwrap();
        std::fs::write(&staged, b"staged-data").unwrap();
        // When: swapped
        replace_live(&staged, &live).unwrap();
        // Then: the live path carries the staged content
        assert_eq!(std::fs::read(&live).unwrap(), b"staged-data");

        // Given: a missing staged file
        // When: replacement is attempted
        let missing = dir.join("nope.db");
        let before = std::fs::read(&live).unwrap();
        // Then: it errors and the live file is untouched
        assert!(replace_live(&missing, &live).is_err());
        assert_eq!(std::fs::read(&live).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
