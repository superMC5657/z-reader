use crate::db;
use crate::feed;
use crate::models::{GetItemsParams, Settings, SyncAction};
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

/// First line of an error message, truncated: reasons never carry file paths,
/// URLs, tokens or article text.
fn short_reason(msg: &str) -> String {
    const MAX_CHARS: usize = 160;
    let first = msg.lines().next().unwrap_or("").trim();
    if first.chars().count() > MAX_CHARS {
        first.chars().take(MAX_CHARS).collect()
    } else {
        first.to_string()
    }
}

/// Basename only: full file paths never enter logs.
fn file_base(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
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
    log::debug!("[CMD] action=create_group");
    let conn = state.db.lock().await;
    db::create_group(&conn, &name)
}

#[tauri::command]
pub async fn rename_group(state: State<'_, AppState>, id: i64, name: String) -> Result<(), String> {
    log::debug!("[CMD] action=rename_group id={id}");
    let conn = state.db.lock().await;
    db::rename_group(&conn, id, &name)
}

#[tauri::command]
pub async fn delete_group(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    log::debug!("[CMD] action=delete_group id={id}");
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
    // Log host only: full URLs can carry tokens/query secrets.
    let (host, _) = crate::net::sanitize_url(&url);
    log::info!("[CMD] action=add_source host={host}");
    let client = state.http_client();

    {
        let conn = state.db.lock().await;
        if let Some(s) = db::get_source_by_url(&conn, &url)? {
            return Err(format!("source already exists: {}", s.title));
        }
    }

    // Probe without persisting: parse the remote feed to validate first.
    let parsed = feed::fetch_and_parse(&client, &url).await.inspect_err(|e| {
        log::warn!("[CMD] action=add_source host={host} failed reason={}", short_reason(e));
    })?;
    let title = if parsed.title.is_empty() { url.clone() } else { parsed.title.clone() };

    let source = {
        let conn = state.db.lock().await;
        let s = db::insert_source(&conn, &url, &title, parsed.description.as_deref(), group_id).inspect_err(|e| {
            log::warn!("[CMD] action=add_source host={host} failed reason={}", short_reason(e));
        })?;
        let ctx = feed::SourceCtx { id: s.id, group_id: s.group_id, url: url.clone() };
        feed::store(&conn, &ctx, &parsed, None).inspect_err(|e| {
            log::warn!("[CMD] action=add_source host={host} failed reason={}", short_reason(e));
        })?;
        db::mark_source_fetched(&conn, s.id, true, None)?;
        db::get_source(&conn, s.id)?
    };
    if source.favicon.is_none() {
        let dir = favicon_dir(&app)?;
        let third_party =
            settings_io::load(&settings_io::settings_path(&app)?).favicon_third_party;
        let icon_url = parsed.icon_url.as_deref();
        let site_url = parsed.site_url.as_deref();
        if let Some(fav) =
            feed::fetch_favicon(&client, &url, icon_url, site_url, &dir, source.id, third_party).await
        {
            let conn = state.db.lock().await;
            db::set_source_favicon(&conn, source.id, fav.to_string_lossy().as_ref())?;
        }
    }
    log::info!("[CMD] action=add_source ok id={} host={host}", source.id);
    Ok(source)
}

#[tauri::command]
pub async fn remove_source(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    log::debug!("[CMD] action=remove_source id={id}");
    let conn = state.db.lock().await;
    db::remove_source(&conn, id)
}

#[tauri::command]
pub async fn rename_source(state: State<'_, AppState>, id: i64, title: String) -> Result<(), String> {
    log::debug!("[CMD] action=rename_source id={id}");
    let conn = state.db.lock().await;
    db::rename_source(&conn, id, &title)
}

