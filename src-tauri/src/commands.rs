use crate::db;
use crate::feed;
use crate::models::{GetItemsParams, Settings};
use crate::opml_io;
use crate::settings as settings_io;
use crate::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn favicon_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = data_dir(app)?.join("favicons");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
pub async fn get_sources(state: State<'_, AppState>) -> Result<Vec<crate::models::Source>, String> {
    let conn = state.db.lock().await;
    db::get_sources(&conn)
}

#[tauri::command]
pub async fn get_groups(state: State<'_, AppState>) -> Result<Vec<crate::models::Group>, String> {
    let conn = state.db.lock().await;
    db::get_groups(&conn)
}

#[tauri::command]
pub async fn create_group(state: State<'_, AppState>, name: String) -> Result<crate::models::Group, String> {
    let conn = state.db.lock().await;
    db::create_group(&conn, &name)
}

#[tauri::command]
pub async fn rename_group(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::rename_group(&conn, id, &name)
}

#[tauri::command]
pub async fn delete_group(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::delete_group(&conn, id)
}

#[tauri::command]
pub async fn set_group_expanded(state: State<'_, AppState>, id: i64, expanded: bool) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::set_group_expanded(&conn, id, expanded)
}

/// Validate the URL by fetching its feed, then persist the source and its entries.
#[tauri::command]
pub async fn add_source(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    group_id: Option<i64>,
) -> Result<crate::models::Source, String> {
    let mut url = url.trim().to_string();
    if !url.contains("://") {
        url = format!("https://{url}");
    }
    let client = state.http.clone();

    {
        let conn = state.db.lock().await;
        if let Some(s) = db::get_source_by_url(&conn, &url)? {
            return Err(format!("source already exists: {}", s.title));
        }
    }

    // Probe without persisting: parse the remote feed to validate first.
    let parsed = feed::fetch_and_parse(&client, &url).await?;
    let title = if parsed.title.is_empty() { url.clone() } else { parsed.title.clone() };

    let source = {
        let conn = state.db.lock().await;
        let s = db::insert_source(&conn, &url, &title, parsed.description.as_deref(), group_id)?;
        feed::store(&conn, s.id, &parsed)?;
        db::mark_source_fetched(&conn, s.id, true)?;
        db::get_source(&conn, s.id)?
    };
    if source.favicon.is_none() {
        let dir = favicon_dir(&app)?;
        let icon_url = parsed.icon_url.as_deref();
        let site_url = parsed.site_url.as_deref();
        if let Some(fav) = feed::fetch_favicon(&client, &url, icon_url, site_url, &dir, source.id).await {
            let conn = state.db.lock().await;
            db::set_source_favicon(&conn, source.id, fav.to_string_lossy().as_ref())?;
        }
    }
    Ok(source)
}

#[tauri::command]
pub async fn remove_source(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::remove_source(&conn, id)
}

#[tauri::command]
pub async fn rename_source(state: State<'_, AppState>, id: i64, title: String) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::rename_source(&conn, id, &title)
}

#[tauri::command]
pub async fn set_source_group(state: State<'_, AppState>, id: i64, group_id: Option<i64>) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::set_source_group(&conn, id, group_id)
}

#[tauri::command]
pub async fn set_custom_favicon(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    data_base64: String,
) -> Result<String, String> {
    let dir = favicon_dir(&app)?;
    let raw = if let Some(idx) = data_base64.find("base64,") {
        &data_base64[idx + 7..]
    } else {
        &data_base64
    };
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| format!("invalid base64: {e}"))?;
    if bytes.is_empty() || bytes.len() > 5_000_000 {
        return Err("image data too large or empty".into());
    }
    let ext = if data_base64.contains("image/svg") {
        "svg"
    } else if data_base64.contains("image/jpeg") || data_base64.contains("image/jpg") {
        "jpg"
    } else {
        "png"
    };
    let path = dir.join(format!("{id}.{ext}"));
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().to_string();
    {
        let conn = state.db.lock().await;
        db::set_source_favicon(&conn, id, &path_str)?;
    }
    Ok(path_str)
}

