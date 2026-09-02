//! Cloud sync engine for Google Reader compatible servers.
//!
//! Per run: push the local action queue first (so later pulls observe our own
//! changes), reconcile subscriptions, then incrementally pull changed items
//! with the server's read/starred state winning (last-write-wins).

use crate::db;
use crate::greader::{self, GReaderError};
use crate::models::{html_to_text, SyncAccount};
use crate::AppState;
use tauri::{Emitter, Manager};

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
}

/// Cached login session; never persisted to disk.
#[derive(Clone)]
pub struct Session {
    pub base: String,
    pub username: String,
    pub auth: String,
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
    let auth = greader::login(http, &acct.server_url, &acct.username, &acct.password).await?;
    store_session(state, acct, &auth);
    Ok(auth)
}

async fn relogin(state: &AppState, http: &reqwest::Client, acct: &SyncAccount) -> Result<String, GReaderError> {
    clear_session(state);
    ensure_session(state, http, acct).await
}

/// Run one full sync cycle. Emits "fetch-done" so the frontend reloads.
pub async fn run(app: &tauri::AppHandle, background: bool) -> Result<SyncReport, String> {
    let settings = crate::settings::load(&crate::settings::settings_path(app)?);
    let acct = settings
        .sync_account
        .filter(|a| a.provider == "greader")
        .ok_or("no Google Reader account configured")?;
    let state = app.state::<AppState>();
    let http = state.http_client();
    let mut auth = ensure_session(&state, &http, &acct).await.map_err(|e| e.to_string())?;

    let mut report = SyncReport::default();

    // 1. push queued local actions
    match push_queue(&state, &http, &acct, &auth).await {
        Ok(n) => report.pushed = n,
        Err(GReaderError::Auth(_)) => {
            auth = relogin(&state, &http, &acct).await.map_err(|e| e.to_string())?;
            match push_queue(&state, &http, &acct, &auth).await {
                Ok(n) => report.pushed = n,
                Err(e) => {
                    log::warn!("sync push retry failed: {e}");
                    report.failures += 1;
                }
            }
        }
        Err(e) => {
            log::warn!("sync push failed: {e}");
            report.failures += 1;
        }
    }

    // 2. sync subscriptions
    match sync_subscriptions(&state, &http, &acct, &auth).await {
        Ok(n) => report.subscription_count = n,
        Err(GReaderError::Auth(_)) => {
            auth = relogin(&state, &http, &acct).await.map_err(|e| e.to_string())?;
            match sync_subscriptions(&state, &http, &acct, &auth).await {
                Ok(n) => report.subscription_count = n,
                Err(e) => {
                    log::warn!("sync subscriptions retry failed: {e}");
                    report.failures += 1;
                }
            }
        }
        Err(e) => {
            log::warn!("sync subscriptions failed: {e}");
            report.failures += 1;
        }
    }

    // 3. incremental item pull
    match pull_items(&state, &http, &acct, &auth).await {
        Ok(n) => report.new_items = n,
        Err(GReaderError::Auth(_)) => {
            auth = relogin(&state, &http, &acct).await.map_err(|e| e.to_string())?;
            match pull_items(&state, &http, &acct, &auth).await {
                Ok(n) => report.new_items = n,
                Err(e) => {
                    log::warn!("sync pull retry failed: {e}");
                    report.failures += 1;
                }
            }
        }
        Err(e) => {
            log::warn!("sync pull failed: {e}");
            report.failures += 1;
        }
    }

    crate::tray::update_tray(app).await;
    let _ = app.emit(
        "fetch-done",
        serde_json::json!({
            "newItems": report.new_items,
            "failures": report.failures,
            "background": background,
            "sync": true,
        }),
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
    let mut by_action: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    let mut stream_actions: Vec<(i64, String)> = Vec::new();
    for e in entries {
        if e.action == "mark_all_read" {
            stream_actions.push((e.id, e.target));
        } else {
            by_action.entry(e.action).or_default().push((e.id, e.target));
        }
    }

    let mut pushed_ids: Vec<i64> = Vec::new();
    for (action, items) in &by_action {
        let (add, remove): (&[&str], &[&str]) = match action.as_str() {
            "mark_read" => (&[greader::STATE_READ], &[]),
            "mark_unread" => (&[], &[greader::STATE_READ]),
            "star" => (&[greader::STATE_STARRED], &[]),
            "unstar" => (&[], &[greader::STATE_STARRED]),
            _ => continue,
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
            let group_id = match &sub.category {
                Some(label) => Some(
                    db::find_or_create_group(&conn, label)
                        .map_err(GReaderError::Other)?
                        .id,
                ),
                None => None,
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

/// Pull items changed since the last sync cursor and upsert them with the
/// server's read/starred state.
async fn pull_items(
    state: &AppState,
    http: &reqwest::Client,
    acct: &SyncAccount,
    auth: &str,
) -> Result<usize, GReaderError> {
    let (ot, stream_map): (i64, std::collections::HashMap<String, i64>) = {
        let conn = state.db.lock().await;
        let ot = db::get_state(&conn, "greader.last_sync")
            .map_err(GReaderError::Other)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let ot = if ot > 0 { ot } else { crate::models::now_ts() - FIRST_SYNC_WINDOW_SECS };
        let map = db::get_sources(&conn)
            .map_err(GReaderError::Other)?
            .into_iter()
            .filter_map(|s| s.remote_id.map(|r| (r, s.id)))
            .collect();
        (ot, map)
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
    for chunk in ids.chunks(CONTENTS_BATCH) {
        let items = greader::contents(http, &acct.server_url, auth, chunk).await?;
        let conn = state.db.lock().await;
        for item in items {
            let Some(&source_id) = stream_map.get(&item.stream_id) else {
                continue; // feed not linked locally yet
            };
            let content = ammonia::clean(&item.content);
            let snippet: String = html_to_text(&content).trim().chars().take(200).collect();
            let inserted = db::upsert_remote_item(
                &conn,
                &db::RemoteItemUpsert {
                    remote_id: &item.remote_id,
                    source_id,
                    title: &item.title,
                    url: item.url.as_deref(),
                    author: item.author.as_deref(),
                    published_at: item.published_at,
                    content: Some(&content),
                    summary: None,
                    snippet: Some(&snippet),
                    has_been_read: item.read,
                    starred: item.starred,
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
    Ok(new_count)
}
