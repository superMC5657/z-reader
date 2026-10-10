//! 文章与内容处理服务：唯一的 Verdict 归一模块（纯函数，无 DB/网络/AppHandle）。
//!
//! 职责（独占）：规则匹配、hide→已读唯一折叠、服务端状态与规则结果的 OR 合并、
//! 摘要选源（summary 优先回退 content）、HTML 清洗与单文本匹配。
//! 调用方只做 wire→原文的翻译并交出归一化文本，清洗与匹配统一在本模块内完成。

#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

use crate::models::html_to_text;
use crate::rules::{ArticleEvalContext, Outcome, RuleEngine};

/// 规则与服务端状态合并后的最终裁决（Verdict）。
///
/// - `has_been_read` 已含 hide→已读折叠与服务端已读的 OR；
/// - `starred` 为服务端加星与规则加星的 OR；
/// - `hidden` 仅由规则决定（单调置位，不回清）；
/// - `notify` 仅触发通知，不改变行状态。
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub has_been_read: bool,
    pub starred: bool,
    pub hidden: bool,
    pub notify: bool,
}

/// 提取 HTML 或富文本内容的纯文本摘要，修剪首尾空白并截取前 200 个字符。
///
/// 对齐 `feed.rs`、`sync.rs` 与 `extractor.rs` 的摘要截取逻辑。
pub fn snippet_of(html: &str) -> String {
    let text = html_to_text(html);
    text.trim().chars().take(200).collect::<String>()
}

/// 摘要选源的唯一收口：summary 优先，回退到 content，行为与历史一致。
///
/// `feed` 传双源（content + summary），`sync` 传合并体（`GItem.content`）配 `None`，
/// 输出与此前各调用方手写的 `summary.unwrap_or(content)` 完全一致，仅收拢位置。
pub fn snippet_for(content: &str, summary: Option<&str>) -> String {
    snippet_of(summary.unwrap_or(content))
}

/// 将双源原文拼成单路归一化文本（未清洗）：`content + " " + summary`。
///
/// Adapter 只做 wire→原文翻译并交出该文本，清洗（`html_to_text`）统一在模块内完成。
/// 任一源缺失时退化为单源；双缺失为空串。不再区分 content-only 与 concat 两条老路。
pub fn normalize_body(content: Option<&str>, summary: Option<&str>) -> String {
    match (content, summary) {
        (Some(c), Some(s)) => format!("{c} {s}"),
        (Some(c), None) => c.to_string(),
        (None, Some(s)) => s.to_string(),
        (None, None) => String::new(),
    }
}

/// 单文本评估核心：对已归一化的原文统一清洗后匹配。
///
/// `body_raw` 为调用方按路由拼好的原文：feed 双源拼接、sync 合并体、backfill 行拼接。
/// 规则为空时直接返回默认 `Outcome`，调用方无需预检 `is_empty`。
pub fn evaluate_normalized(
    engine: &RuleEngine,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    body_raw: &str,
    author: Option<&str>,
    url: Option<&str>,
) -> Outcome {
    if engine.is_empty() {
        return Outcome::default();
    }
    let content_text = html_to_text(body_raw);
    let ctx = ArticleEvalContext {
        source_id,
        group_id,
        source_url,
        title,
        content_text: &content_text,
        author,
        url,
    };
    engine.evaluate(&ctx)
}

/// 唯一的折叠 + OR 点：原始 `Outcome` 与服务端状态合并为 `Verdict`。
///
/// - `has_been_read = server_read || outcome.mark_read || outcome.hide`（hide→已读唯一折叠）；
/// - `starred = server_starred || outcome.star`（服务端权威 LWW 之上的叠加）；
/// - `hidden = outcome.hide`（只置位，清除路径不存在）；
/// - `notify` 原样透出，不改变行状态。
pub fn merge_outcome_with_server(
    outcome: &Outcome,
    server_read: bool,
    server_starred: bool,
) -> Verdict {
    Verdict {
        has_been_read: server_read || outcome.mark_read || outcome.hide,
        starred: server_starred || outcome.star,
        hidden: outcome.hide,
        notify: outcome.notify,
    }
}

/// 双路共用的唯一 Interface：归一化单文本评估 + 折叠 + 服务端 OR，一次得出 `Verdict`。
///
/// `body_raw` 约定同 [`evaluate_normalized`]；本地路由传 `false/false` 作为服务端状态，
/// 远端路由传入 `GItem` 的已读/加星。调用方只收 `Verdict`，不再手写 `||hide`。
pub fn derive_verdict(
    engine: Option<&RuleEngine>,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    body_raw: &str,
    author: Option<&str>,
    url: Option<&str>,
    server_read: bool,
    server_starred: bool,
) -> Verdict {
    let outcome = match engine {
        Some(e) => evaluate_normalized_wrapper(e, source_id, group_id, source_url, title, body_raw, author, url),
        None => Outcome::default(),
    };
    merge_outcome_with_server(&outcome, server_read, server_starred)
}

