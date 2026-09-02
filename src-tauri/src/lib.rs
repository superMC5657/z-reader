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
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            use tauri::Manager;
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
                log::warn!("tray init failed: {e}");
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
    if settings.sync_account.as_ref().is_some_and(|a| a.provider == "greader") {
        let report = sync::run(&app, background).await?;
        return Ok(report.new_items);
    }

    let client = state.http_client();
    let engine = {
        let conn = state.db.lock().await;
        rules::RuleEngine::load(&conn)?
    };
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

    let mut total_new = 0usize;
    let mut failures = 0usize;
    let mut notified: Vec<String> = Vec::new();
    for (id, group_id, url, favicon) in &targets {
        if !background {
            let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": false }));
        }
        let ctx = feed::SourceCtx { id: *id, group_id: *group_id, url: url.clone() };
        match feed::fetch_and_parse(&client, url).await {
            Ok(parsed) => {
                let outcome = {
                    let conn = state.db.lock().await;
                    feed::store(&conn, &ctx, &parsed, Some(&engine))
                };
                match outcome {
                    Ok(out) => {
                        total_new += out.inserted;
                        notified.extend(out.notified);
                    }
                    Err(e) => {
                        failures += 1;
                        log::warn!("store source {id} failed: {e}");
                        let conn = state.db.lock().await;
                        let _ = db::mark_source_fetched(&conn, *id, false);
                    }
                }
                if favicon.is_none() {
                    let icon_url = parsed.icon_url.as_deref();
                    let site_url = parsed.site_url.as_deref();
                    if let Some(fav) =
                        feed::fetch_favicon(&client, url, icon_url, site_url, &dir, *id).await
                    {
                        let conn = state.db.lock().await;
                        let _ = db::set_source_favicon(&conn, *id, fav.to_string_lossy().as_ref());
                    }
                }
                let conn = state.db.lock().await;
                let _ = db::mark_source_fetched(&conn, *id, true);
            }
            Err(e) => {
                failures += 1;
                log::warn!("refresh source {id} failed: {e}");
                let conn = state.db.lock().await;
                let _ = db::mark_source_fetched(&conn, *id, false);
            }
        }
        if !background {
            let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": true }));
        }
    }

    // Retention policy; VACUUM only after large deletions to avoid churn.
    {
        let conn = state.db.lock().await;
        match db::cleanup_retention(&conn, settings.retention_days, settings.max_items_per_source) {
            Ok(n) if n > 200 => {
                let _ = db::vacuum(&conn);
            }
            Ok(_) => {}
            Err(e) => log::warn!("retention cleanup failed: {e}"),
        }
    }

    tray::update_tray(&app).await;

    if background {
        let _ = app.emit(
            "fetch-done",
            serde_json::json!({ "newItems": total_new, "failures": failures, "background": true }),
        );
        if (settings.notify_on_new && total_new > 0) || !notified.is_empty() {
            notify_new_articles(&app, &settings, total_new, &notified);
        }
    } else {
        let _ = app.emit(
            "fetch-done",
            serde_json::json!({ "newItems": total_new, "failures": failures }),
        );
    }
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
