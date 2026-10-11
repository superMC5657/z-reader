//! 面向兼容 Google Reader 服务器的云同步引擎。
//!
//! 每次运行流程：优先推送本地操作队列（以便后续拉取能感知到自身变更），
//! 对齐订阅源列表，然后增量拉取变更条目，
//! 服务端的已读/加星状态优先生效（以最后写入为准，LWW）。

use crate::db;
use crate::greader::{self, GReaderError};
use crate::models::{SyncAccount, SyncAction};
use crate::AppState;
use tauri::{AppHandle, Manager};

const MAX_PAGES: usize = 20;
const IDS_PAGE_SIZE: u32 = 1000;
/// 不存在游标时的首次同步回溯窗口（180 天）。
const FIRST_SYNC_WINDOW_SECS: i64 = 180 * 86_400;
const CONTENTS_BATCH: usize = 100;
/// edit-tag 批次大小（设限以保证 POST 请求体大小合理）。
const PUSH_BATCH: usize = 100;

#[derive(Default, Debug)]
pub struct SyncReport {
    pub new_items: usize,
    pub pushed: usize,
    pub failures: usize,
    pub subscription_count: usize,
    /// 匹配到“通知”规则的拉取文章标题列表。
    pub notified: Vec<String>,
    /// 汇总的失败类别，用于 `fail_kinds={...}` 摘要输出。
    pub fail_kinds: std::collections::HashMap<String, usize>,
}

/// 缓存的登录会话；绝不持久化到磁盘。
#[derive(Clone)]
pub struct Session {
    pub base: String,
    pub username: String,
    pub auth: String,
}

/// 将 greader 错误映射到稳定的 `error_kind`（认证错误特殊处理，其他消息通过通用的网络分类器处理）。
fn greader_err_kind(e: &GReaderError) -> &'static str {
    match e {
        GReaderError::Auth(_) => "auth",
        GReaderError::Other(m) => crate::net::classify_error(m),
    }
}

/// 用于同步日志的净化主机名（绝不输出原始 URL/查询参数）。
fn sync_host(acct: &SyncAccount) -> String {
    crate::net::sanitize_url(&acct.server_url).0
}

fn record_fail(report: &mut SyncReport, kind: &str) {
    *report.fail_kinds.entry(kind.to_string()).or_insert(0) += 1;
}

pub fn clear_session(state: &AppState) {
    *state.sync_token.write().expect("sync token lock") = None;
}

fn store_session(state: &AppState, acct: &SyncAccount, auth: &str) {
    *state.sync_token.write().expect("sync token lock") = Some(Session {
        base: acct.server_url.clone(),
        username: acct.username.clone(),
        auth: auth.to_string(),
    });
}

/// 当缓存的 Auth 令牌与当前账户匹配时返回该令牌，否则执行登录。
/// 日志中标记 `cycle=`（为空时省略）。
pub async fn ensure_session_with(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    cycle: &str,
) -> Result<String, GReaderError> {
    // 限制守卫的作用域：标准库的 RwLockReadGuard 不是 Send，绝不能跨越下方的登录 await 持有。
    let cached = {
        let guard = state.sync_token.read().expect("sync token lock");
        guard
            .as_ref()
            .filter(|s| s.base == acct.server_url && s.username == acct.username)
            .map(|s| s.auth.clone())
    };
    if let Some(auth) = cached {
        return Ok(auth);
    }
    let host = sync_host(acct);
    let cycle_disp = if cycle.is_empty() { String::new() } else { format!(" cycle={cycle}") };
    let auth = match greader::login(http, &acct.server_url, &acct.username, &acct.password).await
    {
        Ok(a) => a,
        Err(e) => {
            let error_kind = greader_err_kind(&e);
            log::warn!("sync login failed{cycle_disp} host={host} error_kind={error_kind} reason {}", crate::utils::short_reason(&e.to_string()));
            return Err(e);
        }
    };
    store_session(state, acct, &auth);
    log::debug!("sync login ok{cycle_disp} provider=greader host={host}");
    Ok(auth)
}