#[tauri::command]
pub async fn refresh_favicon(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<String>, String> {
    let (url, mut parsed) = {
        let conn = state.db.lock().await;
        let s = db::get_source(&conn, id)?;
        (s.url, None)
    };
    let client = state.http.clone();
    if let Ok(p) = feed::fetch_and_parse(&client, &url).await {
        parsed = Some(p);
    }
    let dir = favicon_dir(&app)?;
    let icon_url = parsed.as_ref().and_then(|p| p.icon_url.as_deref());
    let site_url = parsed.as_ref().and_then(|p| p.site_url.as_deref());
    if let Some(fav) = feed::fetch_favicon(&client, &url, icon_url, site_url, &dir, id).await {
        let path_str = fav.to_string_lossy().to_string();
        let conn = state.db.lock().await;
        db::set_source_favicon(&conn, id, &path_str)?;
        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

/// Fetch all sources, or just the given ids. Emits "fetch-progress" / "fetch-done".
#[tauri::command]
pub async fn fetch_sources(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Option<Vec<i64>>,
) -> Result<usize, String> {
    let targets: Vec<(i64, String, Option<String>)> = {
        let conn = state.db.lock().await;
        match ids {
            Some(v) => db::get_sources(&conn)?
                .into_iter()
                .filter(|s| v.contains(&s.id))
                .map(|s| (s.id, s.url, s.favicon))
                .collect(),
            None => db::get_sources(&conn)?
                .into_iter()
                .map(|s| (s.id, s.url, s.favicon))
                .collect(),
        }
    };
    let dir = favicon_dir(&app)?;
    let client = state.http.clone();
    let mut total_new = 0usize;
    let mut failures = 0usize;
    for (id, url, favicon) in &targets {
        let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": false }));
        let result = feed::fetch_and_parse(&client, url).await;
        match result {
            Ok(parsed) => {
                let conn = state.db.lock().await;
                match feed::store(&conn, *id, &parsed) {
                    Ok(n) => total_new += n,
                    Err(e) => {
                        failures += 1;
                        log::warn!("store source {id} failed: {e}");
                        let _ = db::mark_source_fetched(&conn, *id, false);
                    }
                }
                drop(conn);
                if favicon.is_none() {
                    let icon_url = parsed.icon_url.as_deref();
                    let site_url = parsed.site_url.as_deref();
                    if let Some(fav) = feed::fetch_favicon(&client, url, icon_url, site_url, &dir, *id).await {
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
        let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": true }));
    }
    let _ = app.emit(
        "fetch-done",
        serde_json::json!({ "newItems": total_new, "failures": failures }),
    );
    Ok(total_new)
}

#[tauri::command]
pub async fn get_items(
    state: State<'_, AppState>,
    params: GetItemsParams,
) -> Result<Vec<crate::models::Item>, String> {
    let conn = state.db.lock().await;
    // The list never ships full article HTML; the reader loads it per-item via get_item.
    Ok(db::get_items(&conn, &params)?
        .into_iter()
        .map(|mut i| {
            i.content = None;
            i
        })
        .collect())
}

#[tauri::command]
pub async fn get_item(state: State<'_, AppState>, id: i64) -> Result<crate::models::Item, String> {
    let conn = state.db.lock().await;
    db::get_item(&conn, id)
}

#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, ids: Vec<i64>, read: bool) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::set_items_read(&conn, &ids, read)
}

#[tauri::command]
pub async fn mark_all_read(
    state: State<'_, AppState>,
    scope: Option<String>,
    scope_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::mark_all_read(&conn, scope.as_deref(), scope_id)?;
    Ok(())
}

#[tauri::command]
pub async fn star(state: State<'_, AppState>, id: i64, starred: bool) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::set_item_starred(&conn, id, starred)
}

#[tauri::command]
pub async fn fetch_full_content(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let client = state.http.clone();
    let link = {
        let conn = state.db.lock().await;
        db::get_item(&conn, id)?.url.ok_or("item has no link")?
    };
    let content = crate::extractor::extract_from_url(&client, &link).await?;
    let snippet = crate::extractor::snippet_of(&content);
    let conn = state.db.lock().await;
    db::set_item_content(&conn, id, &content, &snippet)
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(settings_io::load(&settings_io::settings_path(&app)?))
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    settings_io::save(&settings_io::settings_path(&app)?, &settings)
}

#[tauri::command]
pub async fn import_opml(state: State<'_, AppState>, text: String) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().await;
    let r = opml_io::import(&conn, &text)?;
    Ok(serde_json::json!({
        "groupsAdded": r.groups_added,
        "sourcesAdded": r.sources_added,
        "sourcesExisting": r.sources_existing,
    }))
}

/// Returns the OPML document as XML text for the frontend to download.
#[tauri::command]
pub async fn export_opml(state: State<'_, AppState>) -> Result<String, String> {
    let conn = state.db.lock().await;
    opml_io::export(&conn)
}
