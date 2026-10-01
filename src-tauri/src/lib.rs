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
mod services;
mod settings;
mod sync;
mod tray;
mod utils;
mod z_log;

use std::sync::RwLock;
use tokio::sync::Mutex;

use crate::services::source::SourceRefreshOutcome;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    pub db_path: std::path::PathBuf,
    /// 可替换的 HTTP 客户端：在代理设置变更时重新构建。
    pub http: RwLock<reqwest::Client>,
    /// 内存中的云同步登录会话（绝不持久化）。
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
                log::error!("tray create failed reason {}", crate::utils::short_reason(&e.to_string()));
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
            commands::cleanup_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 后台轮询循环：每隔 `fetchInterval` 分钟刷新所有订阅源。
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

/// 刷新周期中允许的最大并发订阅源抓取数。
const REFRESH_CONCURRENCY: usize = 6;

/// 后台刷新链路的站点图标目录解析。
///
/// 与新增订阅源（`commands::add_source`）、手动刷新图标（`commands::refresh_favicon`）
/// 共用 `commands::favicon_dir` 作为唯一的目录拼接点，本模块内统一经由此辅助函数
/// 获取，避免各处重复手写 `join("favicons")`。
fn refresh_favicon_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    crate::commands::favicon_dir(app)
}

/// 单个刷新任务所需的所有数据；具有所有权以保证任务为 `'static`。
/// 注意：此处不包含标题/URL —— 日志仅输出 ID 和首行错误原因。
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

/// 刷新单个订阅源：委托 `services::source::refresh_single_source` 执行业务流程
///（抓取 + 解析、存储 + 标记、缺失站点图标回退补全，含分段耗时日志）。
/// 本函数仅负责前端 `fetch-progress` 事件收发；业务错误绝不向上抛出，
/// 错误会记录在该源的数据行并通过结果返回。
async fn refresh_one_source(task: RefreshTask) -> SourceRefreshOutcome {
    use tauri::{Emitter, Manager};
    let RefreshTask { app, client, engine, favicon_dir, background, allow_third_party, id, group_id, url, favicon, cycle } = task;
    if !background {
        let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": false }));
    }
    let state = app.state::<AppState>();
    let out = crate::services::source::refresh_single_source(
        crate::services::source::RefreshSingleSourceContext {
            client: &client,
            db: &state.db,
            engine: Some(engine.as_ref()),
            source_id: id,
            group_id,
            url: &url,
            favicon: favicon.as_deref(),
            favicon_dir: Some(favicon_dir.as_path()),
            allow_third_party,
            cycle: &cycle,
        },
    )
    .await
    .unwrap_or_else(|e| {
        log::warn!("feed refresh failed id={id} reason {}", crate::utils::short_reason(&e));
        SourceRefreshOutcome { inserted: 0, notified: Vec::new(), failed: true, error_kind: Some("other".to_string()) }
    });
    if !background {
        let _ = app.emit("fetch-progress", serde_json::json!({ "sourceId": id, "done": true }));
    }
    out
}

/// 将单个订阅源的刷新结果累加到刷新周期总计中。
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

/// 抓取所有（或选定的）订阅源，将新条目送入规则引擎处理、
/// 存储条目、刷新缺失的站点图标、应用保留策略并同步托盘红点徽标。向前端发送 fetch-progress / fetch-done 事件。
///
/// `background` = 定时器触发：不发送单源进度事件，并在有新文章（或规则命中“通知”）到达时发出聚合的桌面通知。
pub async fn refresh_all_sources(
    app: tauri::AppHandle,
    ids: Option<Vec<i64>>,
    background: bool,
) -> Result<usize, String> {
    use tauri::{Emitter, Manager};
    let state = app.state::<AppState>();
    let settings = settings::load(&settings::settings_path(&app)?);

    // 云同步模式：订阅源保存在服务端，因此刷新即等于同步。
    // 拉取过程中由规则引擎命中的通知将被转发，使后台通知行为与本地刷新路径保持一致。
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
                    log::warn!("refresh rules load failed cycle={cycle} error_kind={error_kind} reason {}", crate::utils::short_reason(&e));
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
        let dir = refresh_favicon_dir(&app)?;
        let feed_count = targets.len();
        log::info!("refresh start cycle={cycle} mode={mode} sync=false feeds={feed_count}");

        // 有界并发刷新：慢速订阅源不再阻塞其余源。
        // 每个任务独占其网络 I/O，仅在存储/标记条目的极短临界区内获取数据库锁。
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
                    log::debug!("refresh task join failed cycle={cycle} reason {}", crate::utils::short_reason(&e.to_string()));
                }
            }
        }
        (total_new, failures, notified, feed_count, fail_kinds)
    };

    // 保留策略清理委托存储服务；阈值语义不变：删除数严格大于 200 时执行 VACUUM。
    // 计数收敛为单行输出：仅在确实有行被删除时输出 info 级别日志（均由服务内部完成）。
    {
        let conn = state.db.lock().await;
        crate::services::storage::run_retention_policy(
            &conn,
            settings.retention_days,
            settings.max_items_per_source,
            200,
        );
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
    // 单行汇总周期摘要，附带 fail_kinds 失败分类明细（仅在存在失败时输出；通知仅输出数量，不输出文章标题）。
    let fail_suffix = crate::utils::format_fail_kinds(&fail_kinds);
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
    let zh = crate::utils::is_zh_locale(&settings.locale);
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
        // 设定：混合包含成功、触发通知以及失败的单源刷新结果
        let outcomes = vec![
            SourceRefreshOutcome { inserted: 3, notified: vec!["a".into()], failed: false, error_kind: None },
            SourceRefreshOutcome { inserted: 0, notified: vec![], failed: true, error_kind: Some("timeout".into()) },
            SourceRefreshOutcome { inserted: 2, notified: vec!["b".into(), "c".into()], failed: false, error_kind: None },
        ];
        // 操作：汇总折叠
        let (mut total, mut failures, mut notified) = (0usize, 0usize, Vec::new());
        let mut fail_kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for out in outcomes {
            merge_outcome(&mut total, &mut failures, &mut notified, &mut fail_kinds, out);
        }
        // 验证：新增数求和，失败数统计，文章标题按顺序拼接
        assert_eq!(total, 5);
        assert_eq!(failures, 1);
        assert_eq!(notified, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(fail_kinds.get("timeout"), Some(&1));
    }
}
