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

// ==================== 拉取管线 Fetch 缝合（candidate 3） ====================
//
// Pull 模块拥有完整拉取管线：stream_map → ids → contents → verdict → upsert → cursor，
// 网络抓取收敛到 `PullFetch` 一个注入点之后：
// - live 适配器 `LivePullFetch`：对 `greader::{subscriptions, stream_ids, contents}`
//   的薄委托（调用形状与历史一致；纯解析 `parse_*` / `normalize_item_id` 不动，
//   ADR-0002“协议层与引擎分离”保持）；
// - fixture 适配器（`#[cfg(test)] FixturePullFetch`，见本文件测试模块）：内存分页 +
//   内存条目，与 live 实现同一 trait——换适配器即换数据源，无需改编排，是真实接缝。
// 政策（行为零变更，ADR-0003 日志字符串逐字保留）：
// - MAX_PAGES 截断 vs 游标推进：见 `collect_pull_ids` 与 `pull_items_with` 的游标注释；
// - 未知 stream_id：见 `UnknownStreamDecision::Skip`（沿用历史的静默 continue）；
// - 逐条落盘链经 P2 Verdict 唯一入口 `verdict_for_remote`：见 `apply_remote_item`。
// 本缝合不碰 `run_with_cycle` / `push_queue` / `with_auth_retry`（推送优先与 LWW 不变），
// 不碰刷新生命周期与 Verdict 推导内部；`db` 侧 SQL 一字未动。

/// 拉取管线的抓取面：订阅列表、增量 ID 分页、条目正文三段抓取（单一注入点）。
///
/// live 与 fixture 为其两种实现（ADR-0002：本地 SQLite 仍是 UI 唯一数据源，
/// 此处只换“远端从哪来”，落盘与裁决路径唯一且不变）。
pub trait PullFetch {
    /// 订阅列表（语义与 `greader::subscriptions` 一致）。
    async fn fetch_subscriptions(&self) -> Result<Vec<greader::GSubscription>, GReaderError>;
    /// reading-list 增量 ID 单页：`ot` 为 `greader.last_sync` 游标或首同步回溯值；
    /// 返回单页 ID 与 continuation，语义与 `greader::stream_ids` 一致。
    async fn fetch_ids_page(
        &self,
        ot: i64,
        continuation: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), GReaderError>;
    /// 按 ID 块取正文（语义与 `greader::contents` 一致；ID 规范化仍在 greader 侧完成，
    /// fixture 直接构造 `GItem` 以绕过线格式——纯解析函数不受影响）。
    async fn fetch_contents(&self, ids: &[String]) -> Result<Vec<greader::GItem>, GReaderError>;
}

/// Live 抓取适配器：`PullFetch` 在生产路径的唯一实现，
/// 对 greader 网络调用的薄委托（调用形状与历史逐字一致）。
pub struct LivePullFetch<'a> {
    pub http: &'a reqwest::Client,
    pub base: &'a str,
    pub auth: &'a str,
}

impl PullFetch for LivePullFetch<'_> {
    async fn fetch_subscriptions(&self) -> Result<Vec<greader::GSubscription>, GReaderError> {
        greader::subscriptions(self.http, self.base, self.auth).await
    }

    async fn fetch_ids_page(
        &self,
        ot: i64,
        continuation: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), GReaderError> {
        greader::stream_ids(
            self.http,
            self.base,
            self.auth,
            greader::STREAM_READING_LIST,
            ot,
            IDS_PAGE_SIZE,
            continuation,
        )
        .await
    }

    async fn fetch_contents(&self, ids: &[String]) -> Result<Vec<greader::GItem>, GReaderError> {
        greader::contents(self.http, self.base, self.auth, ids).await
    }
}

/// 将服务端的订阅源更新或插入到本地分组/订阅源中。本地存在但服务端不存在的订阅源保持不变。
async fn sync_subscriptions(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &str,
) -> Result<usize, GReaderError> {
    // Live 适配器经 `PullFetch` 缝合面取数；纯解析仍在 greader.rs（ADR-0002）。
    let fetch = LivePullFetch { http, base: &acct.server_url, auth };
    let subs = fetch.fetch_subscriptions().await?;
    apply_subscriptions(state, &subs).await
}

