//! 分组与订阅源目录（含远端对齐 lookup；零行为变更：由 db.rs 按域拆分而来，SQL 逐字不变）。

use crate::models::{Group, Source};
use rusqlite::{params, Connection, OptionalExtension, Row};

fn row_to_group(row: &Row) -> rusqlite::Result<Group> {
    Ok(Group {
        id: row.get(0)?,
        name: row.get(1)?,
        expanded: row.get::<_, i64>(2)? != 0,
        sort: row.get(3)?,
        remote_id: row.get(4)?,
    })
}

pub fn get_groups(conn: &Connection) -> Result<Vec<Group>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, expanded, sort, remote_id FROM groups ORDER BY sort, id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_group)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn create_group(conn: &Connection, name: &str) -> Result<Group, String> {
    conn.execute("INSERT INTO groups (name) VALUES (?1)", params![name])
        .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid();
    Ok(Group { id, name: name.into(), expanded: true, sort: 0, remote_id: None })
}

pub fn rename_group(conn: &Connection, id: i64, name: &str) -> Result<(), String> {
    conn.execute("UPDATE groups SET name=?1 WHERE id=?2", params![name, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_group(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM groups WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_group_expanded(conn: &Connection, id: i64, expanded: bool) -> Result<(), String> {
    conn.execute(
        "UPDATE groups SET expanded=?1 WHERE id=?2",
        params![expanded as i64, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn row_to_source(row: &Row) -> rusqlite::Result<Source> {
    Ok(Source {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        favicon: row.get(4)?,
        group_id: row.get(5)?,
        last_fetched: row.get(6)?,
        error_count: row.get(7)?,
        unread: row.get(8)?,
        remote_id: row.get(9)?,
        last_error: row.get(10)?,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.url, s.title, s.description, s.favicon, s.group_id, s.last_fetched, s.error_count,
    (SELECT COUNT(*) FROM items i WHERE i.source_id = s.id AND i.has_been_read = 0) AS unread, s.remote_id, s.last_error
    FROM sources s";

pub fn get_sources(conn: &Connection) -> Result<Vec<Source>, String> {
    let mut stmt = conn
        .prepare(&format!("{SOURCE_SELECT} ORDER BY s.id"))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_source)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn get_source(conn: &Connection, id: i64) -> Result<Source, String> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.id = ?1"),
        params![id],
        row_to_source,
    )
    .map_err(|e| e.to_string())
}

pub fn get_source_by_url(conn: &Connection, url: &str) -> Result<Option<Source>, String> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.url = ?1"),
        params![url],
        row_to_source,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn insert_source(
    conn: &Connection,
    url: &str,
    title: &str,
    description: Option<&str>,
    group_id: Option<i64>,
) -> Result<Source, String> {
    conn.execute(
        "INSERT INTO sources (url, title, description, group_id) VALUES (?1, ?2, ?3, ?4)",
        params![url, title, description, group_id],
    )
    .map_err(|e| e.to_string())?;
    get_source(conn, conn.last_insert_rowid())
}

pub fn remove_source(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM sources WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_source_group(conn: &Connection, source_id: i64, group_id: Option<i64>) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET group_id=?1 WHERE id=?2",
        params![group_id, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn rename_source(conn: &Connection, source_id: i64, title: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET title=?1 WHERE id=?2",
        params![title, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_source_favicon(conn: &Connection, source_id: i64, path: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET favicon=?1 WHERE id=?2",
        params![path, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn mark_source_fetched(
    conn: &Connection,
    source_id: i64,
    ok: bool,
    err: Option<&str>,
) -> Result<(), String> {
    if ok {
        conn.execute(
            "UPDATE sources SET last_fetched=?1, error_count=0, last_error=NULL WHERE id=?2",
            params![crate::models::now_ts(), source_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        let msg: String = err.unwrap_or("fetch failed").chars().take(500).collect();
        conn.execute(
            "UPDATE sources SET error_count = error_count + 1, last_error=?1 WHERE id=?2",
            params![msg, source_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn get_group(conn: &Connection, id: i64) -> Result<Option<Group>, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort, remote_id FROM groups WHERE id = ?1",
        params![id],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn get_group_by_remote_id(conn: &Connection, remote_id: &str) -> Result<Option<Group>, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort, remote_id FROM groups WHERE remote_id = ?1",
        params![remote_id],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_group_remote(conn: &Connection, group_id: i64, remote_id: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE groups SET remote_id=?1 WHERE id=?2",
        params![remote_id, group_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn find_or_create_group(conn: &Connection, name: &str) -> Result<Group, String> {
    conn.query_row(
        "SELECT id, name, expanded, sort, remote_id FROM groups WHERE name = ?1",
        params![name],
        row_to_group,
    )
    .optional()
    .map_err(|e| e.to_string())?
    .map(Ok)
    .unwrap_or_else(|| create_group(conn, name))
}

pub fn find_or_create_group_by_remote(
    conn: &Connection,
    remote_id: &str,
    name: &str,
) -> Result<Group, String> {
    if let Some(mut g) = get_group_by_remote_id(conn, remote_id)? {
        if g.name != name {
            rename_group(conn, g.id, name)?;
            g.name = name.to_string();
        }
        return Ok(g);
    }
    let existing = conn
        .query_row(
            "SELECT id, name, expanded, sort, remote_id FROM groups WHERE name = ?1",
            params![name],
            row_to_group,
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(mut g) = existing {
        set_group_remote(conn, g.id, Some(remote_id))?;
        g.remote_id = Some(remote_id.to_string());
        Ok(g)
    } else {
        conn.execute(
            "INSERT INTO groups (name, remote_id) VALUES (?1, ?2)",
            params![name, remote_id],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid();
        Ok(Group {
            id,
            name: name.to_string(),
            expanded: true,
            sort: 0,
            remote_id: Some(remote_id.to_string()),
        })
    }
}

pub fn get_source_by_remote_id(conn: &Connection, remote_id: &str) -> Result<Option<Source>, String> {
    conn.query_row(
        &format!("{SOURCE_SELECT} WHERE s.remote_id = ?1"),
        params![remote_id],
        row_to_source,
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_source_remote(conn: &Connection, source_id: i64, remote_id: Option<&str>) -> Result<(), String> {
    conn.execute(
        "UPDATE sources SET remote_id=?1 WHERE id=?2",
        params![remote_id, source_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
