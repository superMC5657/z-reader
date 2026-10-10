//! 订阅源抓取与生命周期服务：唯一的单源生命周期 Module。
//!
//! 锁纪律契约（唯一出处）：`await` 网络段绝不跨持 `db` 锁；仅存库/标记状态/图标路径写入的
//! 短暂临界区内加锁。本模块之外不再重复描述该纪律。
//!
//! 两段式 Interface：[`FetchSpec`]（抓取）与 [`StoreSpec`]（存库）共同覆盖原 10 字段上下文
//!（`source_id` 两端共享）；[`refresh_single_source`] 为便捷入口（内部分两段调用），
//! [`store_parsed`] 为存库段入口（跳过抓取：store→标状态→补图标，供 `add_source` 复用已预检的
//! `ParsedFeed`）。

#![allow(dead_code)]

use crate::feed::ParsedFeed;

/// 刷新订阅源的产出结果。
#[derive(Debug, Default, Clone)]
pub struct SourceRefreshOutcome {
    pub inserted: usize,
    pub notified: Vec<String>,
    pub failed: bool,
    pub error_kind: Option<String>,
}

/// 抓取段规格：纯网络输入（`client/url/cycle/source_id`），不触锁。
#[derive(Clone, Copy)]
pub struct FetchSpec<'a> {
    pub client: &'a reqwest::Client,
    pub url: &'a str,
    pub cycle: &'a str,
    pub source_id: i64,
}

/// 存库段规格：`db/engine/source_id/group_id/favicon/favicon_dir/allow_third_party` + `ParsedFeed`。
///
/// 网络所需的 `client/url/cycle` 由 [`FetchSpec`] 随行提供（存库段入口取两规格共同输入，
/// `source_id` 两端一致）；`ParsedFeed` 作独立参数传入。
#[derive(Clone, Copy)]
pub struct StoreSpec<'a> {
    pub db: &'a tokio::sync::Mutex<rusqlite::Connection>,
    pub engine: Option<&'a crate::rules::RuleEngine>,
    pub source_id: i64,
    pub group_id: Option<i64>,
    /// 当前已持久化的 favicon 路径；`None`/空字符串均视为缺失并触发回退补抓。
    pub favicon: Option<&'a str>,
    /// 缺图标时的写入目录；`None` 表示跳过补抓（调用方无目录时）。
    pub favicon_dir: Option<&'a std::path::Path>,
    pub allow_third_party: bool,
}

/// 单源刷新的参数包（便捷入口用，与两段规格等价）。
///
/// 聚合后调用参数不超过 clippy `too_many_arguments` 上限（7 个）。
#[derive(Clone, Copy)]
pub struct RefreshSingleSourceContext<'a> {
    pub client: &'a reqwest::Client,
    /// 见模块顶锁纪律契约。
    pub db: &'a tokio::sync::Mutex<rusqlite::Connection>,
    pub engine: Option<&'a crate::rules::RuleEngine>,
    pub source_id: i64,
    pub group_id: Option<i64>,
    pub url: &'a str,
    /// 当前已持久化的 favicon 路径；`None`/空字符串均视为缺失并触发回退补抓。
    pub favicon: Option<&'a str>,
    /// 缺图标时的写入目录；`None` 表示跳过补抓（调用方无目录时）。
    pub favicon_dir: Option<&'a std::path::Path>,
    pub allow_third_party: bool,
    /// 刷新周期 ID；为空时与旧调用方行为一致（日志中省略）。
    pub cycle: &'a str,
}

/// `feed::StoreOutcome` 到 [`SourceRefreshOutcome`] 的纯提升（成功态），收归本 Module。
pub fn promote_store(s: &crate::feed::StoreOutcome) -> SourceRefreshOutcome {
    SourceRefreshOutcome {
        inserted: s.inserted,
        notified: s.notified.clone(),
        failed: false,
        error_kind: None,
    }
}

