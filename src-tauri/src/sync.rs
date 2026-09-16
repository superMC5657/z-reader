//! Cloud sync engine for Google Reader compatible servers.
//!
//! Per run: push the local action queue first (so later pulls observe our own
//! changes), reconcile subscriptions, then incrementally pull changed items
//! with the server's read/starred state winning (last-write-wins).

use crate::db;
use crate::greader::{self, GReaderError};
use crate::models::{html_to_text, SyncAccount, SyncAction};
use crate::AppState;
use tauri::{AppHandle, Manager};

const MAX_PAGES: usize = 20;
const IDS_PAGE_SIZE: u32 = 1000;
/// First-sync lookback window when no cursor exists (180 days).
const FIRST_SYNC_WINDOW_SECS: i64 = 180 * 86_400;
const CONTENTS_BATCH: usize = 100;
/// edit-tag batch size (bounded to keep POST bodies reasonable).
const PUSH_BATCH: usize = 100;

#[derive(Default, Debug)]
pub struct SyncReport {
    pub new_items: usize,
    pub pushed: usize,
    pub failures: usize,
    pub subscription_count: usize,
    /// Titles of pulled articles matched by a "notify" rule.
    pub notified: Vec<String>,
}

/// Cached login session; never persisted to disk.
#[derive(Clone)]
pub struct Session {
    pub base: String,
    pub username: String,
    pub auth: String,
}