async fn relogin(state: &AppState, http: &reqwest::Client, acct: &SyncAccount, cycle: &str) -> Result<String, GReaderError> {
    clear_session(state);
    ensure_session_with(state, http, acct, cycle).await
}

/// 同步失败记账 + warn 日志（原因经 `utils::short_reason` 收敛为单行）。
fn log_sync_fail(report: &mut SyncReport, op_name: &str, cycle: &str, host: &str, e: &GReaderError) {
    let error_kind = greader_err_kind(e);
    report.failures += 1;
    record_fail(report, error_kind);
    log::warn!(
        "{op_name} failed cycle={cycle} host={host} error_kind={error_kind} reason {}",
        crate::utils::short_reason(&e.to_string())
    );
}

/// 统一鉴权重试：执行 `op` 一次，401/403（`GReaderError::Auth`）时重登录一次再试，语义与旧 `retry_auth!` 宏一致。
/// 成功返回 `Some(val)`；失败时经 [`log_sync_fail`] 记账并返回 `None`。
#[allow(clippy::too_many_arguments)]
async fn with_auth_retry<T, F, Fut>(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &mut String,
    cycle: &str,
    host: &str,
    op_name: &str,
    report: &mut SyncReport,
    op: F,
) -> Option<T>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<T, GReaderError>>,
{
    match op(auth.clone()).await {
        Ok(val) => Some(val),
        Err(GReaderError::Auth(_)) => match relogin(state, http, acct, cycle).await {
            Ok(new_auth) => {
                *auth = new_auth;
                match op(auth.clone()).await {
                    Ok(val) => Some(val),
                    Err(e) => {
                        log_sync_fail(report, op_name, cycle, host, &e);
                        None
                    }
                }
            }
            Err(e) => {
                log_sync_fail(report, op_name, cycle, host, &e);
                None
            }
        },
        Err(e) => {
            log_sync_fail(report, op_name, cycle, host, &e);
            None
        }
    }
}

/// 运行一次完整的同步周期（调用方传入显式周期 ID，由 lib.rs 生成，使刷新开始与完成共享同一个 `cycle=`；
/// 手动同步时调用方现场生成 `new_cycle()`）：
/// 1. 将本地队列中的操作推送到服务端（优先推送队列）
/// 2. 同步订阅列表（创建/更新订阅源与分组）
/// 3. 增量拉取新增/更新条目
pub async fn run_with_cycle(app: &AppHandle, _background: bool, cycle: &str) -> Result<SyncReport, String> {
    let settings = crate::settings::load(&crate::settings::settings_path(app)?);
    let acct = settings
        .sync_account
        .filter(|a| a.provider == "greader")
        .ok_or("no Google Reader account configured")?;
    let state = app.state::<AppState>();
    let http = state.http_client();
    let host = sync_host(&acct);
    let mut auth = ensure_session_with(&state, &http, &acct, cycle).await.map_err(|e| e.to_string())?;

    let mut report = SyncReport::default();

    // 1. 推送队列中的本地操作（优先推送，以便后续拉取感知自身变更）
    if let Some(n) = with_auth_retry(
        &state,
        &http,
        &acct,
        &mut auth,
        cycle,
        &host,
        "sync push",
        &mut report,
        |auth_owned: String| {
            let s = state.clone();
            let h = http.clone();
            let a = acct.clone();
            async move { push_queue(&s, &h, &a, &auth_owned).await }
        },
    )
    .await
    {
        report.pushed = n;
    }

    // 2. 同步订阅源列表
    if let Some(n) = with_auth_retry(
        &state,
        &http,
        &acct,
        &mut auth,
        cycle,
        &host,
        "sync subscriptions",
        &mut report,
        |auth_owned: String| {
            let s = state.clone();
            let h = http.clone();
            let a = acct.clone();
            async move { sync_subscriptions(&s, &h, &a, &auth_owned).await }
        },
    )
    .await
    {
        report.subscription_count = n;
    }

    // 3. 增量拉取条目
    if let Some((n, notified)) = with_auth_retry(
        &state,
        &http,
        &acct,
        &mut auth,
        cycle,
        &host,
        "sync pull",
        &mut report,
        |auth_owned: String| {
            let s = state.clone();
            let h = http.clone();
            let a = acct.clone();
            let c = cycle;
            async move { pull_items(&s, &h, &a, &auth_owned, c).await }
        },
    )
    .await
    {
        report.new_items = n;
        report.notified = notified;
    }

    // 单行汇总同步摘要：在单行中输出推送 + 订阅 + 拉取数量（不输出分阶段详情，通知仅输出数量，仅在存在失败时细分失败类型）。
    let fail_suffix = crate::utils::format_fail_kinds(&report.fail_kinds);
    log::info!(
        "sync done cycle={cycle} host={host} pushed={} subs={} new={} failures={}{} notified={}",
        report.pushed,
        report.subscription_count,
        report.new_items,
        report.failures,
        fail_suffix,
        report.notified.len()
    );
    Ok(report)
}

