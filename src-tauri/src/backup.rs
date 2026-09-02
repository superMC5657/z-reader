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

    let db_bytes = std::fs::read(db_file).map_err(|e| e.to_string())?;
    zw.start_file(DB_ENTRY, opts).map_err(|e| e.to_string())?;
    zw.write_all(&db_bytes).map_err(|e| e.to_string())?;

    if let Some(sp) = settings_file {
        if let Ok(text) = std::fs::read_to_string(sp) {
            zw.start_file(SETTINGS_ENTRY, opts).map_err(|e| e.to_string())?;
            zw.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
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

/// Extract an archive into a destination directory (path-traversal safe).
pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut za = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..za.len() {
        let mut entry = za.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
            continue;
        }
        let out_path = dest.join(&name);
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
        assert_eq!(
            std::fs::read_to_string(restored_dir.join(SETTINGS_ENTRY)).unwrap(),
            "{\"theme\":\"dark\"}"
        );
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
    fn test_validate_rejects_garbage() {
        let dir = temp_dir("bad");
        let bad = dir.join("bad.db");
        std::fs::write(&bad, b"not a database at all").unwrap();
        assert!(validate_db(&bad).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