#[tauri::command]
pub async fn set_source_group(state: State<'_, AppState>, id: i64, group_id: Option<i64>) -> Result<(), String> {
    log::debug!("[CMD] action=set_source_group id={id}");
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
    // Extension comes from the payload, never the data-URL hint: SVG and
    // non-image uploads are rejected instead of being stored.
    let Some(ext) = crate::feed::sniff_image_ext(&bytes) else {
        return Err("unsupported image format (SVG is not accepted)".into());
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
    let third_party = settings_io::load(&settings_io::settings_path(&app)?).favicon_third_party;
    let icon_url = parsed.as_ref().and_then(|p| p.icon_url.as_deref());
    let site_url = parsed.as_ref().and_then(|p| p.site_url.as_deref());
    if let Some(fav) =
        feed::fetch_favicon(&client, &url, icon_url, site_url, &dir, id, third_party).await
    {
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
    // Count only, no titles: article titles never enter logs.
    log::debug!("[CMD] action=mark_read count={} read={read}", ids.len());
    let acct = settings_io::load(&settings_io::settings_path(&app)?).sync_account;
    let conn = state.db.lock().await;
    db::set_items_read(&conn, &ids, read)?;
    if acct.is_some() {
        let action = if read { SyncAction::MarkRead } else { SyncAction::MarkUnread };
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
    // Scope ids only: feed/folder names never enter logs.
    log::debug!(
        "[CMD] action=mark_all_read scope={} scope_id={:?}",
        scope.as_deref().unwrap_or("all"),
        scope_id
    );
    let acct = settings_io::load(&settings_io::settings_path(&app)?).sync_account;
    let conn = state.db.lock().await;
    db::mark_all_read(&conn, scope.as_deref(), scope_id)?;
    if acct.as_ref().is_some_and(|a| a.provider == "greader") {
        // Map the scope to a remote stream and queue the server-side mark-all.
        // Only streams the server actually knows (remote_id) are reported;
        // fabricating a label stream for never-synced groups would push a
        // bogus target.
        let stream = match scope.as_deref() {
            Some("source") => db::get_source(&conn, scope_id.unwrap_or(-1))?.remote_id,
            Some("group") => db::get_group(&conn, scope_id.unwrap_or(-1))?
                .and_then(|g| g.remote_id),
            _ => Some(crate::greader::STREAM_READING_LIST.to_string()),
        };
        if let Some(target) = stream.filter(|t| !t.is_empty()) {
            db::enqueue_stream_action(&conn, SyncAction::MarkAllRead, &target)?;
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
    log::debug!("[CMD] action=star id={id} starred={starred}");
    let acct = settings_io::load(&settings_io::settings_path(&app)?).sync_account;
    let conn = state.db.lock().await;
    db::set_item_starred(&conn, id, starred)?;
    if acct.is_some() {
        let action = if starred { SyncAction::Star } else { SyncAction::Unstar };
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
    let link = {
        let conn = state.db.lock().await;
        let item = db::get_item(&conn, id)?;
        item.url.ok_or("item has no link")?
    };
    log::debug!("[CMD] action=fetch_full_content id={id}");
    match fetch_full_content_inner(&state, id, &link).await {
        Ok(chars) => {
            log::debug!("[CMD] action=fetch_full_content ok id={id} chars={chars}");
            Ok(())
        }
        Err(e) => {
            log::warn!("[CMD] action=fetch_full_content failed id={id} reason={}", short_reason(&e));
            Err(e)
        }
    }
}

async fn fetch_full_content_inner(state: &AppState, id: i64, link: &str) -> Result<usize, String> {
    let client = state.http_client();
    let content = crate::extractor::extract_from_url(&client, link).await?;
    let snippet = crate::extractor::snippet_of(&content);
    let chars = content.chars().count();
    let conn = state.db.lock().await;
    db::set_item_content(&conn, id, &content, &snippet)?;
    Ok(chars)
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
    // No field details: proxy credentials/URLs never enter logs.
    log::debug!("[CMD] action=save_settings");
    crate::net::validate_proxy(&settings)?;
    let path = settings_io::settings_path(&app)?;
    let old = settings_io::load(&path);
    let proxy_changed = old.proxy_mode != settings.proxy_mode
        || old.proxy_url != settings.proxy_url
        || old.proxy_username != settings.proxy_username
        || old.proxy_password != settings.proxy_password;
    let locale_changed = old.locale != settings.locale;
    settings_io::save(&path, &settings)?;
    if proxy_changed {
        state.set_http_client(crate::net::build_http_client(&settings));
    }
    if locale_changed {
        // Tray menu labels are baked at creation; refresh them so the
        // language switch applies outside the main window too.
        crate::tray::update_tray(&app).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn import_opml(state: State<'_, AppState>, text: String) -> Result<serde_json::Value, String> {
    log::debug!("[CMD] action=import_opml");
    let conn = state.db.lock().await;
    match opml_io::import(&conn, &text) {
        Ok(r) => {
            log::debug!(
                "[CMD] action=import_opml ok groups={} sources={} existing={}",
                r.groups_added,
                r.sources_added,
                r.sources_existing
            );
            Ok(serde_json::json!({
                "groupsAdded": r.groups_added,
                "sourcesAdded": r.sources_added,
                "sourcesExisting": r.sources_existing,
            }))
        }
        Err(e) => {
            log::warn!("[CMD] action=import_opml failed reason={}", short_reason(&e));
            Err(e)
        }
    }
}

/// Returns the OPML document as XML text for the frontend to download.
#[tauri::command]
pub async fn export_opml(state: State<'_, AppState>) -> Result<String, String> {
    let conn = state.db.lock().await;
    match opml_io::export(&conn) {
        Ok(xml) => {
            log::debug!("[CMD] action=export_opml ok");
            Ok(xml)
        }
        Err(e) => {
            log::error!("[CMD] action=export_opml failed reason={}", short_reason(&e));
            Err(e)
        }
    }
}

// ---------- Proxy ----------

/// Probe connectivity with candidate proxy settings (before they are saved).
/// `target` is the URL actually fetched: the caller passes a failing feed URL
/// when one exists so the test reflects real conditions, else a default probe.
/// Returns the request latency in milliseconds.
#[tauri::command]
pub async fn test_proxy(settings: Settings, target: Option<String>) -> Result<u64, String> {
    log::debug!("[CMD] action=test_proxy");
    if let Err(e) = crate::net::validate_proxy(&settings) {
        log::warn!("[CMD] action=test_proxy failed reason={}", short_reason(&e));
        return Err(e);
    }
    let url = target
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or("https://example.com");
    let client = crate::net::build_http_client(&settings);
    let start = std::time::Instant::now();
    let req = client.get(url).timeout(std::time::Duration::from_secs(10));
    let resp = crate::net::send_logged("proxy", "GET", url, req).await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            let msg = crate::net::http_err_reason("", &e);
            log::warn!("[CMD] action=test_proxy failed reason={}", short_reason(&msg));
            return Err(msg);
        }
    };
    if let Err(e) = resp.error_for_status() {
        let msg = crate::net::http_err_reason("", &e);
        log::warn!("[CMD] action=test_proxy failed reason={}", short_reason(&msg));
        return Err(msg);
    }
    let ms = start.elapsed().as_millis() as u64;
    log::debug!("[CMD] action=test_proxy ok latency={ms}ms");
    Ok(ms)
}

// ---------- Cloud Sync (Google Reader API) ----------

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
        .map_err(|e| {
            let msg = e.to_string();
            log::warn!("sync login subscriptions failed reason {}", short_reason(&msg));
            msg
        })?;

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
    // Best effort: a stale account password must not survive next to logout.
    let _ = settings_io::clear_sync_secret(&path);
    crate::sync::clear_session(&state);
    {
        let conn = state.db.lock().await;
        db::queue_clear(&conn)?;
    }
    log::info!("[CMD] action=sync_logout provider=greader");
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
pub async fn sync_now(app: AppHandle, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    log::info!("[CMD] action=sync_now");
    let r = crate::sync::run(&app, false).await?;
    let settings = settings_io::load(&settings_io::settings_path(&app)?);
    {
        let conn = state.db.lock().await;
        match db::cleanup_retention(&conn, settings.retention_days, settings.max_items_per_source) {
            Ok(n) => {
                if n > 200 {
                    let _ = db::vacuum(&conn);
                }
                if n > 0 {
                    log::info!("[CMD] action=sync_now retention deleted={n}");
                }
            }
            Err(e) => {
                log::warn!("[CMD] action=sync_now retention failed reason={}", short_reason(&e));
            }
        }
    }
    crate::tray::update_tray(&app).await;
    use tauri::Emitter;
    let _ = app.emit(
        "fetch-done",
        serde_json::json!({
            "newItems": r.new_items,
            "failures": r.failures,
            "background": false,
            "sync": true,
        }),
    );
    Ok(serde_json::json!({
        "newItems": r.new_items,
        "pushed": r.pushed,
        "failures": r.failures,
        "subscriptions": r.subscription_count,
    }))
}

// ---------- Regex Automation Rules ----------

fn validate_rule_input(r: &crate::models::RuleInput) -> Result<(), String> {
    if r.name.trim().is_empty() {
        return Err("rule name is empty".into());
    }
    if r.pattern.is_empty() {
        return Err("pattern is empty".into());
    }
    let probe = crate::models::Rule {
        id: 0,
        name: String::new(),
        pattern: r.pattern.clone(),
        target_field: r.target_field,
        action_type: r.action_type,
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
    log::debug!("[CMD] action=create_rule");
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
    log::debug!("[CMD] action=update_rule id={id}");
    validate_rule_input(&input)?;
    let conn = state.db.lock().await;
    db::update_rule(&conn, id, &input)
}

#[tauri::command]
pub async fn delete_rule(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    log::debug!("[CMD] action=delete_rule id={id}");
    let conn = state.db.lock().await;
    db::delete_rule(&conn, id)
}

/// Re-run all enabled rules over the whole article archive.
#[tauri::command]
pub async fn apply_rules_backfill(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    log::debug!("[CMD] action=apply_rules_backfill");
    let conn = state.db.lock().await;
    let engine = match crate::rules::RuleEngine::load(&conn) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("[CMD] action=apply_rules_backfill failed reason {}", short_reason(&e));
            return Err(e);
        }
    };
    match crate::rules::backfill(&conn, &engine) {
        Ok(stats) => {
            log::info!(
                "[CMD] action=apply_rules_backfill ok read={} star={} hide={} notify={}",
                stats.marked_read,
                stats.starred,
                stats.hidden,
                stats.notified
            );
            Ok(serde_json::json!({
                "markedRead": stats.marked_read,
                "starred": stats.starred,
                "hidden": stats.hidden,
                "notified": stats.notified,
            }))
        }
        Err(e) => {
            log::warn!("[CMD] action=apply_rules_backfill failed reason {}", short_reason(&e));
            Err(e)
        }
    }
}

// ---------- Backup & Restore ----------

#[tauri::command]
pub async fn export_backup(app: AppHandle, state: State<'_, AppState>) -> Result<Option<String>, String> {
    log::info!("[CMD] action=export_backup");
    match export_backup_inner(app, &state).await {
        Ok(Some(path)) => {
            log::info!("[CMD] action=export_backup ok file={}", file_base(&path));
            Ok(Some(path))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            log::error!("[CMD] action=export_backup failed reason={}", short_reason(&e));
            Err(e)
        }
    }
}

async fn export_backup_inner(app: AppHandle, state: &AppState) -> Result<Option<String>, String> {
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
    log::info!("[CMD] action=import_backup");
    match import_backup_inner(app, &state).await {
        Ok(Some(path)) => {
            log::info!("[CMD] action=import_backup ok file={}", file_base(&path));
            Ok(Some(path))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            log::warn!("[CMD] action=import_backup failed reason={}", short_reason(&e));
            Err(e)
        }
    }
}

async fn import_backup_inner(app: AppHandle, state: &AppState) -> Result<Option<String>, String> {
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

    // Stage the validated DB next to the live file (outside the lock so a
    // slow copy doesn't block readers) and re-validate the staged copy.
    let db_path = state.db_path.clone();
    let staged = db_path.with_extension("db.restoring");
    std::fs::copy(&restored_db, &staged).map_err(|e| e.to_string())?;
    crate::backup::validate_db(&staged)?;

    // Snapshot the live DB for rollback (best effort; restore proceeds anyway).
    let rollback = db_path.with_extension("db.pre-restore-bak");
    {
        let conn = state.db.lock().await;
        let _ = crate::backup::snapshot_live(&conn, &rollback);
    }

    // Swap the database: drop the live connection, replace files, reopen
    // (db::open runs migrations, so v1 backups are upgraded in place).
    // Any failure restores the pre-restore snapshot, so the live data is
    // never left in a half-replaced state.
    {
        let mut guard = state.db.lock().await;
        *guard = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
        let swapped: Result<(), String> = (|| {
            let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
            let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
            // Same-directory rename/copy is the atomic point; see replace_live.
            crate::backup::replace_live(&staged, &db_path)?;
            *guard = crate::db::open(&db_path)?;
            Ok(())
        })();
        if let Err(e) = swapped {
            let _ = std::fs::copy(&rollback, &db_path);
            match crate::db::open(&db_path) {
                Ok(conn) => *guard = conn,
                Err(reopen) => {
                    log::error!(
                        "[CMD] action=import_backup rollback reopen failed reason={}",
                        short_reason(&reopen)
                    );
                }
            }
            let _ = std::fs::remove_file(&staged);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            log::warn!(
                "[CMD] action=import_backup rollback file={} reason={}",
                file_base(&rollback.to_string_lossy()),
                short_reason(&e)
            );
            return Err(e);
        }
    }

    // Settings + favicons land only after the DB swap succeeded.
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

    let _ = std::fs::remove_dir_all(&tmp_dir);

    crate::tray::update_tray(&app).await;
    use tauri::Emitter;
    let _ = app.emit("data-restored", ());
    Ok(Some(archive_path.to_string_lossy().to_string()))
}

// ---------- Storage Lifecycle & Stats ----------

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
    log::info!("[CMD] action=vacuum_now");
    let conn = state.db.lock().await;
    db::vacuum(&conn)?;
    log::info!("[CMD] action=vacuum_now ok");
    Ok(())
}

/// Apply the retention policy immediately, compacting only when something
/// was actually deleted.
#[tauri::command]
pub async fn cleanup_now(app: AppHandle, state: State<'_, AppState>) -> Result<usize, String> {
    let s = settings_io::load(&settings_io::settings_path(&app)?);
    let deleted = {
        let conn = state.db.lock().await;
        db::cleanup_retention(&conn, s.retention_days, s.max_items_per_source)?
    };
    if deleted > 0 {
        let conn = state.db.lock().await;
        db::vacuum(&conn)?;
    }
    crate::tray::update_tray(&app).await;
    log::info!("[CMD] action=cleanup_now ok deleted={deleted}");
    Ok(deleted)
}