/// 将本地操作队列消耗并推送到服务端。推送成功的条目将被删除；途中发生失败时剩余条目保留在队列中（重复推送已应用的修改是幂等的）。
async fn push_queue(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &str,
) -> Result<usize, GReaderError> {
    let entries = {
        let conn = state.db.lock().await;
        db::queue_fetch(&conn, 5000).map_err(GReaderError::Other)?
    };
    if entries.is_empty() {
        return Ok(0);
    }
    let token = greader::get_token(http, &acct.server_url, auth).await?;

    use std::collections::HashMap;
    let mut by_action: HashMap<SyncAction, Vec<(i64, String)>> = HashMap::new();
    let mut stream_actions: Vec<(i64, String)> = Vec::new();
    for e in entries {
        if e.action == SyncAction::MarkAllRead {
            stream_actions.push((e.id, e.target));
        } else {
            by_action.entry(e.action).or_default().push((e.id, e.target));
        }
    }

    let mut pushed_ids: Vec<i64> = Vec::new();
    for (action, items) in &by_action {
        let (add, remove): (&[&str], &[&str]) = match action {
            SyncAction::MarkRead => (&[greader::STATE_READ], &[]),
            SyncAction::MarkUnread => (&[], &[greader::STATE_READ]),
            SyncAction::Star => (&[greader::STATE_STARRED], &[]),
            SyncAction::Unstar => (&[], &[greader::STATE_STARRED]),
            SyncAction::MarkAllRead => continue,
        };
        for chunk in items.chunks(PUSH_BATCH) {
            let ids: Vec<String> = chunk.iter().map(|(_, t)| t.clone()).collect();
            greader::edit_tag(http, &acct.server_url, auth, &token, &ids, add, remove).await?;
            pushed_ids.extend(chunk.iter().map(|(id, _)| *id));
        }
    }
    for (id, stream) in &stream_actions {
        greader::edit_tag_stream(http, &acct.server_url, auth, &token, stream, greader::STATE_READ)
            .await?;
        pushed_ids.push(*id);
    }

    {
        let conn = state.db.lock().await;
        db::queue_delete(&conn, &pushed_ids).map_err(GReaderError::Other)?;
    }
    Ok(pushed_ids.len())
}