/// 抓取段：下载并解析订阅源。纯网络阶段，不持有数据库锁。
pub async fn fetch_stage(spec: FetchSpec<'_>) -> Result<ParsedFeed, String> {
    crate::feed::fetch_and_parse_with(spec.client, spec.url, spec.cycle, Some(spec.source_id)).await
}

/// 存库段入口（跳过抓取）：store→标状态→补图标，供 `add_source` 复用已预检的 `ParsedFeed`。
///
/// `fetch_ms` 为抓取段耗时（复用路径传 0，保持 `fetch_ms/store_ms/favicon_ms/elapsed_ms` 键名不变）；
/// `net` 提供存库与补图标所需的 `client/url/cycle`（与 `store.source_id` 一致）。
/// 网络 `await` 段不持锁，仅短临界区加锁（见模块顶契约）。
pub async fn store_parsed(
    store: StoreSpec<'_>,
    net: FetchSpec<'_>,
    parsed: &ParsedFeed,
    fetch_ms: u128,
) -> Result<SourceRefreshOutcome, String> {
    let StoreSpec {
        db,
        engine,
        source_id,
        group_id,
        favicon,
        favicon_dir,
        allow_third_party,
    } = store;
    let FetchSpec { client, url, cycle, .. } = net;
    // 日志仅输出主机名：完整 URL 可能包含令牌或查询参数等机密信息。
    let (host, _) = crate::net::sanitize_url(url);
    let mut out = SourceRefreshOutcome::default();
    let store_start = std::time::Instant::now();

    // 存库（短暂加锁临界区：仅包裹同步的 `feed::store` 调用）。
    // `feed::store` 内部经 Verdict 模块裁决，绝不修改已有数据行（IGNORE）。
    let source_ctx = crate::feed::SourceCtx {
        id: source_id,
        group_id,
        url: url.to_string(),
    };
    let stored = {
        let conn = db.lock().await;
        crate::feed::store(&conn, &source_ctx, parsed, engine)
    };
    let store_ms = store_start.elapsed().as_millis();
    match stored {
        Ok(stored) => {
            let promoted = promote_store(&stored);
            out.inserted = promoted.inserted;
            out.notified = promoted.notified;
        }
        Err(e) => {
            out.failed = true;
            let error_kind = crate::net::classify_error(&e).to_string();
            out.error_kind = Some(error_kind.clone());
            log::warn!(
                "feed store failed cycle={cycle} id={source_id} host={host} error_kind={error_kind} reason {}",
                crate::utils::short_reason(&e)
            );
            let conn = db.lock().await;
            let _ = crate::db::mark_source_fetched(&conn, source_id, false, Some(&e));
            return Ok(out);
        }
    }

    // Favicon 缺失回退补全（仅在缺失时触发；网络段不持锁，仅写入路径时短暂加锁）。
    let favicon_missing = favicon.is_none() || favicon.is_some_and(|s| s.is_empty());
    let favicon_start = std::time::Instant::now();
    if favicon_missing {
        if let Some(dir) = favicon_dir {
            let icon_url = parsed.icon_url.as_deref();
            let site_url = parsed.site_url.as_deref();
            if let Some(fav) = crate::feed::fetch_favicon_with(
                client,
                crate::feed::FaviconFetchOptions {
                    feed_url: url,
                    icon_url,
                    site_url,
                    favicon_dir: dir,
                    source_id,
                    allow_third_party,
                    cycle,
                },
            )
            .await
            {
                let conn = db.lock().await;
                let _ = crate::db::set_source_favicon(
                    &conn,
                    source_id,
                    fav.to_string_lossy().as_ref(),
                );
            }
        }
    }
    let favicon_ms = favicon_start.elapsed().as_millis();

    // 标记成功（仅在存库成功时）并输出单源成功日志。
    {
        let conn = db.lock().await;
        let _ = crate::db::mark_source_fetched(&conn, source_id, true, None);
    }
    let elapsed_ms = store_start.elapsed().as_millis() + fetch_ms;
    if out.inserted > 0 {
        log::info!("feed refreshed cycle={cycle} id={source_id} host={host} new={} fetch_ms={fetch_ms} store_ms={store_ms} favicon_ms={favicon_ms} elapsed_ms={elapsed_ms}", out.inserted);
    } else {
        log::info!("feed up to date cycle={cycle} id={source_id} host={host} fetch_ms={fetch_ms} store_ms={store_ms} favicon_ms={favicon_ms} elapsed_ms={elapsed_ms}");
    }
    Ok(out)
}