/// 订阅对账的落盘段：纯 DB 应用（`find_or_create_group_by_remote` /
/// `get_source_by_remote_id` 调用点语义不变，SQL 未动）。
/// 经 `PullFetch` 注入订阅数据后，live 与 fixture 走同一落盘路径。
async fn apply_subscriptions(
    state: &AppState,
    subs: &[greader::GSubscription],
) -> Result<usize, GReaderError> {
    let count = subs.len();
    {
        let conn = state.db.lock().await;
        for sub in subs {
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

/// 未知 stream_id 的命名决策点（政策显式化，同步结果不变）。
///
/// 背景（ADR-0002 §3）：远端条目按 `origin.streamId` 回填本地订阅源；订阅对账
/// (`sync_subscriptions`) 与拉取是同一周期内先后两步，若某流尚未在本地建源
/// （对账尚未覆盖、远端已删源、或本地库从零重建），`stream_map` 中查无此流。
/// 决策：`Skip`——静默跳过该条目（不写行、不计数、不记日志），与历史的 bare
/// `continue` 逐字同义；刻意不打日志，避免大同步周期刷屏（ADR-0003 噪声预算）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnknownStreamDecision {
    Skip,
}

/// 未知流解析：命中返回本地标识，缺失以 `Err(UnknownStreamDecision::Skip)` 显式表达跳过。
/// 调用方以 `let Ok(..) else { continue }` 应用该决策（语义即历史的静默 continue）。
fn resolve_pull_source<'m>(
    stream_map: &'m std::collections::HashMap<String, SourceIdentity>,
    stream_id: &str,
) -> Result<&'m SourceIdentity, UnknownStreamDecision> {
    stream_map.get(stream_id).ok_or(UnknownStreamDecision::Skip)
}

/// 增量 ID 收集：分页遍历 reading-list 流（不过滤未读：已知条目的状态变更也必须被感知到）。
///
/// 截断政策（与历史逐字一致）：至多 `MAX_PAGES` 页；触发上限且 continuation 仍 pending 时，
/// 超出窗口的 ID 在当前周期跳过——游标仍在 `pull_items_with` 尾部推进到 now
/// （接受项见 ADR-0002 后果：极端窗口漂移；日志字符串保持逐字不变）。
async fn collect_pull_ids<F: PullFetch + Sync>(
    fetch: &F,
    ot: i64,
    cycle: &str,
    host: &str,
) -> Result<Vec<String>, GReaderError> {
    let mut ids: Vec<String> = Vec::new();
    let mut continuation: Option<String> = None;
    let mut pages = 0usize;
    for _ in 0..MAX_PAGES {
        let (page, cont) = fetch.fetch_ids_page(ot, continuation.as_deref()).await?;
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
        log::warn!("sync truncated cycle={cycle} host={host} pages={MAX_PAGES} ids={} continuation=pending", ids.len());
    }
    Ok(ids)
}

/// 单条远端条目的落盘链（P2 Verdict 接口唯一入口，链条与历史逐字一致）：
/// `ammonia::clean` → `snippet_for` → `verdict_for_remote` → `upsert_remote_item`
/// （推导 + 折叠 + 服务端 OR 在 Verdict 模块内一次完成；行级 hidden 单调由
/// `db::upsert_remote_item` 的 SQL CASE 落盘保证，本函数只透传本次 `hidden`）。
///
/// 返回 `Ok(None)` = 未知流 `Skip`（无写行、无计数）；`Ok(Some((inserted, notify)))` =
/// 已落盘（`inserted` 为是否新建行；`notify` 为命中通知规则时的标题拷贝）。
fn apply_remote_item(
    conn: &rusqlite::Connection,
    stream_map: &std::collections::HashMap<String, SourceIdentity>,
    engine: &crate::rules::RuleEngine,
    item: &greader::GItem,
) -> Result<Option<(bool, Option<String>)>, String> {
    let Ok((source_id, group_id, source_url)) = resolve_pull_source(stream_map, &item.stream_id) else {
        return Ok(None);
    };
    let stored_content = ammonia::clean(&item.content);
    let snippet: String = crate::services::article::snippet_for(&stored_content, None);
    // 远端路由经 Verdict 模块一次裁决：合并体原文交模块统一清洗匹配，直接收 Verdict。
    let verdict = crate::services::article::verdict_for_remote(
        Some(engine),
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
    let notify_title = if verdict.notify { Some(item.title.clone()) } else { None };
    let inserted = db::upsert_remote_item(
        conn,
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
    )?;
    Ok(Some((inserted, notify_title)))
}

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
    // Live 适配器经 `PullFetch` 缝合面进入可注入编排核心；行为见 `pull_items_with`。
    let host = sync_host(acct);
    let fetch = LivePullFetch { http, base: &acct.server_url, auth };
    pull_items_with(state, &fetch, cycle, &host).await
}

