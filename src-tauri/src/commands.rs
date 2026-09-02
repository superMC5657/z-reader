use crate::db;
use crate::feed;
use crate::models::{GetItemsParams, Settings};
use crate::opml_io;
use crate::settings as settings_io;
use crate::AppState;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

pub(crate) fn favicon_dir(app: &AppHandle) -> Result<PathBuf, String> {
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
    let client = state.http_client();

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
        let ctx = feed::SourceCtx { id: s.id, group_id: s.group_id, url: url.clone() };
        feed::store(&conn, &ctx, &parsed, None)?;
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
    let client = state.http_client();
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
pub async fn fetch_sources(app: AppHandle, ids: Option<Vec<i64>>) -> Result<usize, String> {
    crate::refresh_all_sources(app, ids, false).await
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
pub async fn mark_read(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<i64>,
    read: bool,
) -> Result<(), String> {
    let acct = settings_io::load(&settings_io::settings_path(&app)?).sync_account;
    let conn = state.db.lock().await;
    db::set_items_read(&conn, &ids, read)?;
    if acct.is_some() {
        let action = if read { "mark_read" } else { "mark_unread" };
        db::enqueue_item_actions(&conn, &ids, action)?;
    }
    drop(conn);
    crate::tray::update_tray(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn mark_all_read(
    app: AppHandle,
    state: State<'_, AppState>,
    scope: Option<String>,
    scope_id: Option<i64>,
) -> Result<(), String> {
    let acct = settings_io::load(&settings_io::settings_path(&app)?).sync_account;
    let conn = state.db.lock().await;
    db::mark_all_read(&conn, scope.as_deref(), scope_id)?;
    if acct.as_ref().is_some_and(|a| a.provider == "greader") {
        // Map the scope to a remote stream and queue the server-side mark-all.
        let stream = match scope.as_deref() {
            Some("source") => db::get_source(&conn, scope_id.unwrap_or(-1))?.remote_id,
            Some("group") => db::get_group(&conn, scope_id.unwrap_or(-1))?
                .map(|g| format!("user/-/label/{}", g.name)),
            _ => Some(crate::greader::STREAM_READING_LIST.to_string()),
        };
        if let Some(target) = stream.filter(|t| !t.is_empty()) {
            db::enqueue_stream_action(&conn, "mark_all_read", &target)?;
        }
    }
    drop(conn);
    crate::tray::update_tray(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn star(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    starred: bool,
) -> Result<(), String> {
    let acct = settings_io::load(&settings_io::settings_path(&app)?).sync_account;
    let conn = state.db.lock().await;
    db::set_item_starred(&conn, id, starred)?;
    if acct.is_some() {
        let action = if starred { "star" } else { "unstar" };
        db::enqueue_item_actions(&conn, &[id], action)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_item_hidden(state: State<'_, AppState>, id: i64, hidden: bool) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::set_item_hidden(&conn, id, hidden)
}

#[tauri::command]
pub async fn fetch_full_content(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let client = state.http_client();
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
pub async fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let path = settings_io::settings_path(&app)?;
    let old = settings_io::load(&path);
    let proxy_changed = old.proxy_mode != settings.proxy_mode
        || old.proxy_url != settings.proxy_url
        || old.proxy_username != settings.proxy_username
        || old.proxy_password != settings.proxy_password;
    settings_io::save(&path, &settings)?;
    if proxy_changed {
        state.set_http_client(crate::net::build_http_client(&settings));
    }
    Ok(())
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

// ---------- Phase 2: proxy ----------

/// Probe connectivity with candidate proxy settings (before they are saved).
/// Returns the request latency in milliseconds.
#[tauri::command]
pub async fn test_proxy(settings: Settings) -> Result<u64, String> {
    let client = crate::net::build_http_client(&settings);
    let start = std::time::Instant::now();
    client
        .get("https://example.com")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    Ok(start.elapsed().as_millis() as u64)
}

// ---------- Phase 2.3: cloud sync (Google Reader API) ----------

/// Validate credentials against the server and store the account on success.
/// Returns the number of subscriptions on the server.
#[tauri::command]
pub async fn sync_login(
    app: AppHandle,
    state: State<'_, AppState>,
    server_url: String,
    username: String,
    password: String,
) -> Result<usize, String> {
    let acct = crate::models::SyncAccount {
        provider: "greader".into(),
        server_url: server_url.trim().trim_end_matches('/').to_string(),
        username: username.trim().to_string(),
        password,
    };
    if acct.server_url.is_empty() || acct.username.is_empty() || acct.password.is_empty() {
        return Err("server URL, username and password are required".into());
    }
    let http = state.http_client();
    let auth = crate::sync::ensure_session(&state, &http, &acct)
        .await
        .map_err(|e| e.to_string())?;
    let subs = crate::greader::subscriptions(&http, &acct.server_url, &auth)
        .await
        .map_err(|e| e.to_string())?;

    let path = settings_io::settings_path(&app)?;
    let mut s = settings_io::load(&path);
    s.sync_account = Some(acct);
    settings_io::save(&path, &s)?;
    Ok(subs.len())
}

/// Disconnect the account. Local data is kept; queued (unpushed) actions are
/// dropped because their target server is gone.
#[tauri::command]
pub async fn sync_logout(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let path = settings_io::settings_path(&app)?;
    let mut s = settings_io::load(&path);
    s.sync_account = None;
    settings_io::save(&path, &s)?;
    crate::sync::clear_session(&state);
    {
        let conn = state.db.lock().await;
        db::queue_clear(&conn)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn sync_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let last_sync = {
        let conn = state.db.lock().await;
        db::get_state(&conn, "greader.last_sync")?
    };
    let queue_len = {
        let conn = state.db.lock().await;
        db::queue_len(&conn)?
    };
    Ok(serde_json::json!({
        "lastSync": last_sync.and_then(|v| v.parse::<i64>().ok()),
        "queueLen": queue_len,
    }))
}

/// Run one manual sync cycle.
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<serde_json::Value, String> {
    let r = crate::sync::run(&app, false).await?;
    Ok(serde_json::json!({
        "newItems": r.new_items,
        "pushed": r.pushed,
        "failures": r.failures,
        "subscriptions": r.subscription_count,
    }))
}

// ---------- Phase 2: regex rules ----------

fn validate_rule_input(r: &crate::models::RuleInput) -> Result<(), String> {
    if r.name.trim().is_empty() {
        return Err("rule name is empty".into());
    }
    if r.pattern.is_empty() {
        return Err("pattern is empty".into());
    }
    if !crate::rules::valid_target(&r.target_field) {
        return Err(format!("invalid target field: {}", r.target_field));
    }
    if !crate::rules::valid_action(&r.action_type) {
        return Err(format!("invalid action: {}", r.action_type));
    }
    if !crate::rules::valid_scope(&r.source_scope) {
        return Err(format!("invalid source scope: {}", r.source_scope));
    }
    let probe = crate::models::Rule {
        id: 0,
        name: String::new(),
        pattern: r.pattern.clone(),
        target_field: r.target_field.clone(),
        action_type: r.action_type.clone(),
        is_case_sensitive: r.is_case_sensitive,
        is_enabled: r.is_enabled,
        source_scope: r.source_scope.clone(),
        created_at: 0,
    };
    crate::rules::compile_pattern(&probe)?;
    Ok(())
}

#[tauri::command]
pub async fn get_rules(state: State<'_, AppState>) -> Result<Vec<crate::models::Rule>, String> {
    let conn = state.db.lock().await;
    db::get_rules(&conn)
}

#[tauri::command]
pub async fn create_rule(
    state: State<'_, AppState>,
    input: crate::models::RuleInput,
) -> Result<crate::models::Rule, String> {
    validate_rule_input(&input)?;
    let conn = state.db.lock().await;
    db::create_rule(&conn, &input)
}

#[tauri::command]
pub async fn update_rule(
    state: State<'_, AppState>,
    id: i64,
    input: crate::models::RuleInput,
) -> Result<(), String> {
    validate_rule_input(&input)?;
    let conn = state.db.lock().await;
    db::update_rule(&conn, id, &input)
}

#[tauri::command]
pub async fn delete_rule(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::delete_rule(&conn, id)
}

/// Re-run all enabled rules over the whole article archive.
#[tauri::command]
pub async fn apply_rules_backfill(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let conn = state.db.lock().await;
    let engine = crate::rules::RuleEngine::load(&conn)?;
    let stats = crate::rules::backfill(&conn, &engine)?;
    Ok(serde_json::json!({
        "markedRead": stats.marked_read,
        "starred": stats.starred,
        "hidden": stats.hidden,
        "notified": stats.notified,
    }))
}

// ---------- Phase 2: backup & restore ----------

#[tauri::command]
pub async fn export_backup(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let dir = data_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let app_for_dialog = app.clone();
    let default_name = format!(
        "zreader-backup-{}.zreader.bak",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let target = tauri::async_runtime::spawn_blocking(move || {
        app_for_dialog
            .dialog()
            .file()
            .add_filter("ZReader Backup", &["zreader.bak"])
            .set_file_name(default_name)
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(target) = target else {
        return Ok(None); // dialog cancelled
    };
    let out_path = target.into_path().map_err(|e| e.to_string())?;

    let snapshot = dir.join("backup-snapshot.db");
    {
        let conn = state.db.lock().await;
        crate::backup::snapshot_live(&conn, &snapshot)?;
    }
    let settings_file = settings_io::settings_path(&app)?;
    let favicon_dir = dir.join("favicons");
    let result = crate::backup::write_archive(&snapshot, Some(&settings_file), Some(&favicon_dir), &out_path);
    let _ = std::fs::remove_file(&snapshot);
    result?;
    Ok(Some(out_path.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn import_backup(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let app_for_dialog = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app_for_dialog
            .dialog()
            .file()
            .add_filter("ZReader Backup", &["zreader.bak"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(picked) = picked else {
        return Ok(None); // dialog cancelled
    };
    let archive_path = picked.into_path().map_err(|e| e.to_string())?;

    let dir = data_dir(&app)?;
    let tmp_dir = dir.join("restore-tmp");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    crate::backup::extract_archive(&archive_path, &tmp_dir)?;

    let restored_db = tmp_dir.join(crate::backup::DB_ENTRY);
    if !restored_db.exists() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err("backup archive does not contain zreader.db".into());
    }
    crate::backup::validate_db(&restored_db)?;

    // settings.json is restored before the DB swap so the frontend reload sees it.
    let restored_settings = tmp_dir.join(crate::backup::SETTINGS_ENTRY);
    if restored_settings.exists() {
        let settings_file = settings_io::settings_path(&app)?;
        std::fs::copy(&restored_settings, &settings_file).map_err(|e| e.to_string())?;
    }

    // Favicons are plain files; copy them back over the live ones.
    let restored_favicons = tmp_dir.join("favicons");
    if restored_favicons.is_dir() {
        let fav_dir = dir.join("favicons");
        std::fs::create_dir_all(&fav_dir).map_err(|e| e.to_string())?;
        if let Ok(entries) = std::fs::read_dir(&restored_favicons) {
            for entry in entries.flatten() {
                let from = entry.path();
                if let Some(name) = from.file_name() {
                    let _ = std::fs::copy(&from, fav_dir.join(name));
                }
            }
        }
    }

    // Swap the database: drop the live connection, replace files, reopen
    // (db::open runs migrations, so v1 backups are upgraded in place).
    {
        let mut guard = state.db.lock().await;
        let db_path = state.db_path.clone();
        *guard = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
        std::fs::copy(&restored_db, &db_path).map_err(|e| e.to_string())?;
        *guard = crate::db::open(&db_path)?;
    }
    let _ = std::fs::remove_dir_all(&tmp_dir);

    crate::tray::update_tray(&app).await;
    use tauri::Emitter;
    let _ = app.emit("data-restored", ());
    Ok(Some(archive_path.to_string_lossy().to_string()))
}

// ---------- Phase 2: stats & storage lifecycle ----------

#[tauri::command]
pub async fn get_stats(_app: AppHandle, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (articles, unread) = {
        let conn = state.db.lock().await;
        (db::item_count(&conn)?, db::total_unread(&conn)?)
    };
    let db_size = std::fs::metadata(&state.db_path).map(|m| m.len()).unwrap_or(0);
    Ok(serde_json::json!({
        "articles": articles,
        "unread": unread,
        "dbSize": db_size,
    }))
}

#[tauri::command]
pub async fn vacuum_now(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().await;
    db::vacuum(&conn)
}

/// Apply the retention policy immediately, then compact the database.
#[tauri::command]
pub async fn cleanup_now(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    let s = settings_io::load(&settings_io::settings_path(&app)?);
    let deleted = {
        let conn = state.db.lock().await;
        db::cleanup_retention(&conn, s.retention_days, s.max_items_per_source)?
    };
    {
        let conn = state.db.lock().await;
        db::vacuum(&conn)?;
    }
    crate::tray::update_tray(&app).await;
    Ok(deleted)
}