/// 将服务端的订阅源更新或插入到本地分组/订阅源中。本地存在但服务端不存在的订阅源保持不变。
async fn sync_subscriptions(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &str,
) -> Result<usize, GReaderError> {
    let subs = greader::subscriptions(http, &acct.server_url, auth).await?;
    let count = subs.len();
    {
        let conn = state.db.lock().await;
        for sub in &subs {
            let group_id = match (&sub.category_id, &sub.category) {
                (Some(cid), Some(label)) => Some(
                    db::find_or_create_group_by_remote(&conn, cid, label)
                        .map_err(GReaderError::Other)?
                        .id,
                ),
                (None, Some(label)) => Some(
                    db::find_or_create_group(&conn, label)
                        .map_err(GReaderError::Other)?
                        .id,
                ),
                _ => None,
            };
            let url = sub
                .url
                .clone()
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| format!("greader:{}", sub.stream_id));
            let existing = match db::get_source_by_remote_id(&conn, &sub.stream_id)
                .map_err(GReaderError::Other)?
            {
                Some(s) => Some(s),
                None => db::get_source_by_url(&conn, &url)
                    .map_err(GReaderError::Other)?,
            };
            match existing {
                Some(s) => {
                    let _ = db::set_source_remote(&conn, s.id, Some(&sub.stream_id));
                    let _ = db::rename_source(&conn, s.id, &sub.title);
                    if let Some(gid) = group_id {
                        let _ = db::set_source_group(&conn, s.id, Some(gid));
                    }
                }
                None => {
                    let s = db::insert_source(&conn, &url, &sub.title, None, group_id)
                        .map_err(GReaderError::Other)?;
                    db::set_source_remote(&conn, s.id, Some(&sub.stream_id))
                        .map_err(GReaderError::Other)?;
                }
            }
        }
    }
    Ok(count)
}

/// 同步订阅源的本地标识：(source_id, group_id, source_url)。
type SourceIdentity = (i64, Option<i64>, String);

