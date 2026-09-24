use crate::db;
use crate::models::{html_to_text, Rule, RuleActionType, RuleSourceScope, RuleTargetField};
use regex::{Regex, RegexSet};
use rusqlite::{params, Connection};

/// 针对同一目标字段的一组规则，编译为单个 `RegexSet` 以支持单遍批量匹配。
struct FieldRuleGroup {
    rules: Vec<Rule>,
    set: Option<RegexSet>,
}

impl FieldRuleGroup {
    fn new(rules: Vec<Rule>) -> Result<Self, String> {
        if rules.is_empty() {
            return Ok(Self { rules, set: None });
        }
        let patterns: Vec<String> = rules
            .iter()
            .map(|r| {
                if r.is_case_sensitive {
                    r.pattern.clone()
                } else {
                    format!("(?i){}", r.pattern)
                }
            })
            .collect();
        let set = RegexSet::new(&patterns).map_err(|e| format!("invalid regex in rules: {e}"))?;
        Ok(Self {
            rules,
            set: Some(set),
        })
    }
}

/// 已编译且已启用的规则集合，可直接用于执行评估。
pub struct RuleEngine {
    title_rules: FieldRuleGroup,
    content_rules: FieldRuleGroup,
    author_rules: FieldRuleGroup,
    source_url_rules: FieldRuleGroup,
    any_rules: FieldRuleGroup,
    total_rules: usize,
}

/// 规则评估后针对单篇文章应用的动作结果。
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

/// 编译正则模式，若不区分大小写则在开头内联追加 `(?i)` 标志。
pub fn compile_pattern(rule: &Rule) -> Result<Regex, String> {
    let mut pattern = String::new();
    if !rule.is_case_sensitive {
        pattern.push_str("(?i)");
    }
    pattern.push_str(&rule.pattern);
    Regex::new(&pattern).map_err(|e| format!("invalid regex: {e}"))
}

/// 规则评估上下文，打包了针对单篇文章匹配所需的所有字段信息。
#[derive(Debug, Clone)]
pub struct ArticleEvalContext<'a> {
    pub source_id: i64,
    pub group_id: Option<i64>,
    pub source_url: &'a str,
    pub title: &'a str,
    pub content_text: &'a str,
    pub author: Option<&'a str>,
    pub url: Option<&'a str>,
}