/// First line of an error message, truncated: reasons never carry tokens,
/// bodies or full URLs (server text stays server-side, tokens stay in memory).
fn short_reason(msg: &str) -> String {
    const MAX_CHARS: usize = 160;
    let first = msg.lines().next().unwrap_or("").trim();
    if first.chars().count() > MAX_CHARS {
        first.chars().take(MAX_CHARS).collect()
    } else {
        first.to_string()
    }
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

/// Return the cached Auth token when it matches the account, else log in.
pub async fn ensure_session(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
) -> Result<String, GReaderError> {
    // Scope the guard: std RwLockReadGuard is not Send and must not be held
    // across the login await below.
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
    let auth = match greader::login(http, &acct.server_url, &acct.username, &acct.password).await
    {
        Ok(a) => a,
        Err(e) => {
            log::warn!("sync login failed reason {}", short_reason(&e.to_string()));
            return Err(e);
        }
    };
    store_session(state, acct, &auth);
    log::info!("sync login ok provider greader");
    Ok(auth)
}

async fn relogin(state: &AppState, http: &reqwest::Client, acct: &SyncAccount) -> Result<String, GReaderError> {
    clear_session(state);
    ensure_session(state, http, acct).await
}

/// Run one full sync cycle:
/// 1. push local queued actions to server (push-queue-first)
/// 2. sync subscriptions list (creates/updates sources & groups)
/// 3. incremental pull of new/updated items
pub async fn run(app: &AppHandle, _background: bool) -> Result<SyncReport, String> {
    let settings = crate::settings::load(&crate::settings::settings_path(app)?);
    let acct = settings
        .sync_account
        .filter(|a| a.provider == "greader")
        .ok_or("no Google Reader account configured")?;
    let state = app.state::<AppState>();
    let http = state.http_client();
    let mut auth = ensure_session(&state, &http, &acct).await.map_err(|e| e.to_string())?;

    let mut report = SyncReport::default();

    macro_rules! retry_auth {
        ($op_name:expr, $expr:expr) => {
            match $expr {
                Ok(val) => Some(val),
                Err(GReaderError::Auth(_)) => match relogin(&state, &http, &acct).await {
                    Ok(new_auth) => {
                        auth = new_auth;
                        match $expr {
                            Ok(val) => Some(val),
                            Err(e) => {
                                report.failures += 1;
                                log::warn!("{} failed reason {}", $op_name, short_reason(&e.to_string()));
                                None
                            }
                        }
                    }
                    Err(e) => {
                        report.failures += 1;
                        log::warn!("{} failed reason {}", $op_name, short_reason(&e.to_string()));
                        None
                    }
                },
                Err(e) => {
                    report.failures += 1;
                    log::warn!("{} failed reason {}", $op_name, short_reason(&e.to_string()));
                    None
                }
            }
        };
    }

    // 1. push queued local actions
    if let Some(n) = retry_auth!("sync push", push_queue(&state, &http, &acct, &auth).await) {
        report.pushed = n;
    }

    // 2. sync subscriptions
    if let Some(n) = retry_auth!("sync subscriptions", sync_subscriptions(&state, &http, &acct, &auth).await) {
        report.subscription_count = n;
    }

    // 3. incremental item pull (auth retry inline so the final failure
    // reason stays available for the warn log).
    match pull_items(&state, &http, &acct, &auth).await {
        Ok((n, notified)) => {
            report.new_items = n;
            report.notified = notified;
        }
        Err(GReaderError::Auth(_)) => match relogin(&state, &http, &acct).await {
            Ok(new_auth) => {
                auth = new_auth;
                match pull_items(&state, &http, &acct, &auth).await {
                    Ok((n, notified)) => {
                        report.new_items = n;
                        report.notified = notified;
                    }
                    Err(e) => {
                        report.failures += 1;
                        log::warn!("sync pull failed reason {}", short_reason(&e.to_string()));
                    }
                }
            }
            Err(e) => {
                report.failures += 1;
                log::warn!("sync pull failed reason {}", short_reason(&e.to_string()));
            }
        },
        Err(e) => {
            report.failures += 1;
            log::warn!("sync pull failed reason {}", short_reason(&e.to_string()));
        }
    }

    // Single aggregated sync summary: push + subscription + pull counts in
    // one line (no per-stage info).
    log::info!(
        "sync done pushed={} subs={} new={} failures={} notified={}",
        report.pushed,
        report.subscription_count,
        report.new_items,
        report.failures,
        report.notified.len()
    );
    Ok(report)
}

/// Drain the local action queue to the server. Pushed entries are deleted;
/// on a mid-way failure the remainder stays queued (re-pushing already
/// applied edits is idempotent).
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

/// Upsert server subscriptions into local groups/sources. Sources present
/// locally but not on the server are left untouched.
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

/// Local identity of a synced feed: (source_id, group_id, source_url).
type SourceIdentity = (i64, Option<i64>, String);

/// Merge server state with local rule outcome: server read/starred state wins
/// (LWW), rule flags apply on top. Returns (has_been_read, starred, hidden).
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

/// Pull items changed since the last sync cursor and upsert them with the
/// server's read/starred state, running new entries through the rule engine
/// (mark read / star / hide / notify) like the local refresh path.
/// Returns (new row count, notify-matched titles).
async fn pull_items(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &str,
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

    // Page through the reading-list stream (no read filter: state changes for
    // already-known items must be observed too).
    let mut ids: Vec<String> = Vec::new();
    let mut continuation: Option<String> = None;
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
        if continuation.is_none() || page_len == 0 {
            break;
        }
    }

    let mut new_count = 0usize;
    let mut notified: Vec<String> = Vec::new();
    for chunk in ids.chunks(CONTENTS_BATCH) {
        let items = greader::contents(http, &acct.server_url, auth, chunk).await?;
        let conn = state.db.lock().await;
        for item in items {
            let Some((source_id, group_id, source_url)) = stream_map.get(&item.stream_id) else {
                continue; // feed not linked locally yet
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
        // Given: server says unread/unstarred, rules say mark-read + star + hide + notify
        let outcome = crate::rules::Outcome { mark_read: true, star: true, hide: true, notify: true };
        // When: merged
        let flags = apply_rules_to_remote(false, false, &outcome);
        // Then: every rule flag is honored
        assert_eq!(flags, (true, true, true));

        // Given: server already read/starred, no rules match
        let quiet = crate::rules::Outcome::default();
        // When: merged
        let flags = apply_rules_to_remote(true, true, &quiet);
        // Then: server state survives untouched
        assert_eq!(flags, (true, true, false));

        // Given: server unread, only notify matches (no state change)
        let notify_only = crate::rules::Outcome { notify: true, ..Default::default() };
        // When: merged
        let flags = apply_rules_to_remote(false, false, &notify_only);
        // Then: row state unchanged
        assert_eq!(flags, (false, false, false));
    }
}
