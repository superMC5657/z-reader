//! 订阅源抓取与生命周期服务。
//!
//! 封装单源刷新的标准业务流程：抓取解析、规则评估、批量存库、标记状态与站点图标补抓。

#![allow(dead_code)]

/// 刷新订阅源的产出结果。
#[derive(Debug, Default, Clone)]
pub struct SourceRefreshOutcome {
    pub inserted: usize,
    pub notified: Vec<String>,
    pub failed: bool,
    pub error_kind: Option<String>,
}

/// 单源刷新的参数包。
///
/// 聚合后调用参数不超过 clippy `too_many_arguments` 上限（7 个）。
/// 对齐 `lib.rs::refresh_one_source`（后台/手动刷新）与
/// `commands.rs::add_source`（新增订阅源验证入库）两处流程语义：
///
/// - 前者：`fetch_and_parse_with`（带 cycle/id）→ `store`（带规则引擎）→
///   失败标状态 + 成功标状态 → 缺图标时 `fetch_favicon_with` 回退补全；
/// - 后者：`fetch_and_parse` → `insert_source`（调用方已完成）→ `store` →
///   标成功 → 缺图标时 `fetch_favicon` 回退补全。
///
/// 本服务覆盖“已存在订阅源”的刷新路径（抓取→存库→标状态→补图标），
/// 新增订阅源的 `insert_source` 建行步骤仍由调用方完成。
#[derive(Clone, Copy)]
pub struct RefreshSingleSourceContext<'a> {
    pub client: &'a reqwest::Client,
    /// 数据库互斥锁：各阶段仅在存库/标记状态的短暂临界区内加锁，
    /// 网络抓取期间绝不持有锁，避免慢速订阅源阻塞其余并发刷新任务。
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

/// 刷新单个订阅源的完整生命周期：
///
/// 1. 抓取与解析（fetch & parse）：发起 HTTP 请求获取远端订阅源 XML 并解析为结构化条目
///    （纯网络阶段，不持有数据库锁）；
/// 2. 规则评估（rules evaluation）：`feed::store` 内部对新条目执行与
///    `services::article::evaluate_article_rules` 同语义的规则匹配，
///    决定标已读/星标/隐藏/系统通知；
/// 3. 存储入库（db store）：将解析出的文章条目批量存入数据库（短暂加锁临界区）；
/// 4. 标记状态（mark status）：更新订阅源抓取状态（成功/失败及错误分类、时间戳，
///    均为短暂加锁临界区）；
/// 5. Favicon 补抓（favicon fetch）：若源缺少站点图标，拉取图标文件并持久化
///    （网络阶段不加锁，仅写入路径时短暂加锁）。
///
/// 成功时输出与旧 `lib.rs` 单源刷新一致的 INFO 分段耗时日志
///（`fetch_ms` / `store_ms` / `favicon_ms` / `elapsed_ms`，仅主机名）；
/// 绝不向上抛出抓取/存库的业务失败：失败时标记该源为失败状态并返回
/// `Ok(Outcome { failed: true, error_kind })`（对齐 `lib.rs` 单源刷新语义）；
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
    // 日志仅输出主机名：完整 URL 可能包含令牌或查询参数等机密信息。
    let (host, _) = crate::net::sanitize_url(url);
    // release 模式可见的单源耗时统计：仅主机名（不含原始 URL/查询令牌），
    // 拆分为 fetch / store / favicon 分段及总耗时。
    let total_start = std::time::Instant::now();

    // 1. 抓取与解析（纯网络阶段，绝不持有数据库锁）。
    let fetch_start = std::time::Instant::now();
    let parsed = match crate::feed::fetch_and_parse_with(client, url, cycle, Some(source_id)).await
    {
        Ok(p) => p,
        Err(e) => {
            out.failed = true;
            let fetch_ms = fetch_start.elapsed().as_millis();
            let error_kind = crate::net::classify_error(&e).to_string();
            out.error_kind = Some(error_kind.clone());
            // 失败保持为 warn（便于诊断），仅包含 cycle + id + host + error_kind + fetch 耗时 + 首行原因。
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

    // 2.+3. 规则过滤 + 存库（短暂加锁临界区：仅包裹同步的 `feed::store` 调用）。
    // `feed::store` 内部对每条新条目组装 `ArticleEvalContext`
    //（正文 + 摘要拼接）并调用 `RuleEngine::evaluate`，
    // 与 `services::article::evaluate_article_rules` 同语义，绝不修改已有数据行。
    let source_ctx = crate::feed::SourceCtx {
        id: source_id,
        group_id,
        url: url.to_string(),
    };
    let store_start = std::time::Instant::now();
    let stored = {
        let conn = db.lock().await;
        crate::feed::store(&conn, &source_ctx, &parsed, engine)
    };
    let store_ms = store_start.elapsed().as_millis();
    match stored {
        Ok(stored) => {
            out.inserted = stored.inserted;
            out.notified = stored.notified;
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

    // 4. Favicon 缺失回退补全（对齐两处流程：仅在缺失时触发）。
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

    // 5. 标记成功（仅在抓取 + 存库均成功时）并输出单源成功日志。
    {
        let conn = db.lock().await;
        let _ = crate::db::mark_source_fetched(&conn, source_id, true, None);
    }
    // 单源成功日志保持为 INFO（全量）：主机名 + 新增数量 + 分段耗时 + 周期 ID。键名小写，`_ms` 为纯数字。
    let elapsed_ms = total_start.elapsed().as_millis();
    if out.inserted > 0 {
        log::info!("feed refreshed cycle={cycle} id={source_id} host={host} new={} fetch_ms={fetch_ms} store_ms={store_ms} favicon_ms={favicon_ms} elapsed_ms={elapsed_ms}", out.inserted);
    } else {
        log::info!("feed up to date cycle={cycle} id={source_id} host={host} fetch_ms={fetch_ms} store_ms={store_ms} favicon_ms={favicon_ms} elapsed_ms={elapsed_ms}");
    }
    Ok(out)
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
}
