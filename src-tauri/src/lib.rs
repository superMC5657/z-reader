mod commands;
mod db;
mod extractor;
mod feed;
mod models;
mod opml_io;
mod settings;


use tokio::sync::Mutex;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub http: reqwest::Client,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let conn = db::open(&data_dir.join("zreader.db"))?;
            let http = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; ZReader/0.1)")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| e.to_string())?;
            app.manage(AppState { db: Mutex::new(conn), http });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                background_refresh(handle).await;
            });
            Ok(())
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
            commands::fetch_full_content,
            commands::get_settings,
            commands::save_settings,
            commands::import_opml,
            commands::export_opml,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Background loop: refresh all sources every `fetchInterval` minutes.
async fn background_refresh(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let mut last_fetch: Option<std::time::Instant> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let interval = {
            let Ok(path) = settings::settings_path(&app) else { continue };
            let s = settings::load(&path);
            s.fetch_interval.max(1)
        };
        let due = match last_fetch {
            None => false, // let the user trigger the first refresh themselves
            Some(t) => t.elapsed().as_secs() >= interval * 60,
        };
        if !due {
            continue;
        }
        last_fetch = Some(std::time::Instant::now());
        let state = app.state::<AppState>();
        let targets = {
            let conn = state.db.lock().await;
            match db::get_sources(&conn) {
                Ok(list) => list
                    .into_iter()
                    .map(|s| (s.id, s.url))
                    .collect::<Vec<_>>(),
                Err(_) => continue,
            }
        };
        let client = state.http.clone();
        let mut new_items = 0usize;
        for (id, url) in &targets {
            let result = match feed::fetch_and_parse(&client, url).await {
                Ok(parsed) => {
                    let conn = state.db.lock().await;
                    feed::store(&conn, *id, &parsed)
                }
                Err(e) => Err(e),
            };
            match result {
                Ok(n) => {
                    new_items += n;
                    let conn = state.db.lock().await;
                    let _ = db::mark_source_fetched(&conn, *id, true);
                }
                Err(e) => {
                    let conn = state.db.lock().await;
                    let _ = db::mark_source_fetched(&conn, *id, false);
                    log::warn!("background refresh source {id} failed: {e}");
                }
            }
        }
        let _ = app.emit("fetch-done", serde_json::json!({ "newItems": new_items, "failures": 0, "background": true }));
    }
}