fn evaluate_normalized_wrapper(
    engine: &RuleEngine,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    body_raw: &str,
    author: Option<&str>,
    url: Option<&str>,
) -> Outcome {
    evaluate_normalized(engine, source_id, group_id, source_url, title, body_raw, author, url)
}

/// 本地路由 Adapter（feed 新条目 / backfill 历史行）：双源拼成归一化文本后走共用 Interface。
///
/// 服务端状态固定为 `false/false`；已读/加星/隐藏/通知全由规则裁决（含 hide→已读折叠）。
pub fn verdict_for_local(
    engine: Option<&RuleEngine>,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    content: Option<&str>,
    summary: Option<&str>,
    author: Option<&str>,
    url: Option<&str>,
) -> Verdict {
    let body = normalize_body(content, summary);
    derive_verdict(engine, source_id, group_id, source_url, title, &body, author, url, false, false)
}

/// 远端路由 Adapter（sync 拉取）：上游已合并的 `GItem.content` 直接作为归一化文本。
///
/// 服务端已读/加星在此与规则结果做 OR（LWW 权威之上的叠加）；隐藏仍只由规则置位。
pub fn verdict_for_remote(
    engine: Option<&RuleEngine>,
    source_id: i64,
    group_id: Option<i64>,
    source_url: &str,
    title: &str,
    merged_raw: &str,
    author: Option<&str>,
    url: Option<&str>,
    server_read: bool,
    server_starred: bool,
) -> Verdict {
    derive_verdict(engine, source_id, group_id, source_url, title, merged_raw, author, url, server_read, server_starred)
}

/// 针对单篇文章组装 ArticleEvalContext 并执行规则匹配评估。
///
/// 统一 `feed.rs::store`（新条目入库前）、`sync.rs::pull_items`（远端拉取合并前）
/// 与 `rules.rs::backfill`（历史回溯）三处的上下文组装语义：
/// 双源先经 [`normalize_body`] 拼成单路原文，再由 [`evaluate_normalized`]
/// 统一清洗匹配。规则为空时直接返回默认 `Outcome`（全 false），调用方无需预检 `is_empty`。
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
    let body = normalize_body(content, summary);
    evaluate_normalized(engine, source_id, group_id, source_url, title, &body, author, url)
}

