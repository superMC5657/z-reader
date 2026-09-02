use crate::db;
use crate::models::{html_to_text, Rule};
use regex::Regex;
use rusqlite::{params, Connection};

/// Compiled, enabled rules ready for evaluation.
pub struct RuleEngine {
    rules: Vec<(Rule, Regex)>,
}

/// Actions to apply to a single article after rule evaluation.
#[derive(Default, Clone, Copy, Debug)]
pub struct Outcome {
    pub mark_read: bool,
    pub star: bool,
    pub hide: bool,
    pub notify: bool,
}

#[derive(Default, Debug)]
pub struct BackfillStats {
    pub marked_read: usize,
    pub starred: usize,
    pub hidden: usize,
    pub notified: usize,
}

pub fn valid_action(action: &str) -> bool {
    matches!(action, "mark_read" | "star" | "hide" | "notify")
}

pub fn valid_target(target: &str) -> bool {
    matches!(target, "title" | "content" | "author" | "source_url" | "any")
}

/// Validate the scope format: "all", "source:{id}" or "group:{id}".
pub fn valid_scope(scope: &str) -> bool {
    scope == "all"
        || scope
            .strip_prefix("source:")
            .or_else(|| scope.strip_prefix("group:"))
            .map(|id| id.parse::<i64>().is_ok())
            .unwrap_or(false)
}

/// Compile the pattern with an inline `(?i)` flag when case-insensitive.
pub fn compile_pattern(rule: &Rule) -> Result<Regex, String> {
    let mut pattern = String::new();
    if !rule.is_case_sensitive {
        pattern.push_str("(?i)");
    }
    pattern.push_str(&rule.pattern);
    Regex::new(&pattern).map_err(|e| format!("invalid regex: {e}"))
}