/// 拉取自上次同步游标以来发生变更的条目，以服务端的已读/加星状态进行更新或插入，并像本地刷新路径一样通过规则引擎处理新条目（标记已读 / 加星 / 隐藏 / 通知）。
/// 远端裁决唯一入口为 `services::article::verdict_for_remote`（推导 + 折叠 + 服务端 OR 一次完成；
/// 行级 hidden 单调由 `db::upsert_remote_item` 落盘保证）。
/// 返回（新增行数，匹配通知规则的文章标题列表）。
async fn pull_items(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &str,
    cycle: &str,
) -> Result<(usize, Vec<String>), GReaderError> {
    let (ot, stream_map): (i64, std::collections::HashMap<String, SourceIdentity>) = {
        let conn = state.db.lock().await;
        let ot = db::get_state(&conn, "greader.last_sync")
            .map_err(GReaderError::Other)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let ot = if ot > 0 { ot } else { crate::models::now_ts() - FIRST_SYNC_WINDOW_SECS };
        let map = db::get_sources(&conn)
            .map_err(GReaderError::Other)?
            .into_iter()
            .filter_map(|s| {
                s.remote_id.map(|r| {
                    let ctx = (s.id, s.group_id, s.url.clone());
                    (r, ctx)
                })
            })
            .collect();
        (ot, map)
    };
    let engine = {
        let conn = state.db.lock().await;
        crate::rules::RuleEngine::load(&conn).map_err(GReaderError::Other)?
    };

    // 分页遍历 reading-list 流（不过滤未读：已知条目的状态变更也必须被感知到）。
    let mut ids: Vec<String> = Vec::new();
    let mut continuation: Option<String> = None;
    let mut pages = 0usize;
    for _ in 0..MAX_PAGES {
        let (page, cont) = greader::stream_ids(
            http,
            &acct.server_url,
            auth,
            greader::STREAM_READING_LIST,
            ot,
            IDS_PAGE_SIZE,
            continuation.as_deref(),
        )
        .await?;
        let page_len = page.len();
        ids.extend(page);
        continuation = cont;
        pages += 1;
        if continuation.is_none() || page_len == 0 {
            break;
        }
    }
    // 触发 MAX_PAGES 限制且仍有未拉取的分页：超出窗口的 ID 将在当前周期中跳过（游标在下方仍会推进）。
    if continuation.is_some() && pages >= MAX_PAGES {
        let host = sync_host(acct);
        log::warn!("sync truncated cycle={cycle} host={host} pages={MAX_PAGES} ids={} continuation=pending", ids.len());
    }

    let mut new_count = 0usize;
    let mut notified: Vec<String> = Vec::new();
    for chunk in ids.chunks(CONTENTS_BATCH) {
        let items = greader::contents(http, &acct.server_url, auth, chunk).await?;
        let conn = state.db.lock().await;
        for item in items {
            let Some((source_id, group_id, source_url)) = stream_map.get(&item.stream_id) else {
                continue; // 订阅源尚未在本地关联
            };
            let stored_content = ammonia::clean(&item.content);
            let snippet: String = crate::services::article::snippet_for(&stored_content, None);
            // 远端路由经 Verdict 模块一次裁决：合并体原文交模块统一清洗匹配，直接收 Verdict。
            let verdict = crate::services::article::verdict_for_remote(
                Some(&engine),
                *source_id,
                *group_id,
                source_url,
                &item.title,
                &item.content,
                item.author.as_deref(),
                item.url.as_deref(),
                item.read,
                item.starred,
            );
            let (has_been_read, starred, hidden) = (verdict.has_been_read, verdict.starred, verdict.hidden);
            if verdict.notify {
                notified.push(item.title.clone());
            }
            let inserted = db::upsert_remote_item(
                &conn,
                &db::RemoteItemUpsert {
                    remote_id: &item.remote_id,
                    source_id: *source_id,
                    title: &item.title,
                    url: item.url.as_deref(),
                    author: item.author.as_deref(),
                    published_at: item.published_at,
                    content: Some(&stored_content),
                    summary: None,
                    snippet: Some(&snippet),
                    has_been_read,
                    starred,
                    hidden,
                },
            )
            .map_err(GReaderError::Other)?;
            if inserted {
                new_count += 1;
            }
        }
    }

    {
        let conn = state.db.lock().await;
        db::set_state(&conn, "greader.last_sync", &crate::models::now_ts().to_string())
            .map_err(GReaderError::Other)?;
    }
    Ok((new_count, notified))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RuleActionType, RuleInput, RuleSourceScope, RuleTargetField};

    fn mem_engine(
        pattern: &str,
        target: RuleTargetField,
        action: RuleActionType,
    ) -> (rusqlite::Connection, crate::rules::RuleEngine) {
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
        let engine = crate::rules::RuleEngine::load(&conn).unwrap();
        (conn, engine)
    }

    #[test]
    fn rule_flags_apply_on_top_of_server_state() {
        // 远端裁决唯一入口 `verdict_for_remote` 的行状态语义（不再经 Outcome 薄委托）：
        // 规则标记在服务端 LWW 之上叠加，hide 唯一折叠为已读，notify 不改行。
        // 设定：hide 规则命中，服务端为未读/未加星
        let (_c, hide_engine) = mem_engine("hide-me-xyz", RuleTargetField::Content, RuleActionType::Hide);
        // 操作：远端路由一次裁决
        let v = crate::services::article::verdict_for_remote(
            Some(&hide_engine), 1, None, "https://s.example", "t", "hide-me-xyz body", None, None, false, false,
        );
        // 验证：隐藏与已读同时置位
        assert!(v.hidden && v.has_been_read);

        // 设定：服务端已为已读/已加星，quiet 规则（空引擎）
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let quiet = crate::rules::RuleEngine::load(&conn).unwrap();
        // 操作：远端路由一次裁决
        let v = crate::services::article::verdict_for_remote(
            Some(&quiet), 1, None, "https://s.example", "t", "plain", None, None, true, true,
        );
        // 验证：服务端状态原样保留
        assert!(v.has_been_read && v.starred && !v.hidden && !v.notify);

        // 设定：服务端为未读，仅匹配通知规则（无状态修改）
        let (_c3, notify_engine) = mem_engine("notify-me-xyz", RuleTargetField::Content, RuleActionType::Notify);
        // 操作：远端路由一次裁决
        let v = crate::services::article::verdict_for_remote(
            Some(&notify_engine), 1, None, "https://s.example", "t", "notify-me-xyz body", None, None, false, false,
        );
        // 验证：仅通知，行状态保持不变
        assert!(v.notify && !v.has_been_read && !v.hidden && !v.starred);
    }
}