/// 刷新单个订阅源的完整生命周期（便捷入口，内部分两段调用）：
///
/// 1. 抓取与解析（fetch 段，纯网络，不持锁）；
/// 2. 存库段（store→标状态→补图标，见 [`store_parsed`]）。
///
/// 成功时输出与旧单源刷新一致的 INFO 分段耗时日志
///（`fetch_ms` / `store_ms` / `favicon_ms` / `elapsed_ms`，仅主机名）；
/// 绝不向上抛出抓取/存库的业务失败：失败时标记该源为失败状态并返回
/// `Ok(Outcome { failed: true, error_kind })`；
/// `Err(String)` 仅保留给调用方编程期错误，内部错误文本均经
/// `utils::short_reason` 收敛为单行后记录与持久化。
pub async fn refresh_single_source(
    ctx: RefreshSingleSourceContext<'_>,
) -> Result<SourceRefreshOutcome, String> {
    let RefreshSingleSourceContext {
        client,
        db,
        engine,
        source_id,
        group_id,
        url,
        favicon,
        favicon_dir,
        allow_third_party,
        cycle,
    } = ctx;
    let mut out = SourceRefreshOutcome::default();
    // 日志仅输出主机名。
    let (host, _) = crate::net::sanitize_url(url);

    // 1. 抓取与解析（纯网络阶段，绝不持有数据库锁）。
    let fetch_start = std::time::Instant::now();
    let parsed = match fetch_stage(FetchSpec { client, url, cycle, source_id }).await {
        Ok(p) => p,
        Err(e) => {
            out.failed = true;
            let fetch_ms = fetch_start.elapsed().as_millis();
            let error_kind = crate::net::classify_error(&e).to_string();
            out.error_kind = Some(error_kind.clone());
            log::warn!(
                "feed fetch failed cycle={cycle} id={source_id} host={host} error_kind={error_kind} fetch_ms={fetch_ms} reason {}",
                crate::utils::short_reason(&e)
            );
            let conn = db.lock().await;
            let _ = crate::db::mark_source_fetched(&conn, source_id, false, Some(&e));
            return Ok(out);
        }
    };
    let fetch_ms = fetch_start.elapsed().as_millis();

    // 2. 存库段（store→标状态→补图标）。
    store_parsed(
        StoreSpec { db, engine, source_id, group_id, favicon, favicon_dir, allow_third_party },
        FetchSpec { client, url, cycle, source_id },
        &parsed,
        fetch_ms,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_default_is_success_empty() {
        let out = SourceRefreshOutcome::default();
        assert!(!out.failed);
        assert_eq!(out.inserted, 0);
        assert!(out.notified.is_empty());
        assert!(out.error_kind.is_none());
    }

    #[test]
    fn promote_store_lifts_inserted_and_notified() {
        // 给定：feed 存库结果
        let s = crate::feed::StoreOutcome { inserted: 2, notified: vec!["a".into()] };
        // 当：纯提升时
        let out = promote_store(&s);
        // 那么：成功态原样提升，无失败标记
        assert_eq!(out.inserted, 2);
        assert_eq!(out.notified, vec!["a".to_string()]);
        assert!(!out.failed && out.error_kind.is_none());
    }

    /// 存库段：内存库 + RSS fixture（含双源/缺 summary），与 #1 verdict 一致且重入幂等。
    #[test]
    fn store_parsed_memdb_matches_verdict_and_idempotent() {
        // 给定：内存库 + 订阅源 + 规则（拆分关键词标已读，摘要侧命中通知）
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let s = crate::db::insert_source(&conn, "https://s.example/feed", "S", None, None).unwrap();
        let source_id = s.id;
        for (name, pattern, action) in [
            ("split", "alpha beta", crate::models::RuleActionType::MarkRead),
            ("ping", "notify-xyz", crate::models::RuleActionType::Notify),
        ] {
            crate::db::create_rule(
                &conn,
                &crate::models::RuleInput {
                    name: name.into(),
                    pattern: pattern.into(),
                    target_field: crate::models::RuleTargetField::Content,
                    action_type: action,
                    is_case_sensitive: false,
                    is_enabled: true,
                    source_scope: crate::models::RuleSourceScope::All,
                },
            )
            .unwrap();
        }
        let engine = crate::rules::RuleEngine::load(&conn).unwrap();
        // RSS fixture：g1 双源（content alpha + description beta），g2 缺 summary（仅 content:encoded）
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/">
        <channel><title>T</title><link>https://s.example/</link>
        <item><guid>g1</guid><title>First</title><link>https://s.example/1</link><description>beta and notify-xyz</description><content:encoded><![CDATA[<p>alpha</p>]]></content:encoded></item>
        <item><guid>g2</guid><title>Second</title><link>https://s.example/2</link><content:encoded><![CDATA[<p>plain</p>]]></content:encoded></item>
        </channel></rss>"#;
        let parsed = crate::feed::parse_feed_data(xml.as_bytes(), Some("https://s.example/feed"), "https://s.example/feed").unwrap();
        assert_eq!(parsed.entries.len(), 2);
        // g1 应为双源（content+summary 均非空），g2 缺 summary
        let g1 = parsed.entries.iter().find(|e| e.guid == "g1").unwrap();
        assert!(!g1.content.is_empty() && g1.summary.is_some(), "g1 dual");
        let g2 = parsed.entries.iter().find(|e| e.guid == "g2").unwrap();
        assert!(g2.summary.is_none(), "g2 missing summary");

        // 当：经存库段入口写入（跳过抓取，favicon_dir=None 跳过补图标无网络）
        let db = tokio::sync::Mutex::new(conn);
        let client = reqwest::Client::new();
        let out1 = tauri::async_runtime::block_on(store_parsed(
            StoreSpec { db: &db, engine: Some(&engine), source_id, group_id: None, favicon: None, favicon_dir: None, allow_third_party: false },
            FetchSpec { client: &client, url: "https://s.example/feed", cycle: "", source_id },
            &parsed,
            0,
        ))
        .unwrap();
        // 那么：新增 2，通知命中 g1（含 notify-xyz），未失败
        assert!(!out1.failed, "store should succeed");
        assert_eq!(out1.inserted, 2);
        assert_eq!(out1.notified, vec!["First".to_string()]);
        // 与 #1 verdict 一致：g1 拆分关键词已读，g2 未读
        {
            let guard = db.try_lock().expect("lock");
            let first: bool = guard.query_row("SELECT has_been_read FROM items WHERE guid='g1'", [], |r| r.get::<_, i64>(0)).unwrap() != 0;
            assert!(first, "g1 split keyword should mark read per verdict");
            let second: bool = guard.query_row("SELECT has_been_read FROM items WHERE guid='g2'", [], |r| r.get::<_, i64>(0)).unwrap() != 0;
            assert!(!second, "g2 should stay unread");
        }
        // 当：二次重入同一 ParsedFeed 时
        let out2 = tauri::async_runtime::block_on(store_parsed(
            StoreSpec { db: &db, engine: Some(&engine), source_id, group_id: None, favicon: None, favicon_dir: None, allow_third_party: false },
            FetchSpec { client: &client, url: "https://s.example/feed", cycle: "", source_id },
            &parsed,
            0,
        ))
        .unwrap();
        // 那么：幂等 inserted==0（IGNORE），未失败
        assert_eq!(out2.inserted, 0);
        assert!(!out2.failed);
    }
}
