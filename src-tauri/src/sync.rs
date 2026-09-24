//! 面向兼容 Google Reader 服务器的云同步引擎。
//!
//! 每次运行流程：优先推送本地操作队列（以便后续拉取能感知到自身变更），
//! 对齐订阅源列表，然后增量拉取变更条目，
//! 服务端的已读/加星状态优先生效（以最后写入为准，LWW）。

use crate::db;
use crate::greader::{self, GReaderError};
use crate::models::{html_to_text, SyncAccount, SyncAction};
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

/// 错误消息的首行（截断后）：错误原因绝不包含令牌、请求体或完整 URL（服务端文本留在服务端，令牌仅保留在内存中）。
fn short_reason(msg: &str) -> String {
    const MAX_CHARS: usize = 160;
    let first = msg.lines().next().unwrap_or("").trim();
    if first.chars().count() > MAX_CHARS {
        first.chars().take(MAX_CHARS).collect()
    } else {
        first.to_string()
    }
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

/// 渲染排序后的 ` fail_kinds={timeout:2,auth:1}`；无失败时为空字符串。
fn format_fail_kinds(report: &SyncReport) -> String {
    if report.fail_kinds.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(&String, &usize)> = report.fail_kinds.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let inner: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}:{v}")).collect();
    format!(" fail_kinds={{{}}}", inner.join(","))
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
pub async fn ensure_session(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
) -> Result<String, GReaderError> {
    ensure_session_with(state, http, acct, "").await
}

/// 与 [`ensure_session`] 相同，但在日志中标记 `cycle=`（为空时省略）。
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
            log::warn!("sync login failed{cycle_disp} host={host} error_kind={error_kind} reason {}", short_reason(&e.to_string()));
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

/// 运行一次完整的同步周期：
/// 1. 将本地队列中的操作推送到服务端（优先推送队列）
/// 2. 同步订阅列表（创建/更新订阅源与分组）
/// 3. 增量拉取新增/更新条目
pub async fn run(app: &AppHandle, _background: bool) -> Result<SyncReport, String> {
    let cycle = crate::net::new_cycle();
    run_with_cycle(app, _background, &cycle).await
}

/// 与 [`run`] 相同，但接收来自刷新调用方的显式周期 ID（由 lib.rs 生成，使刷新开始与完成共享同一个 `cycle=`）。
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

    macro_rules! retry_auth {
        ($op_name:expr, $expr:expr) => {
            match $expr {
                Ok(val) => Some(val),
                Err(GReaderError::Auth(_)) => match relogin(&state, &http, &acct, cycle).await {
                    Ok(new_auth) => {
                        auth = new_auth;
                        match $expr {
                            Ok(val) => Some(val),
                            Err(e) => {
                                let error_kind = greader_err_kind(&e);
                                report.failures += 1;
                                record_fail(&mut report, error_kind);
                                log::warn!("{} failed cycle={cycle} host={host} error_kind={error_kind} reason {}", $op_name, short_reason(&e.to_string()));
                                None
                            }
                        }
                    }
                    Err(e) => {
                        let error_kind = greader_err_kind(&e);
                        report.failures += 1;
                        record_fail(&mut report, error_kind);
                        log::warn!("{} failed cycle={cycle} host={host} error_kind={error_kind} reason {}", $op_name, short_reason(&e.to_string()));
                        None
                    }
                },
                Err(e) => {
                    let error_kind = greader_err_kind(&e);
                    report.failures += 1;
                    record_fail(&mut report, error_kind);
                    log::warn!("{} failed cycle={cycle} host={host} error_kind={error_kind} reason {}", $op_name, short_reason(&e.to_string()));
                    None
                }
            }
        };
    }

    // 1. 推送队列中的本地操作
    if let Some(n) = retry_auth!("sync push", push_queue(&state, &http, &acct, &auth).await) {
        report.pushed = n;
    }

    // 2. 同步订阅源列表
    if let Some(n) = retry_auth!("sync subscriptions", sync_subscriptions(&state, &http, &acct, &auth).await) {
        report.subscription_count = n;
    }

    // 3. 增量拉取条目（内联重试认证，使最终失败原因可记录在警告日志中）。
    match pull_items(&state, &http, &acct, &auth, cycle).await {
        Ok((n, notified)) => {
            report.new_items = n;
            report.notified = notified;
        }
        Err(GReaderError::Auth(_)) => match relogin(&state, &http, &acct, cycle).await {
            Ok(new_auth) => {
                auth = new_auth;
                match pull_items(&state, &http, &acct, &auth, cycle).await {
                    Ok((n, notified)) => {
                        report.new_items = n;
                        report.notified = notified;
                    }
                    Err(e) => {
                        let error_kind = greader_err_kind(&e);
                        report.failures += 1;
                        record_fail(&mut report, error_kind);
                        log::warn!("sync pull failed cycle={cycle} host={host} error_kind={error_kind} reason {}", short_reason(&e.to_string()));
                    }
                }
            }
            Err(e) => {
                let error_kind = greader_err_kind(&e);
                report.failures += 1;
                record_fail(&mut report, error_kind);
                log::warn!("sync pull failed cycle={cycle} host={host} error_kind={error_kind} reason {}", short_reason(&e.to_string()));
            }
        },
        Err(e) => {
            let error_kind = greader_err_kind(&e);
            report.failures += 1;
            record_fail(&mut report, error_kind);
            log::warn!("sync pull failed cycle={cycle} host={host} error_kind={error_kind} reason {}", short_reason(&e.to_string()));
        }
    }

    // 单行汇总同步摘要：在单行中输出推送 + 订阅 + 拉取数量（不输出分阶段详情，通知仅输出数量，仅在存在失败时细分失败类型）。
    let fail_suffix = format_fail_kinds(&report);
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

/// 将服务端状态与本地规则执行结果合并：服务端已读/加星状态优先（LWW），规则标记在其基础上叠加。返回 (has_been_read, starred, hidden)。
fn apply_rules_to_remote(
    server_read: bool,
    server_starred: bool,
    outcome: &crate::rules::Outcome,
) -> (bool, bool, bool) {
    (
        server_read || outcome.mark_read || outcome.hide,
        server_starred || outcome.star,
        outcome.hide,
    )
}

/// 拉取自上次同步游标以来发生变更的条目，以服务端的已读/加星状态进行更新或插入，并像本地刷新路径一样通过规则引擎处理新条目（标记已读 / 加星 / 隐藏 / 通知）。
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
            let content = ammonia::clean(&item.content);
            let snippet: String = html_to_text(&content).trim().chars().take(200).collect();
            let (has_been_read, starred, hidden) = if engine.is_empty() {
                (item.read, item.starred, false)
            } else {
                let content_text = html_to_text(&content);
                let ctx = crate::rules::ArticleEvalContext {
                    source_id: *source_id,
                    group_id: *group_id,
                    source_url,
                    title: &item.title,
                    content_text: &content_text,
                    author: item.author.as_deref(),
                    url: item.url.as_deref(),
                };
                let outcome = engine.evaluate(&ctx);
                let flags = apply_rules_to_remote(item.read, item.starred, &outcome);
                if outcome.notify {
                    notified.push(item.title.clone());
                }
                flags
            };
            let inserted = db::upsert_remote_item(
                &conn,
                &db::RemoteItemUpsert {
                    remote_id: &item.remote_id,
                    source_id: *source_id,
                    title: &item.title,
                    url: item.url.as_deref(),
                    author: item.author.as_deref(),
                    published_at: item.published_at,
                    content: Some(&content),
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

    #[test]
    fn rule_flags_apply_on_top_of_server_state() {
        // 设定：服务端表示未读/未加星，规则指定 标记已读 + 加星 + 隐藏 + 通知
        let outcome = crate::rules::Outcome { mark_read: true, star: true, hide: true, notify: true };
        // 操作：合并
        let flags = apply_rules_to_remote(false, false, &outcome);
        // 验证：每项规则标记均生效
        assert_eq!(flags, (true, true, true));

        // 设定：服务端已为已读/已加星，无规则匹配
        let quiet = crate::rules::Outcome::default();
        // 操作：合并
        let flags = apply_rules_to_remote(true, true, &quiet);
        // 验证：服务端状态原样保留
        assert_eq!(flags, (true, true, false));

        // 设定：服务端为未读，仅匹配通知规则（无状态修改）
        let notify_only = crate::rules::Outcome { notify: true, ..Default::default() };
        // 操作：合并
        let flags = apply_rules_to_remote(false, false, &notify_only);
        // 验证：行状态保持不变
        assert_eq!(flags, (false, false, false));
    }
}