/// 拉取管线的可注入编排核心：stream_map → ids → contents → verdict → upsert → cursor。
///
/// - 上下文装载（`greader.last_sync` 游标 / `stream_map` / 规则引擎）与历史逐字一致；
///   首同步无游标时回溯 `FIRST_SYNC_WINDOW_SECS`（180 天，ADR-0002 §3）；
/// - ID 与正文两段抓取经 `PullFetch` 注入（live 与 fixture 同一路径）；
/// - 逐条落盘经 `apply_remote_item`（P2 Verdict 唯一入口；未知流 Skip 政策见
///   `resolve_pull_source`，不计数、不记日志）；
/// - 返回（新增行数，匹配通知规则的文章标题列表）。
async fn pull_items_with<F: PullFetch + Sync>(
    state: &AppState,
    fetch: &F,
    cycle: &str,
    host: &str,
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

    let ids = collect_pull_ids(fetch, ot, cycle, host).await?;

    let mut new_count = 0usize;
    let mut notified: Vec<String> = Vec::new();
    // contents 按 CONTENTS_BATCH 分块（请求体大小政策；与历史行为一致）。
    for chunk in ids.chunks(CONTENTS_BATCH) {
        let items = fetch.fetch_contents(chunk).await?;
        let conn = state.db.lock().await;
        for item in items {
            match apply_remote_item(&conn, &stream_map, &engine, &item).map_err(GReaderError::Other)? {
                // 未知流 Skip（政策见 `resolve_pull_source`）：沿用历史的静默 continue，
                // 不计数、不记日志、不改变同步结果。
                None => continue,
                Some((inserted, notify_title)) => {
                    if let Some(title) = notify_title {
                        notified.push(title);
                    }
                    if inserted {
                        new_count += 1;
                    }
                }
            }
        }
    }

    // 游标政策（与历史逐字一致）：无论是否触发 MAX_PAGES 截断、无论本轮拉到与否，
    // `greader.last_sync` 一律推进到 now（截断溢出 ID 本轮跳过；漂移接受项见 ADR-0002 后果）。
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

    // ==================== candidate 3 缝合证明：live vs fixture 双适配器 ====================
    //
    // `PullFetch` 是拉取管线唯一的抓取面；`LivePullFetch`（生产）与
    // `FixturePullFetch`（此处测试）是其两种实现。以下用例证明截断/游标/未知流政策与
    // 逐条落盘链经同一编排路径生效——换适配器不改行为。

    /// Fixture 抓取适配器：内存分页 + 内存条目（与 live 同一 `PullFetch` 面）。
    ///
    /// ID 规范化（十进制/长格式 → hex）是 greader 纯解析 + live 网络路径的职责，
    /// 本适配器直接构造 `GItem`（绕过线格式），因此 fixture 的页 ID 与条目
    /// `remote_id` 用同一 opaque 字符串配对——纯解析函数不受影响。
    struct FixturePullFetch {
        id_pages: Vec<(Vec<String>, Option<String>)>,
        items: Vec<FixtureItem>,
        subs: Vec<FixtureSub>,
        next_page: std::sync::Mutex<usize>,
    }

    struct FixtureItem {
        remote_id: &'static str,
        stream_id: &'static str,
        title: &'static str,
        content: &'static str,
        read: bool,
        starred: bool,
    }

    struct FixtureSub {
        stream_id: &'static str,
        title: &'static str,
        url: Option<&'static str>,
        category_id: Option<&'static str>,
        category: Option<&'static str>,
    }

    impl FixturePullFetch {
        fn empty() -> Self {
            Self {
                id_pages: Vec::new(),
                items: Vec::new(),
                subs: Vec::new(),
                next_page: std::sync::Mutex::new(0),
            }
        }

        fn to_gitem(it: &FixtureItem) -> greader::GItem {
            greader::GItem {
                remote_id: it.remote_id.to_string(),
                stream_id: it.stream_id.to_string(),
                title: it.title.to_string(),
                url: None,
                author: None,
                published_at: 1_700_000_000,
                content: it.content.to_string(),
                read: it.read,
                starred: it.starred,
            }
        }
    }

    impl PullFetch for FixturePullFetch {
        async fn fetch_subscriptions(&self) -> Result<Vec<greader::GSubscription>, GReaderError> {
            Ok(self
                .subs
                .iter()
                .map(|s| greader::GSubscription {
                    stream_id: s.stream_id.to_string(),
                    title: s.title.to_string(),
                    url: s.url.map(str::to_string),
                    category_id: s.category_id.map(str::to_string),
                    category: s.category.map(str::to_string),
                })
                .collect())
        }

        async fn fetch_ids_page(
            &self,
            _ot: i64,
            _continuation: Option<&str>,
        ) -> Result<(Vec<String>, Option<String>), GReaderError> {
            // 按调用序依次返回预置页（耗尽后返回空终页）；分页停止条件（None/空页）
            // 与生产路径走同一套 `collect_pull_ids` 判定。
            let mut n = self.next_page.lock().expect("fixture page lock");
            let idx = *n;
            *n += 1;
            if idx < self.id_pages.len() {
                Ok(self.id_pages[idx].clone())
            } else {
                Ok((Vec::new(), None))
            }
        }

        async fn fetch_contents(&self, ids: &[String]) -> Result<Vec<greader::GItem>, GReaderError> {
            Ok(self
                .items
                .iter()
                .filter(|it| ids.iter().any(|id| id == it.remote_id))
                .map(Self::to_gitem)
                .collect())
        }
    }

    fn assert_is_pull_fetch<F: PullFetch + Sync>(_: &F) {}

    fn mem_state() -> crate::AppState {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        crate::AppState {
            db: tokio::sync::Mutex::new(conn),
            db_path: std::path::PathBuf::from(":memory:"),
            http: std::sync::RwLock::new(reqwest::Client::new()),
            sync_token: std::sync::RwLock::new(None),
        }
    }

    #[test]
    fn pull_seam_has_live_and_fixture_adapters() {
        // 设定：live（薄委托，本用例不发请求）与 fixture（内存）两种适配器
        let client = reqwest::Client::new();
        let live = LivePullFetch { http: &client, base: "https://sync.example", auth: "t" };
        let fixture = FixturePullFetch::empty();
        // 操作：同一泛型约束接纳两者（编译期即证明单缝合面）
        assert_is_pull_fetch(&live);
        assert_is_pull_fetch(&fixture);
        // 验证：空 fixture 抓取面为空（终页语义）
        let (ids, cont) = tauri::async_runtime::block_on(fixture.fetch_ids_page(0, None)).unwrap();
        assert!(ids.is_empty() && cont.is_none());
    }

    #[test]
    fn pull_collect_truncates_at_max_pages() {
        // 设定：页页 pending（MAX_PAGES + 3 页，每页 1 id，continuation 永不为 None）
        let pages: Vec<(Vec<String>, Option<String>)> = (0..MAX_PAGES + 3)
            .map(|i| (vec![format!("id-{i}")], Some("more".to_string())))
            .collect();
        let fixture = FixturePullFetch {
            id_pages: pages,
            items: Vec::new(),
            subs: Vec::new(),
            next_page: std::sync::Mutex::new(0),
        };
        // 操作：经同一 `collect_pull_ids` 收集
        let ids =
            tauri::async_runtime::block_on(collect_pull_ids(&fixture, 123, "c-trunc", "h")).unwrap();
        // 验证：恰好在 MAX_PAGES 处截断（政策与历史一致；截断 warn 字符串不变）
        assert_eq!(ids.len(), MAX_PAGES);
    }

    #[test]
    fn pull_apply_skips_unknown_stream_and_keeps_chain() {
        // 设定：内存库 + 已知源 + 空引擎（quiet）
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn).unwrap();
        let s = crate::db::insert_source(&conn, "https://s.example/feed", "S", None, None).unwrap();
        crate::db::set_source_remote(&conn, s.id, Some("feed/known")).unwrap();
        let engine = crate::rules::RuleEngine::load(&conn).unwrap();
        let map: std::collections::HashMap<String, SourceIdentity> = [(
            "feed/known".to_string(),
            (s.id, None, "https://s.example/feed".to_string()),
        )]
        .into_iter()
        .collect();
        // 操作：未知流条目
        let ghost = greader::GItem {
            remote_id: "g1".into(),
            stream_id: "feed/ghost".into(),
            title: "Ghost".into(),
            url: None,
            author: None,
            published_at: 1,
            content: "<p>x</p>".into(),
            read: false,
            starred: false,
        };
        // 验证：Skip 决策（Ok(None)，无写行、无计数）
        assert_eq!(apply_remote_item(&conn, &map, &engine, &ghost).unwrap(), None);
        assert_eq!(crate::db::item_count(&conn).unwrap(), 0);
        // 操作：已知流条目（服务端未读叠加 quiet 引擎）
        let known = greader::GItem {
            remote_id: "k1".into(),
            stream_id: "feed/known".into(),
            title: "Known".into(),
            url: None,
            author: None,
            published_at: 1,
            content: "<p>hello</p>".into(),
            read: false,
            starred: false,
        };
        // 验证：落盘链完整（新建行，无通知）
        assert_eq!(
            apply_remote_item(&conn, &map, &engine, &known).unwrap(),
            Some((true, None))
        );
        assert_eq!(crate::db::item_count(&conn).unwrap(), 1);
        // 操作：同一远端重放（更新路径，不新建）
        assert_eq!(
            apply_remote_item(&conn, &map, &engine, &known).unwrap(),
            Some((false, None))
        );
    }

    #[test]
    fn pull_end_to_end_fixture_counts_notified_and_cursor() {
        // 设定：内存 AppState + 已知源 + Notify 规则（命中 c1）
        let state = mem_state();
        tauri::async_runtime::block_on(async {
            let conn = state.db.lock().await;
            let src =
                crate::db::insert_source(&conn, "https://s.example/feed", "S", None, None).unwrap();
            crate::db::set_source_remote(&conn, src.id, Some("feed/known")).unwrap();
            crate::db::create_rule(
                &conn,
                &RuleInput {
                    name: "n".into(),
                    pattern: "ping-notify-xyz".into(),
                    target_field: RuleTargetField::Content,
                    action_type: RuleActionType::Notify,
                    is_case_sensitive: false,
                    is_enabled: true,
                    source_scope: RuleSourceScope::All,
                },
            )
            .unwrap();
        });
        // 设定：fixture 单页 3 id → 已知朴素 / 未知流 / 已知命中通知
        let fixture = FixturePullFetch {
            id_pages: vec![(vec!["a1".to_string(), "b1".to_string(), "c1".to_string()], None)],
            items: vec![
                FixtureItem {
                    remote_id: "a1",
                    stream_id: "feed/known",
                    title: "Plain",
                    content: "<p>plain</p>",
                    read: false,
                    starred: false,
                },
                FixtureItem {
                    remote_id: "b1",
                    stream_id: "feed/ghost",
                    title: "Ghost",
                    content: "<p>ghost</p>",
                    read: false,
                    starred: false,
                },
                FixtureItem {
                    remote_id: "c1",
                    stream_id: "feed/known",
                    title: "Ping",
                    content: "<p>ping-notify-xyz here</p>",
                    read: false,
                    starred: false,
                },
            ],
            subs: Vec::new(),
            next_page: std::sync::Mutex::new(0),
        };
        // 操作：经同一 `pull_items_with` 全链路
        let (new_count, notified) =
            tauri::async_runtime::block_on(pull_items_with(&state, &fixture, "c-e2e", "h-e2e"))
                .unwrap();
        // 验证：未知流跳过不计数；两已知新建；通知仅命中 c1
        assert_eq!(new_count, 2);
        assert_eq!(notified, vec!["Ping".to_string()]);
        // 验证：游标已推进（政策：无论拉到与否一律到 now）
        let cursor: i64 = tauri::async_runtime::block_on(async {
            let conn = state.db.lock().await;
            crate::db::get_state(&conn, "greader.last_sync")
                .unwrap()
                .unwrap()
                .parse()
                .unwrap()
        });
        assert!(cursor >= crate::models::now_ts() - 5);
    }

    #[test]
    fn subs_apply_maps_groups_and_sources_through_seam() {
        // 设定：空库 AppState + fixture 两订阅（一带分组，一无分组）
        let state = mem_state();
        let fixture = FixturePullFetch {
            id_pages: Vec::new(),
            items: Vec::new(),
            subs: vec![
                FixtureSub {
                    stream_id: "feed/s1",
                    title: "Tech News",
                    url: Some("https://s1.example/feed"),
                    category_id: Some("user/-/label/Tech"),
                    category: Some("Tech"),
                },
                FixtureSub {
                    stream_id: "feed/s2",
                    title: "Plain",
                    url: Some("https://s2.example/rss"),
                    category_id: None,
                    category: None,
                },
            ],
            next_page: std::sync::Mutex::new(0),
        };
        // 操作：fixture 取数 → 同一 `apply_subscriptions` 落盘
        let subs = tauri::async_runtime::block_on(fixture.fetch_subscriptions()).unwrap();
        let n = tauri::async_runtime::block_on(apply_subscriptions(&state, &subs)).unwrap();
        // 验证：计数与 DB 行（分组映射 + 远端对齐）
        assert_eq!(n, 2);
        tauri::async_runtime::block_on(async {
            let conn = state.db.lock().await;
            let s1 = crate::db::get_source_by_remote_id(&conn, "feed/s1").unwrap().unwrap();
            assert_eq!(s1.title, "Tech News");
            assert!(s1.group_id.is_some());
            let s2 = crate::db::get_source_by_remote_id(&conn, "feed/s2").unwrap().unwrap();
            assert_eq!(s2.title, "Plain");
        });
    }
}
