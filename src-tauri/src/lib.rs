mod backup;
mod commands;
mod db;
mod extractor;
mod feed;
mod greader;
mod models;
mod net;
mod opml_io;
mod rules;
mod settings;
mod sync;
mod tray;
mod z_log;

use std::sync::RwLock;
use tokio::sync::Mutex;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub db_path: std::path::PathBuf,
    /// Swappable HTTP client: rebuilt when proxy settings change.
    pub http: RwLock<reqwest::Client>,
    /// In-memory cloud-sync login session (never persisted).
    pub sync_token: RwLock<Option<sync::Session>>,
}

impl AppState {
    pub fn http_client(&self) -> reqwest::Client {
        self.http.read().expect("http client lock").clone()
    }

    pub fn set_http_client(&self, client: reqwest::Client) {
        *self.http.write().expect("http client lock") = client;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    z_log::install_panic_hook();
    tauri::Builder::default()
        .plugin(z_log::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            use tauri::Manager;
            z_log::prune_app_dir(app.handle());
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("zreader.db");
            let conn = db::open(&db_path)?;
            let settings = settings::load(&settings::settings_path(app.handle())?);
            let http = net::build_http_client(&settings);
            app.manage(AppState {
                db: Mutex::new(conn),
                db_path,
                http: RwLock::new(http),
                sync_token: RwLock::new(None),
            });

            if let Err(e) = tray::create_tray(app.handle()) {
                log::error!("tray create failed reason {}", short_reason(&e.to_string()));
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                background_refresh(handle).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    use tauri::Manager;
                    let app = window.app_handle();
                    let s = settings::load(&settings::settings_path(app).unwrap_or_default());
                    if s.close_to_tray {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_sources,
            commands::get_groups,
            commands::create_group,
            commands::rename_group,
            commands::delete_group,
            commands::set_group_expanded,
            commands::add_source,
            commands::remove_source,
            commands::rename_source,
            commands::set_source_group,
            commands::fetch_sources,
            commands::get_items,
            commands::get_item,
            commands::mark_read,
            commands::mark_all_read,
            commands::star,
            commands::set_item_hidden,
            commands::fetch_full_content,
            commands::get_settings,
            commands::save_settings,
            commands::import_opml,
            commands::export_opml,
            commands::set_custom_favicon,
            commands::refresh_favicon,
            commands::test_proxy,
            commands::sync_login,
            commands::sync_logout,
            commands::sync_status,
            commands::sync_now,
            commands::get_rules,
            commands::create_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::apply_rules_backfill,
            commands::export_backup,
            commands::import_backup,
            commands::get_stats,
            commands::vacuum_now,
            commands::cleanup_now,
            z_log::zlog_get_dir,
            z_log::zlog_export_bundle,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Background loop: refresh all sources every `fetchInterval` minutes.
async fn background_refresh(app: tauri::AppHandle) {
    let mut last_fetch = std::time::Instant::now();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let Ok(path) = settings::settings_path(&app) else { continue };
        let s = settings::load(&path);
        if last_fetch.elapsed().as_secs() < s.fetch_interval.max(1) * 60 {
            continue;
        }
        last_fetch = std::time::Instant::now();
        let _ = refresh_all_sources(app.clone(), None, true).await;
    }
}

/// Max simultaneous feed fetches during a refresh cycle.
const REFRESH_CONCURRENCY: usize = 6;

/// First line of an error message, truncated: log reasons never span lines
/// and never carry bodies/URLs/tokens (those stay in the DB or memory).
fn short_reason(msg: &str) -> String {
    const MAX_CHARS: usize = 160;
    let first = msg.lines().next().unwrap_or("").trim();
    if first.chars().count() > MAX_CHARS {
        first.chars().take(MAX_CHARS).collect()
    } else {
        first.to_string()
    }
}


/// Per-source result of one refresh task.
struct SourceRefreshOutcome {
    inserted: usize,
    notified: Vec<String>,
    failed: bool,
    /// Stable failure class for `fail_kinds` aggregation (None on success).
    error_kind: Option<String>,
}

/// Everything one refresh task needs; owned so tasks are `'static`.
/// Note: no title/URL here — log lines carry ids + first-line reasons only.
struct RefreshTask {
    app: tauri::AppHandle,
    client: reqwest::Client,
    engine: std::sync::Arc<rules::RuleEngine>,
    favicon_dir: std::path::PathBuf,
    background: bool,
    allow_third_party: bool,
    id: i64,
    group_id: Option<i64>,
    url: String,
    favicon: Option<String>,
    cycle: String,
}

/// Refresh a single source: fetch + parse (network), store + marks (short DB
/// critical sections), lazy favicon fill. Never propagates errors; records
/// them on the source row and reports them via the outcome.
async fn refresh_one_source(task: RefreshTask) -> SourceRefreshOutcome {
    use tauri::{Emitter, Manager};
    let RefreshTask { app, client, engine, favicon_dir, background, allow_third_party, id, group_id, url, favicon, cycle } = task;
    if !background {
        let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": false }));
    }
    let state = app.state::<AppState>();
    let mut out = SourceRefreshOutcome { inserted: 0, notified: Vec::new(), failed: false, error_kind: None };
    let ctx = feed::SourceCtx { id, group_id, url: url.clone() };
    // Release-visible per-feed timing: host only (no raw URL/query tokens),
    // split into fetch / store / favicon segments + total.
    let total_start = std::time::Instant::now();
    let (host, _) = crate::net::sanitize_url(&url);
    let fetch_start = std::time::Instant::now();
    match feed::fetch_and_parse_with(&client, &url, &cycle, Some(id)).await {
        Ok(parsed) => {
            let fetch_ms = fetch_start.elapsed().as_millis();
            let store_start = std::time::Instant::now();
            let stored = {
                let conn = state.db.lock().await;
                feed::store(&conn, &ctx, &parsed, Some(&engine))
            };
            let store_ms = store_start.elapsed().as_millis();
            match stored {
                Ok(stored) => {
                    out.inserted = stored.inserted;
                    out.notified = stored.notified;
                }
                Err(e) => {
                    out.failed = true;
                    let error_kind = crate::net::classify_error(&e);
                    out.error_kind = Some(error_kind.to_string());
                    log::warn!("feed store failed cycle={cycle} id={id} host={host} error_kind={error_kind} reason {}", short_reason(&e.to_string()));
                    let conn = state.db.lock().await;
                    let _ = db::mark_source_fetched(&conn, id, false, Some(&e));
                }
            }
            let favicon_start = std::time::Instant::now();
            if favicon.is_none() {
                let icon_url = parsed.icon_url.as_deref();
                let site_url = parsed.site_url.as_deref();
                if let Some(fav) =
                    feed::fetch_favicon_with(&client, &url, icon_url, site_url, &favicon_dir, id, allow_third_party, &cycle).await
                {
                    let conn = state.db.lock().await;
                    let _ = db::set_source_favicon(&conn, id, fav.to_string_lossy().as_ref());
                }
            }
            let favicon_ms = favicon_start.elapsed().as_millis();
            if !out.failed {
                let conn = state.db.lock().await;
                let _ = db::mark_source_fetched(&conn, id, true, None);
                // Per-feed success stays INFO (full volume): host + new count
                // + segmented timing + cycle. Keys lowercase, `_ms` bare numbers.
                let elapsed_ms = total_start.elapsed().as_millis();
                if out.inserted > 0 {
                    log::info!("feed refreshed cycle={cycle} id={id} host={host} new={} fetch_ms={fetch_ms} store_ms={store_ms} favicon_ms={favicon_ms} elapsed_ms={elapsed_ms}", out.inserted);
                } else {
                    log::info!("feed up to date cycle={cycle} id={id} host={host} fetch_ms={fetch_ms} store_ms={store_ms} favicon_ms={favicon_ms} elapsed_ms={elapsed_ms}");
                }
            }
        }
        Err(e) => {
            out.failed = true;
            let fetch_ms = fetch_start.elapsed().as_millis();
            let error_kind = crate::net::classify_error(&e);
            out.error_kind = Some(error_kind.to_string());
            // Failures stay warn (diagnosable) with cycle + id + host +
            // error_kind + fetch timing + first-line reason only.
            log::warn!("feed fetch failed cycle={cycle} id={id} host={host} error_kind={error_kind} fetch_ms={fetch_ms} reason {}", short_reason(&e));
            let conn = state.db.lock().await;
            let _ = db::mark_source_fetched(&conn, id, false, Some(&e));
        }
    }
    if !background {
        let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": true }));
    }
    out
}

/// Fold one per-source outcome into the refresh-cycle totals.
fn merge_outcome(
    total_new: &mut usize,
    failures: &mut usize,
    notified: &mut Vec<String>,
    fail_kinds: &mut std::collections::HashMap<String, usize>,
    out: SourceRefreshOutcome,
) {
    *total_new += out.inserted;
    if out.failed {
        *failures += 1;
        let kind = out.error_kind.unwrap_or_else(|| "other".to_string());
        *fail_kinds.entry(kind).or_insert(0) += 1;
    }
    notified.extend(out.notified);
}

/// Render `fail_kinds={timeout:2,http_5xx:1}` sorted by key; empty when none.
fn format_fail_kinds(fail_kinds: &std::collections::HashMap<String, usize>) -> String {
    if fail_kinds.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(&String, &usize)> = fail_kinds.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let inner: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}:{v}")).collect();
    format!(" fail_kinds={{{}}}", inner.join(","))
}

/// Fetch all (or selected) sources, run new entries through the rule engine,
/// store them, refresh missing favicons, apply the retention policy and sync
/// the tray badge. Emits fetch-progress / fetch-done for the frontend.
///
/// `background` = timer-triggered: no per-source progress events, and an
/// aggregated desktop notification when new articles (or rule "notify"
/// matches) arrive.
pub async fn refresh_all_sources(
    app: tauri::AppHandle,
    ids: Option<Vec<i64>>,
    background: bool,
) -> Result<usize, String> {
    use tauri::{Emitter, Manager};
    let state = app.state::<AppState>();
    let settings = settings::load(&settings::settings_path(&app)?);

    // Cloud sync mode: subscriptions live on the server, so refresh = sync.
    // Rule-engine notify matches from the pull are forwarded so background
    // notifications behave like the local refresh path.
    let mode = if background { "background" } else { "manual" };
    let cycle = crate::net::new_cycle();
    let cycle_start = std::time::Instant::now();
    let (total_new, failures, notified, feed_count, fail_kinds) = if settings.sync_account.as_ref().is_some_and(|a| a.provider == "greader") {
        log::info!("refresh start cycle={cycle} mode={mode} sync=true");
        let report = sync::run_with_cycle(&app, background, &cycle).await?;
        let kinds = report.fail_kinds.clone();
        (report.new_items, report.failures, report.notified, report.subscription_count, kinds)
    } else {
        let client = state.http_client();
        let engine = std::sync::Arc::new({
            let conn = state.db.lock().await;
            match rules::RuleEngine::load(&conn) {
                Ok(e) => e,
                Err(e) => {
                    let error_kind = crate::net::classify_error(&e);
                    log::warn!("refresh rules load failed cycle={cycle} error_kind={error_kind} reason {}", short_reason(&e));
                    return Err(e);
                }
            }
        });
        let targets: Vec<(i64, Option<i64>, String, Option<String>)> = {
            let conn = state.db.lock().await;
            match ids {
                Some(v) => db::get_sources(&conn)?
                    .into_iter()
                    .filter(|s| v.contains(&s.id))
                    .map(|s| (s.id, s.group_id, s.url, s.favicon))
                    .collect(),
                None => db::get_sources(&conn)?
                    .into_iter()
                    .map(|s| (s.id, s.group_id, s.url, s.favicon))
                    .collect(),
            }
        };
        let dir = commands::favicon_dir(&app)?;
        let feed_count = targets.len();
        log::info!("refresh start cycle={cycle} mode={mode} sync=false feeds={feed_count}");

        // Bounded-concurrency refresh: one slow feed no longer blocks the rest.
        // Each task owns its network I/O and takes the DB lock only for short
        // store/mark critical sections.
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(REFRESH_CONCURRENCY));
        let mut set = tokio::task::JoinSet::new();
        for (id, group_id, url, favicon) in targets {
            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| e.to_string())?;
            let task = RefreshTask {
                app: app.clone(),
                client: client.clone(),
                engine: engine.clone(),
                favicon_dir: dir.clone(),
                background,
                allow_third_party: settings.favicon_third_party,
                id,
                group_id,
                url,
                favicon,
                cycle: cycle.clone(),
            };
            set.spawn(async move {
                let _permit = permit;
                refresh_one_source(task).await
            });
        }

        let mut total_new = 0usize;
        let mut failures = 0usize;
        let mut notified: Vec<String> = Vec::new();
        let mut fail_kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        while let Some(res) = set.join_next().await {
            match res {
                Ok(out) => merge_outcome(&mut total_new, &mut failures, &mut notified, &mut fail_kinds, out),
                Err(e) => {
                    failures += 1;
                    *fail_kinds.entry("other".to_string()).or_insert(0) += 1;
                    log::debug!("refresh task join failed cycle={cycle} reason {}", short_reason(&e.to_string()));
                }
            }
        }
        (total_new, failures, notified, feed_count, fail_kinds)
    };

    // Retention policy; VACUUM only after large deletions to avoid churn.
    // Count converges to one line: info only when rows actually moved.
    {
        let conn = state.db.lock().await;
        match db::cleanup_retention(&conn, settings.retention_days, settings.max_items_per_source) {
            Ok(n) => {
                if n > 200 {
                    let _ = db::vacuum(&conn);
                }
                if n > 0 {
                    log::info!("retention done deleted={n}");
                } else {
                    log::debug!("retention done deleted=0");
                }
            }
            Err(e) => {
                log::warn!("retention failed reason {}", short_reason(&e));
            }
        }
    }

    tray::update_tray(&app).await;

    let is_sync = settings.sync_account.as_ref().is_some_and(|a| a.provider == "greader");
    if background {
        let _ = app.emit(
            "fetch-done",
            serde_json::json!({ "newItems": total_new, "failures": failures, "background": true, "sync": is_sync }),
        );
        if (settings.notify_on_new && total_new > 0) || !notified.is_empty() {
            notify_new_articles(&app, &settings, total_new, &notified);
        }
    } else {
        let _ = app.emit(
            "fetch-done",
            serde_json::json!({ "newItems": total_new, "failures": failures, "sync": is_sync }),
        );
    }
    // Single aggregated cycle summary with fail_kinds breakdown (only when
    // failures exist; notified only as count, never titles).
    let fail_suffix = format_fail_kinds(&fail_kinds);
    log::info!(
        "refresh done cycle={cycle} mode={mode} sync={is_sync} feeds={feed_count} new={total_new} failures={failures}{fail_suffix} notified={} elapsed_ms={}",
        notified.len(),
        cycle_start.elapsed().as_millis()
    );
    Ok(total_new)
}

fn notify_new_articles(
    app: &tauri::AppHandle,
    settings: &models::Settings,
    new_count: usize,
    rule_titles: &[String],
) {
    use tauri_plugin_notification::NotificationExt;
    let zh = settings.locale.starts_with("zh");
    let mut body = if zh {
        format!("获取到 {new_count} 篇新文章")
    } else {
        format!("Fetched {new_count} new articles")
    };
    if !rule_titles.is_empty() {
        let preview: Vec<String> = rule_titles.iter().take(3).cloned().collect();
        let rest = rule_titles.len() - preview.len();
        let list = if zh { preview.join("、") } else { preview.join(", ") };
        body = if zh {
            let more = if rest > 0 { format!(" 等 {rest} 篇") } else { String::new() };
            format!("命中通知规则：{list}{more}")
        } else {
            let more = if rest > 0 { format!(" and {rest} more") } else { String::new() };
            format!("Notify rules matched: {list}{more}")
        };
    }
    let _ = app
        .notification()
        .builder()
        .title("ZReader")
        .body(&body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_outcome_folds_totals() {
        // Given: a mix of successful, notifying and failed per-source outcomes
        let outcomes = vec![
            SourceRefreshOutcome { inserted: 3, notified: vec!["a".into()], failed: false, error_kind: None },
            SourceRefreshOutcome { inserted: 0, notified: vec![], failed: true, error_kind: Some("timeout".into()) },
            SourceRefreshOutcome { inserted: 2, notified: vec!["b".into(), "c".into()], failed: false, error_kind: None },
        ];
        // When: folded
        let (mut total, mut failures, mut notified) = (0usize, 0usize, Vec::new());
        let mut fail_kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for out in outcomes {
            merge_outcome(&mut total, &mut failures, &mut notified, &mut fail_kinds, out);
        }
        // Then: inserts sum, failures count, titles concatenate in order
        assert_eq!(total, 5);
        assert_eq!(failures, 1);
        assert_eq!(notified, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(fail_kinds.get("timeout"), Some(&1));
    }
}