impl RuleEngine {
    /// Load and compile all enabled rules. Scope filtering happens per
    /// article at evaluation time, so one engine serves both the fetch
    /// pipeline and full-database backfill.
    pub fn load(conn: &Connection) -> Result<Self, String> {
        let all = db::get_rules(conn)?;
        let mut rules = Vec::new();
        for r in all {
            if !r.is_enabled {
                continue;
            }
            let re = compile_pattern(&r)?;
            rules.push((r, re));
        }
        Ok(Self { rules })
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    fn scope_applies(rule: &Rule, source_id: i64, group_id: Option<i64>) -> bool {
        if rule.source_scope == "all" {
            return true;
        }
        if let Some(id) = rule.source_scope.strip_prefix("source:") {
            return id.parse::<i64>() == Ok(source_id);
        }
        if let Some(id) = rule.source_scope.strip_prefix("group:") {
            return id.parse::<i64>() == Ok(group_id.unwrap_or(-1));
        }
        false
    }

    /// Evaluate all applicable rules against one article.
    pub fn evaluate(
        &self,
        source_id: i64,
        group_id: Option<i64>,
        source_url: &str,
        title: &str,
        content_text: &str,
        author: Option<&str>,
        url: Option<&str>,
    ) -> Outcome {
        let mut out = Outcome::default();
        for (rule, re) in &self.rules {
            if !Self::scope_applies(rule, source_id, group_id) {
                continue;
            }
            let haystack = match rule.target_field.as_str() {
                "title" => title.to_string(),
                "content" => content_text.to_string(),
                "author" => author.unwrap_or("").to_string(),
                "source_url" => format!("{} {}", url.unwrap_or(""), source_url),
                _ => format!(
                    "{title} {} {} {} {source_url}",
                    content_text,
                    author.unwrap_or(""),
                    url.unwrap_or("")
                ),
            };
            if re.is_match(&haystack) {
                match rule.action_type.as_str() {
                    "mark_read" => out.mark_read = true,
                    "star" => out.star = true,
                    "hide" => out.hide = true,
                    "notify" => out.notify = true,
                    _ => {}
                }
            }
        }
        out
    }
}

/// Apply all rules to existing articles in a single pass.
pub fn backfill(conn: &Connection, engine: &RuleEngine) -> Result<BackfillStats, String> {
    struct Row {
        id: i64,
        source_id: i64,
        group_id: Option<i64>,
        source_url: String,
        title: String,
        content: Option<String>,
        summary: Option<String>,
        author: Option<String>,
        url: Option<String>,
    }
    let rows: Vec<Row> = {
        let mut stmt = conn
            .prepare(
                "SELECT i.id, i.source_id, s.group_id, s.url, i.title, i.content, i.summary, i.author, i.url
                 FROM items i JOIN sources s ON s.id = i.source_id",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |row| {
                Ok(Row {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    group_id: row.get(2)?,
                    source_url: row.get(3)?,
                    title: row.get(4)?,
                    content: row.get(5)?,
                    summary: row.get(6)?,
                    author: row.get(7)?,
                    url: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        mapped.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
    };

    let mut stats = BackfillStats::default();
    let mut to_read: Vec<i64> = Vec::new();
    let mut to_star: Vec<i64> = Vec::new();
    let mut to_hide: Vec<i64> = Vec::new();
    for r in &rows {
        let content_text = html_to_text(r.content.as_deref().unwrap_or(""));
        let summary_text = html_to_text(r.summary.as_deref().unwrap_or(""));
        let out = engine.evaluate(
            r.source_id,
            r.group_id,
            &r.source_url,
            &r.title,
            &format!("{content_text} {summary_text}"),
            r.author.as_deref(),
            r.url.as_deref(),
        );
        if out.mark_read {
            to_read.push(r.id);
        }
        if out.star {
            to_star.push(r.id);
        }
        if out.hide {
            to_hide.push(r.id);
            // Hidden articles leave the reading flow entirely.
            if !to_read.contains(&r.id) {
                to_read.push(r.id);
            }
        }
        if out.notify {
            stats.notified += 1;
        }
    }

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for id in &to_read {
        tx.execute("UPDATE items SET has_been_read=1 WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    for id in &to_star {
        tx.execute("UPDATE items SET starred=1 WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    for id in &to_hide {
        tx.execute("UPDATE items SET hidden=1 WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    stats.marked_read = to_read.len();
    stats.starred = to_star.len();
    stats.hidden = to_hide.len();
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RuleInput;

    fn rule_input(name: &str, pattern: &str, target: &str, action: &str, case_sensitive: bool, scope: &str) -> RuleInput {
        RuleInput {
            name: name.into(),
            pattern: pattern.into(),
            target_field: target.into(),
            action_type: action.into(),
            is_case_sensitive: case_sensitive,
            is_enabled: true,
            source_scope: scope.into(),
        }
    }

    #[test]
    fn test_evaluate_actions_and_scope() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let g = crate::db::create_group(&conn, "G").unwrap();
        let s = crate::db::insert_source(&conn, "https://s.example", "S", None, Some(g.id)).unwrap();
        let s2 = crate::db::insert_source(&conn, "https://t.example", "T", None, None).unwrap();

        // case-insensitive title rule scoped to group G
        let _r1 = crate::db::create_rule(&conn, &rule_input("ad", "广告|ADs?", "title", "mark_read", false, "all")).unwrap();
        let _r2 = crate::db::create_rule(&conn, &rule_input("star-release", "重磅", "any", "star", false, &format!("source:{}", s.id))).unwrap();
        let _r3 = crate::db::create_rule(&conn, &rule_input("hide-spam", "casino", "content", "hide", false, "all")).unwrap();
        let _r4 = crate::db::create_rule(&conn, &rule_input("notify-url", "breaking", "source_url", "notify", true, "all")).unwrap();

        let engine = RuleEngine::load(&conn).unwrap();
        assert_eq!(engine.rules.len(), 4);

        // matches case-insensitively
        let out = engine.evaluate(s.id, Some(g.id), "https://s.example", "今日广告合集", "正常内容", None, None);
        assert!(out.mark_read && !out.star && !out.hide);

        // scope-limited rule applies only to its source
        let out_s = engine.evaluate(s.id, Some(g.id), "https://s.example", "这是重磅发布", "", None, None);
        assert!(out_s.star);
        let out_t = engine.evaluate(s2.id, None, "https://t.example", "这是重磅发布", "", None, None);
        assert!(!out_t.star);

        // hide implies leaving list; content plain-text matching works over html
        let out_h = engine.evaluate(s.id, Some(g.id), "https://s.example", "t", "<b>casino</b> night", None, None);
        assert!(out_h.hide);

        // case-sensitive rule does not match different case
        let out_c = engine.evaluate(s.id, Some(g.id), "https://s.example", "x", "", None, Some("https://s.example/Breaking"));
        assert!(!out_c.notify);
        let out_cs = engine.evaluate(s.id, Some(g.id), "https://s.example", "x", "", None, Some("https://s.example/breaking"));
        assert!(out_cs.notify);
    }

    #[test]
    fn test_compile_rejects_invalid() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let bad = crate::db::create_rule(&conn, &rule_input("bad", "([unclosed", "title", "star", false, "all")).unwrap();
        let rule = crate::db::get_rule(&conn, bad.id).unwrap();
        assert!(compile_pattern(&rule).is_err());
    }
}
