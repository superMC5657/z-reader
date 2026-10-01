//! 文章与内容处理服务。
//!
//! 包含纯文本摘要提取（snippet 生成）与基于规则引擎的文章属性评估。

#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use crate::models::html_to_text;
use crate::rules::{ArticleEvalContext, Outcome, RuleEngine};

/// 提取 HTML 或富文本内容的纯文本摘要，修剪首尾空白并截取前 200 个字符。
///
/// 对齐 `feed.rs`、`sync.rs` 与 `extractor.rs` 的摘要截取逻辑。
pub fn snippet_of(html: &str) -> String {
    let text = html_to_text(html);
    text.trim().chars().take(200).collect::<String>()
}

/// 针对单篇文章组装 ArticleEvalContext 并执行规则匹配评估。
///
/// 统一 `feed.rs::store`（新条目入库前）、`sync.rs::pull_items`（远端拉取合并前）
/// 与 `rules.rs::backfill`（历史回溯）三处的上下文组装语义：
/// 正文与摘要分别经 `html_to_text` 剥离标签后以单个空格拼接为 `content_text`，
/// 再连同标题/作者/链接与订阅源归属一起送入 `RuleEngine::evaluate`。
/// 规则为空时直接返回默认 `Outcome`（全 false），调用方无需预检 `is_empty`。
pub fn evaluate_article_rules(
    engine: &RuleEngine,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    content: Option<&str>,
    summary: Option<&str>,
    author: Option<&str>,
    url: Option<&str>,
) -> Outcome {
    if engine.is_empty() {
        return Outcome::default();
    }
    let content_text = html_to_text(content.unwrap_or(""));
    let summary_text = html_to_text(summary.unwrap_or(""));
    let full_text = format!("{content_text} {summary_text}");
    let ctx = ArticleEvalContext {
        source_id,
        group_id,
        source_url,
        title,
        content_text: &full_text,
        author,
        url,
    };
    engine.evaluate(&ctx)
}

/// `evaluate_article_rules` 的 `Option` 引擎便捷封装：引擎缺失或为空时
/// 直接返回默认 `Outcome`，供 `feed`/`sync` 入库路径复用以省去调用方分支。
pub fn evaluate_article_rules_opt(
    engine: Option<&RuleEngine>,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    content: Option<&str>,
    summary: Option<&str>,
    author: Option<&str>,
    url: Option<&str>,
) -> Outcome {
    match engine {
        Some(e) => evaluate_article_rules(
            e,
            source_id,
            group_id,
            source_url,
            title,
            content,
            summary,
            author,
            url,
        ),
        None => Outcome::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RuleActionType, RuleSourceScope, RuleTargetField};

    #[test]
    fn test_snippet_of() {
        let html = "<p>  Hello <b>World</b>!  </p>";
        assert_eq!(snippet_of(html), "Hello World!");

        let long_text = "<p>".to_string() + &"a".repeat(300) + "</p>";
        assert_eq!(snippet_of(&long_text).len(), 200);
    }

    fn mem_engine(pattern: &str, target: RuleTargetField, action: RuleActionType) -> (rusqlite::Connection, RuleEngine) {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        crate::db::create_rule(
            &conn,
            &crate::models::RuleInput {
                name: "t".into(),
                pattern: pattern.into(),
                target_field: target,
                action_type: action,
                is_case_sensitive: false,
                is_enabled: true,
                source_scope: RuleSourceScope::All,
            },
        )
        .unwrap();
        let engine = RuleEngine::load(&conn).unwrap();
        (conn, engine)
    }

    #[test]
    fn empty_engine_returns_default() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let engine = RuleEngine::load(&conn).unwrap();
        assert!(engine.is_empty());
        let out = evaluate_article_rules(
            &engine,
            1,
            None,
            "https://s.example",
            "anything",
            Some("<p>body</p>"),
            Some("sum"),
            None,
            None,
        );
        assert!(!out.mark_read && !out.star && !out.hide && !out.notify);
        let out2 = evaluate_article_rules_opt(
            None,
            1,
            None,
            "https://s.example",
            "anything",
            Some("body"),
            None,
            None,
            None,
        );
        assert!(!out2.mark_read && !out2.star && !out2.hide && !out2.notify);
    }

    #[test]
    fn content_and_summary_concatenated_for_matching() {
        // 规则仅匹配正文拼接文本：关键词拆分在 content 与 summary 两侧时仍需命中，
        // 与 feed::store / rules::backfill 的 full_text 语义一致。
        let (_conn, engine) = mem_engine(
            "alpha beta",
            RuleTargetField::Content,
            RuleActionType::MarkRead,
        );
        let out = evaluate_article_rules(
            &engine,
            1,
            None,
            "https://s.example",
            "title",
            Some("<p>alpha</p>"),
            Some("<p>beta</p>"),
            None,
            None,
        );
        assert!(out.mark_read);

        // HTML 标签应被剥离后再匹配。
        let (_conn2, engine2) =
            mem_engine("hello world", RuleTargetField::Content, RuleActionType::Hide);
        let out2 = evaluate_article_rules(
            &engine2,
            1,
            None,
            "https://s.example",
            "t",
            Some("<b>hello</b>"),
            Some("<i>world</i>"),
            None,
            None,
        );
        assert!(out2.hide);
    }
}