/// `evaluate_article_rules` 的 `Option` 引擎便捷封装：引擎缺失或为空时
/// 直接返回默认 `Outcome`，供旧调用方过渡复用。新代码优先使用
/// [`verdict_for_local`] / [`verdict_for_remote`] 直接拿 `Verdict`。
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
    use crate::models::{RuleActionType, RuleInput, RuleSourceScope, RuleTargetField};

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
            &RuleInput {
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

    // ---------- 双路摄取 parity（无网络无 AppHandle，内存库） ----------

    /// (1) 拆分关键词三路皆已读：content=<p>alpha</p> + summary=beta，规则 `alpha beta`。
    #[test]
    fn verdict_parity_split_keyword_three_routes_mark_read() {
        // 给定：正文规则 `alpha beta`（Content 域，标记已读）
        let (_conn, engine) = mem_engine("alpha beta", RuleTargetField::Content, RuleActionType::MarkRead);
        let engine_opt = Some(&engine);
        // 当：三路分别以各自归一化文本求裁决
        // feed 路：双源原文；sync 路：上游合并体原文；backfill 路：DB 行双源直调同一接口
        let feed = verdict_for_local(engine_opt, 1, None, "https://s.example", "t", Some("<p>alpha</p>"), Some("beta"), None, None);
        let sync = verdict_for_remote(engine_opt, 1, None, "https://s.example", "t", "<p>alpha</p> beta", None, None, false, false);
        let backfill_body = normalize_body(Some("<p>alpha</p>"), Some("beta"));
        let backfill = derive_verdict(engine_opt, 1, None, "https://s.example", "t", &backfill_body, None, None, false, false);
        // 那么：三路皆 mark_read（has_been_read 置位、无隐藏无星标）
        for v in [feed, sync, backfill] {
            assert!(v.has_been_read, "split keyword should mark read: {v:?}");
            assert!(!v.hidden && !v.starred);
        }
    }

    /// (2) summary-only 命中：关键词仅出现在摘要侧，拼接后仍需命中。
    #[test]
    fn verdict_parity_summary_only_hits() {
        // 给定：仅摘要含关键词的规则
        let (_conn, engine) = mem_engine("beta-only-xyz", RuleTargetField::Content, RuleActionType::MarkRead);
        let engine_opt = Some(&engine);
        // 当：正文不含关键词的三路裁决
        let feed = verdict_for_local(engine_opt, 1, None, "https://s.example", "t", Some("<p>plain</p>"), Some("beta-only-xyz here"), None, None);
        let sync = verdict_for_remote(engine_opt, 1, None, "https://s.example", "t", "<p>plain</p> beta-only-xyz here", None, None, false, false);
        let backfill = verdict_for_local(engine_opt, 1, None, "https://s.example", "t", Some("<p>plain</p>"), Some("beta-only-xyz here"), None, None);
        // 那么：三路皆已读（summary 信号未丢失）
        for v in [feed, sync, backfill] {
            assert!(v.has_been_read, "summary-only should hit: {v:?}");
        }
    }

    /// (3) hide→read 三路皆折叠：隐藏规则同时置位 hidden 与已读。
    #[test]
    fn verdict_parity_hide_folds_to_read_three_routes() {
        // 给定：隐藏规则
        let (_conn, engine) = mem_engine("secret-hide-xyz", RuleTargetField::Content, RuleActionType::Hide);
        let engine_opt = Some(&engine);
        // 当：三路裁决
        let feed = verdict_for_local(engine_opt, 1, None, "https://s.example", "t", Some("secret-hide-xyz body"), Some("extra"), None, None);
        let sync = verdict_for_remote(engine_opt, 1, None, "https://s.example", "t", "secret-hide-xyz body", None, None, false, false);
        let backfill = verdict_for_local(engine_opt, 1, None, "https://s.example", "t", Some("secret-hide-xyz body"), Some("extra"), None, None);
        // 那么：三路 hidden && has_been_read（唯一折叠点生效）
        for v in [feed, sync, backfill] {
            assert!(v.hidden && v.has_been_read, "hide must fold to read: {v:?}");
        }
    }

    /// (4) 服务端 OR 语义：quiet 保持、hide 置位、notify-only 仅通知。
    #[test]
    fn verdict_parity_server_or_semantics() {
        // 给定：空引擎（quiet）、hide 引擎、notify-only 引擎
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let quiet = RuleEngine::load(&conn).unwrap();
        assert!(quiet.is_empty());
        let (_c2, hide_engine) = mem_engine("hide-me", RuleTargetField::Content, RuleActionType::Hide);
        let (_c3, notify_engine) = mem_engine("notify-me", RuleTargetField::Content, RuleActionType::Notify);

        // 当：服务端已读/加星 + quiet 规则
        let keep = verdict_for_remote(Some(&quiet), 1, None, "https://s.example", "t", "plain", None, None, true, true);
        // 那么：服务端状态原样保持，无隐藏无通知
        assert!(keep.has_been_read && keep.starred && !keep.hidden && !keep.notify);

        // 当：服务端未读 + hide 命中
        let hid = verdict_for_remote(Some(&hide_engine), 1, None, "https://s.example", "t", "hide-me body", None, None, false, false);
        // 那么：隐藏置位且折叠为已读
        assert!(hid.hidden && hid.has_been_read);

        // 当：服务端未读 + 仅 notify 命中
        let only = verdict_for_remote(Some(&notify_engine), 1, None, "https://s.example", "t", "notify-me body", None, None, false, false);
        // 那么：仅 notify，不改变行状态
        assert!(only.notify && !only.has_been_read && !only.hidden && !only.starred);
    }

    /// (5) hidden 单调：先 hide 致 1，换 quiet 重 upsert 仍为 1，且 read/star 跟 LWW。
    #[test]
    fn verdict_parity_hidden_monotonic_upsert() {
        // 给定：内存库 + 订阅源
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let s = crate::db::insert_source(&conn, "https://s.example", "S", None, None).unwrap();
        // 当：先以 hide 裁决写入（read=1, star=0, hidden=1）
        let first = crate::db::upsert_remote_item(
            &conn,
            &crate::db::RemoteItemUpsert {
                remote_id: "mono1",
                source_id: s.id,
                title: "t",
                url: Some("https://s.example/a"),
                author: None,
                published_at: 100,
                content: Some("x"),
                summary: None,
                snippet: Some("x"),
                has_been_read: true,
                starred: false,
                hidden: true,
            },
        )
        .unwrap();
        assert!(first);
        // 当：再以 quiet 裁决重 upsert 同一远端（read=0, star=1, hidden=0）
        let second = crate::db::upsert_remote_item(
            &conn,
            &crate::db::RemoteItemUpsert {
                remote_id: "mono1",
                source_id: s.id,
                title: "t",
                url: Some("https://s.example/a"),
                author: None,
                published_at: 100,
                content: Some("x"),
                summary: None,
                snippet: Some("x"),
                has_been_read: false,
                starred: true,
                hidden: false,
            },
        )
        .unwrap();
        assert!(!second);
        // 那么：hidden 单调保持 1，read/star 跟随第二次 LWW
        let row = crate::db::get_items(&conn, &crate::models::GetItemsParams { filter: Some(3), ..Default::default() }).unwrap();
        assert_eq!(row.len(), 1, "hidden must stay 1 (monotonic)");
        assert!(row[0].hidden);
        assert!(!row[0].has_been_read, "read follows LWW");
        assert!(row[0].starred, "star follows LWW");
    }
}
