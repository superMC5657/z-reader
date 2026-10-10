//! 正则规则 CRUD（零行为变更：由 db.rs 按域拆分而来，SQL 逐字不变）。

use rusqlite::{params, Connection, Row};

fn row_to_rule(row: &Row) -> rusqlite::Result<crate::models::Rule> {
    Ok(crate::models::Rule {
        id: row.get(0)?,
        name: row.get(1)?,
        pattern: row.get(2)?,
        target_field: row.get(3)?,
        action_type: row.get(4)?,
        is_case_sensitive: row.get::<_, i64>(5)? != 0,
        is_enabled: row.get::<_, i64>(6)? != 0,
        source_scope: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub fn get_rules(conn: &Connection) -> Result<Vec<crate::models::Rule>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, pattern, target_field, action_type, is_case_sensitive, is_enabled, source_scope, created_at
             FROM regex_rules ORDER BY id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], row_to_rule)
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

pub fn create_rule(conn: &Connection, r: &crate::models::RuleInput) -> Result<crate::models::Rule, String> {
    conn.execute(
        "INSERT INTO regex_rules (name, pattern, target_field, action_type, is_case_sensitive, is_enabled, source_scope, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            r.name,
            r.pattern,
            r.target_field,
            r.action_type,
            r.is_case_sensitive as i64,
            r.is_enabled as i64,
            r.source_scope,
            crate::models::now_ts()
        ],
    )
    .map_err(|e| e.to_string())?;
    get_rule(conn, conn.last_insert_rowid())
}

pub fn get_rule(conn: &Connection, id: i64) -> Result<crate::models::Rule, String> {
    conn.query_row(
        "SELECT id, name, pattern, target_field, action_type, is_case_sensitive, is_enabled, source_scope, created_at
         FROM regex_rules WHERE id = ?1",
        params![id],
        row_to_rule,
    )
    .map_err(|e| e.to_string())
}

pub fn update_rule(conn: &Connection, id: i64, r: &crate::models::RuleInput) -> Result<(), String> {
    conn.execute(
        "UPDATE regex_rules SET name=?1, pattern=?2, target_field=?3, action_type=?4, is_case_sensitive=?5, is_enabled=?6, source_scope=?7 WHERE id=?8",
        params![
            r.name,
            r.pattern,
            r.target_field,
            r.action_type,
            r.is_case_sensitive as i64,
            r.is_enabled as i64,
            r.source_scope,
            id
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM regex_rules WHERE id=?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