impl RuleEngine {
    /// 从数据库加载并编译所有已启用的规则，按字段归类为多个 RegexSet 组。
    pub fn load(conn: &Connection) -> Result<Self, String> {
        let all = db::get_rules(conn)?;
        let mut title_rules = Vec::new();
        let mut content_rules = Vec::new();
        let mut author_rules = Vec::new();
        let mut source_url_rules = Vec::new();
        let mut any_rules = Vec::new();
        let mut total_rules = 0;

        for r in all {
            if !r.is_enabled {
                continue;
            }
            total_rules += 1;
            match r.target_field {
                RuleTargetField::Title => title_rules.push(r),
                RuleTargetField::Content => content_rules.push(r),
                RuleTargetField::Author => author_rules.push(r),
                RuleTargetField::SourceUrl => source_url_rules.push(r),
                RuleTargetField::Any => any_rules.push(r),
            }
        }

        Ok(Self {
            title_rules: FieldRuleGroup::new(title_rules)?,
            content_rules: FieldRuleGroup::new(content_rules)?,
            author_rules: FieldRuleGroup::new(author_rules)?,
            source_url_rules: FieldRuleGroup::new(source_url_rules)?,
            any_rules: FieldRuleGroup::new(any_rules)?,
            total_rules,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.total_rules == 0
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.total_rules
    }

    fn scope_applies(rule: &Rule, source_id: i64, group_id: Option<i64>) -> bool {
        match rule.source_scope {
            RuleSourceScope::All => true,
            RuleSourceScope::Source(id) => id == source_id,
            RuleSourceScope::Group(id) => Some(id) == group_id,
        }
    }

    fn apply_action(action: RuleActionType, out: &mut Outcome) {
        match action {
            RuleActionType::MarkRead => out.mark_read = true,
            RuleActionType::Star => out.star = true,
            RuleActionType::Hide => out.hide = true,
            RuleActionType::Notify => out.notify = true,
        }
    }

    /// 针对单篇文章上下文评估所有适用的规则。
    pub fn evaluate(&self, ctx: &ArticleEvalContext<'_>) -> Outcome {
        let mut out = Outcome::default();
        if self.total_rules == 0 {
            return out;
        }

        if let Some(set) = &self.title_rules.set {
            for idx in set.matches(ctx.title) {
                let rule = &self.title_rules.rules[idx];
                if Self::scope_applies(rule, ctx.source_id, ctx.group_id) {
                    Self::apply_action(rule.action_type, &mut out);
                }
            }
        }

        if let Some(set) = &self.content_rules.set {
            for idx in set.matches(ctx.content_text) {
                let rule = &self.content_rules.rules[idx];
                if Self::scope_applies(rule, ctx.source_id, ctx.group_id) {
                    Self::apply_action(rule.action_type, &mut out);
                }
            }
        }

        if let Some(set) = &self.author_rules.set {
            let author_str = ctx.author.unwrap_or("");
            for idx in set.matches(author_str) {
                let rule = &self.author_rules.rules[idx];
                if Self::scope_applies(rule, ctx.source_id, ctx.group_id) {
                    Self::apply_action(rule.action_type, &mut out);
                }
            }
        }

        if let Some(set) = &self.source_url_rules.set {
            let source_url_str = match ctx.url {
                Some(url) => format!("{url} {}", ctx.source_url),
                None => ctx.source_url.to_string(),
            };
            for idx in set.matches(&source_url_str) {
                let rule = &self.source_url_rules.rules[idx];
                if Self::scope_applies(rule, ctx.source_id, ctx.group_id) {
                    Self::apply_action(rule.action_type, &mut out);
                }
            }
        }

        if let Some(set) = &self.any_rules.set {
            let any_str = format!(
                "{} {} {} {} {}",
                ctx.title,
                ctx.content_text,
                ctx.author.unwrap_or(""),
                ctx.url.unwrap_or(""),
                ctx.source_url
            );
            for idx in set.matches(&any_str) {
                let rule = &self.any_rules.rules[idx];
                if Self::scope_applies(rule, ctx.source_id, ctx.group_id) {
                    Self::apply_action(rule.action_type, &mut out);
                }
            }
        }

        out
    }
}

/// 单遍扫描并将所有规则应用到现有文章（历史回溯）。
/// 数据行按 ID 分页流式处理（在大型归档下保证内存受限），仅累积 ID 列表；
/// 标志位写入在单个事务中通过分批 IN 语句批量提交，而非逐行单独执行 UPDATE。
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
    const PAGE: i64 = 1000;

    let mut stats = BackfillStats::default();
    let mut to_read: Vec<i64> = Vec::new();
    let mut to_star: Vec<i64> = Vec::new();
    let mut to_hide: Vec<i64> = Vec::new();

    let mut last_id = 0i64;
    loop {
        let rows: Vec<Row> = {
            let mut stmt = conn
                .prepare(
                    "SELECT i.id, i.source_id, s.group_id, s.url, i.title, i.content, i.summary, i.author, i.url
                     FROM items i JOIN sources s ON s.id = i.source_id
                     WHERE i.id > ?1 ORDER BY i.id LIMIT ?2",
                )
                .map_err(|e| e.to_string())?;
            let mapped = stmt
                .query_map(params![last_id, PAGE], |row| {
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
        if rows.is_empty() {
            break;
        }
        last_id = rows.last().map(|r| r.id).unwrap_or(last_id);

        for r in &rows {
            let content_text = html_to_text(r.content.as_deref().unwrap_or(""));
            let summary_text = html_to_text(r.summary.as_deref().unwrap_or(""));
            let full_text = format!("{content_text} {summary_text}");
            let ctx = ArticleEvalContext {
                source_id: r.source_id,
                group_id: r.group_id,
                source_url: &r.source_url,
                title: &r.title,
                content_text: &full_text,
                author: r.author.as_deref(),
                url: r.url.as_deref(),
            };
            let out = engine.evaluate(&ctx);
            if out.mark_read {
                to_read.push(r.id);
            }
            if out.star {
                to_star.push(r.id);
            }
            if out.hide {
                to_hide.push(r.id);
                // 隐藏文章完全脱离阅读流（同时标记为已读）。
                if !out.mark_read {
                    to_read.push(r.id);
                }
            }
            if out.notify {
                stats.notified += 1;
            }
        }
    }

    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    batch_set_flag(&tx, "has_been_read", &to_read)?;
    batch_set_flag(&tx, "starred", &to_star)?;
    batch_set_flag(&tx, "hidden", &to_hide)?;
    tx.commit().map_err(|e| e.to_string())?;

    stats.marked_read = to_read.len();
    stats.starred = to_star.len();
    stats.hidden = to_hide.len();
    Ok(stats)
}

/// 使用分批 `WHERE id IN` 语句为多行批量设置某个布尔标志位列。
/// `column` 始终为内部常量，绝非用户输入。
fn batch_set_flag(
    tx: &rusqlite::Transaction,
    column: &str,
    ids: &[i64],
) -> Result<(), String> {
    const CHUNK: usize = 500;
    for chunk in ids.chunks(CHUNK) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders: Vec<String> =
            (1..=chunk.len()).map(|i| format!("?{i}")).collect();
        let sql = format!("UPDATE items SET {column}=1 WHERE id IN ({})", placeholders.join(","));
        let mut stmt = tx.prepare(&sql).map_err(|e| e.to_string())?;
        stmt.execute(rusqlite::params_from_iter(chunk.iter()))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RuleInput;

    fn rule_input(
        name: &str,
        pattern: &str,
        target: RuleTargetField,
        action: RuleActionType,
        case_sensitive: bool,
        scope: RuleSourceScope,
    ) -> RuleInput {
        RuleInput {
            name: name.into(),
            pattern: pattern.into(),
            target_field: target,
            action_type: action,
            is_case_sensitive: case_sensitive,
            is_enabled: true,
            source_scope: scope,
        }
    }

    #[test]
    fn test_evaluate_actions_and_scope() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let g = crate::db::create_group(&conn, "G").unwrap();
        let s = crate::db::insert_source(&conn, "https://s.example", "S", None, Some(g.id)).unwrap();
        let s2 = crate::db::insert_source(&conn, "https://t.example", "T", None, None).unwrap();

        // 作用于分组 G 且不区分大小写的标题规则
        let _r1 = crate::db::create_rule(&conn, &rule_input("ad", "广告|ADs?", RuleTargetField::Title, RuleActionType::MarkRead, false, RuleSourceScope::All)).unwrap();
        let _r2 = crate::db::create_rule(&conn, &rule_input("star-release", "重磅", RuleTargetField::Any, RuleActionType::Star, false, RuleSourceScope::Source(s.id))).unwrap();
        let _r3 = crate::db::create_rule(&conn, &rule_input("hide-spam", "casino", RuleTargetField::Content, RuleActionType::Hide, false, RuleSourceScope::All)).unwrap();
        let _r4 = crate::db::create_rule(&conn, &rule_input("notify-url", "breaking", RuleTargetField::SourceUrl, RuleActionType::Notify, true, RuleSourceScope::All)).unwrap();

        let engine = RuleEngine::load(&conn).unwrap();
        assert_eq!(engine.len(), 4);

        // 不区分大小写匹配
        let out = engine.evaluate(&ArticleEvalContext {
            source_id: s.id,
            group_id: Some(g.id),
            source_url: "https://s.example",
            title: "今日广告合集",
            content_text: "正常内容",
            author: None,
            url: None,
        });
        assert!(out.mark_read && !out.star && !out.hide);

        // 受限作用域规则仅应用于指定订阅源
        let out_s = engine.evaluate(&ArticleEvalContext {
            source_id: s.id,
            group_id: Some(g.id),
            source_url: "https://s.example",
            title: "这是重磅发布",
            content_text: "",
            author: None,
            url: None,
        });
        assert!(out_s.star);
        let out_t = engine.evaluate(&ArticleEvalContext {
            source_id: s2.id,
            group_id: None,
            source_url: "https://t.example",
            title: "这是重磅发布",
            content_text: "",
            author: None,
            url: None,
        });
        assert!(!out_t.star);

        // 隐藏意味着离开列表；正文纯文本匹配能够穿透 HTML
        let out_h = engine.evaluate(&ArticleEvalContext {
            source_id: s.id,
            group_id: Some(g.id),
            source_url: "https://s.example",
            title: "t",
            content_text: "casino night",
            author: None,
            url: None,
        });
        assert!(out_h.hide);

        // 区分大小写的规则不会匹配不同大小写
        let out_c = engine.evaluate(&ArticleEvalContext {
            source_id: s.id,
            group_id: Some(g.id),
            source_url: "https://s.example",
            title: "x",
            content_text: "",
            author: None,
            url: Some("https://s.example/Breaking"),
        });
        assert!(!out_c.notify);
        let out_cs = engine.evaluate(&ArticleEvalContext {
            source_id: s.id,
            group_id: Some(g.id),
            source_url: "https://s.example",
            title: "x",
            content_text: "",
            author: None,
            url: Some("https://s.example/breaking"),
        });
        assert!(out_cs.notify);
    }

    #[test]
    fn test_backfill_pages_through_thousands() {
        // 给定：2500 篇文章（跨多个 ID 分页），隔篇匹配
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let s = crate::db::insert_source(&conn, "https://s.example", "S", None, None).unwrap();
        let _r = crate::db::create_rule(
            &conn,
            &rule_input("m", "matchme", RuleTargetField::Title, RuleActionType::MarkRead, false, RuleSourceScope::All),
        )
        .unwrap();
        for i in 0..2500 {
            let title = if i % 2 == 0 { "matchme article" } else { "other article" };
            crate::db::insert_item(
                &conn,
                s.id,
                &crate::db::UpsertEntry {
                    guid: &format!("g{i}"),
                    title,
                    url: None,
                    author: None,
                    published_at: i,
                    content: Some("x"),
                    summary: None,
                    snippet: Some("x"),
                    image: None,
                    has_been_read: false,
                    starred: false,
                    hidden: false,
                },
            )
            .unwrap();
        }
        // 当：执行回溯应用时
        let engine = RuleEngine::load(&conn).unwrap();
        let stats = backfill(&conn, &engine).unwrap();
        // 那么：恰好匹配的一半被标记，分页过程中无遗漏
        assert_eq!(stats.marked_read, 1250);
        let remaining = crate::db::get_items(
            &conn,
            &crate::models::GetItemsParams { filter: Some(1), limit: Some(3000), ..Default::default() },
        )
        .unwrap();
        assert_eq!(remaining.len(), 1250);
    }

    #[test]
    fn test_compile_rejects_invalid() {        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let bad = crate::db::create_rule(&conn, &rule_input("bad", "([unclosed", RuleTargetField::Title, RuleActionType::Star, false, RuleSourceScope::All)).unwrap();
        let rule = crate::db::get_rule(&conn, bad.id).unwrap();
        assert!(compile_pattern(&rule).is_err());
    }
}
