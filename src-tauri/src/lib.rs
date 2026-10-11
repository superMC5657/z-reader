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
/// 唯一出处为刷新生命周期 Module 的 [`crate::services::source::REFRESH_CONCURRENCY`] Seam；
/// 此处仅为单入口分流循环的薄引用，不再各自硬编码。
const REFRESH_CONCURRENCY: usize = crate::services::source::REFRESH_CONCURRENCY;

/// 后台刷新链路的站点图标目录解析。
///
/// 与新增订阅源（`commands::add_source`）、手动刷新图标（`commands::refresh_favicon`）
/// 共用 `commands::favicon_dir` 作为唯一的目录拼接点，本模块内统一经由此辅助函数
/// 获取，避免各处重复手写 `join("favicons")`。
fn refresh_favicon_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    crate::commands::favicon_dir(app)
}

/// 单个刷新任务的数据（事件所需最小字段 + 业务输入）；不携带 `AppHandle`/`background`，
/// 生命周期渗漏由调用方显式参数承担。
/// 本 struct 仅为单入口分流循环的事件 Seam 载体；业务上下文经由刷新 Module 的
/// `RefreshSingleSourceContext` Interface 进入，抓取/存库/图标均走 Module Seam（Adapter 不直连）。
struct RefreshTask {
    id: i64,
    group_id: Option<i64>,
    url: String,
    favicon: Option<String>,
    cycle: String,
    client: reqwest::Client,
    engine: std::sync::Arc<rules::RuleEngine>,
    favicon_dir: std::path::PathBuf,
    allow_third_party: bool,
}

/// 刷新单个订阅源：刷新 Module 单源 Interface 的事件 Seam 薄包装。
/// 业务委托 `services::source` 两段式流程（抓取段 + 存库段，含分段耗时日志）；
/// 本函数仅负责前端 `fetch-progress` 事件收发；业务错误绝不向上抛出，
/// 错误会记录在该源的数据行并通过结果返回。
async fn refresh_one_source(
    app: tauri::AppHandle,
    task: RefreshTask,
    background: bool,
) -> SourceRefreshOutcome {
    use tauri::{Emitter, Manager};
    let RefreshTask { id, group_id, url, favicon, cycle, client, engine, favicon_dir, allow_third_party } = task;
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

/// 刷新目标组装（纯）：全量或按 `ids` 子集映射为任务元组。
/// 唯一实现收归刷新 Module 的 `build_targets` Interface；此处仅做
/// `RefreshTarget`→元组的薄适配，保持调用方与单测签名不变。
fn build_targets(
    sources: &[crate::models::Source],
    ids: Option<&[i64]>,
) -> Vec<(i64, Option<i64>, String, Option<String>)> {
    crate::services::source::build_targets(sources, ids)
        .into_iter()
        .map(|t| (t.id, t.group_id, t.url, t.favicon))
        .collect()
}

/// 是否弹出聚合桌面通知（纯）：仅后台周期且（新文章命中开关或规则通知非空）时为真。
/// 唯一实现收归刷新 Module 的 `should_notify` Interface；此处为薄委托。
fn should_notify(
    background: bool,
    notify_on_new: bool,
    new_count: usize,
    notified: &[String],
) -> bool {
    crate::services::source::should_notify(background, notify_on_new, new_count, notified)
}

/// 通知正文（纯，沿用既有 zh/en 文案）：空标题时为聚合计数，非空时取前 3 预览 + 余数。
/// 唯一实现收归刷新 Module 的 `notify_body` Interface；此处为薄委托。
fn notify_body(locale: &str, new_count: usize, titles: &[String]) -> String {
    crate::services::source::notify_body(locale, new_count, titles)
}
/// 将单个订阅源的刷新结果累加到刷新周期总计中。
/// 唯一实现收归刷新 Module 的 `merge_outcome` Interface；此处为薄委托。
fn merge_outcome(
    total_new: &mut usize,
    failures: &mut usize,
    notified: &mut Vec<String>,
    fail_kinds: &mut std::collections::HashMap<String, usize>,
    out: SourceRefreshOutcome,
) {
    crate::services::source::merge_outcome(total_new, failures, notified, fail_kinds, out)
}

/// 抓取所有（或选定的）订阅源，将新条目送入规则引擎处理、
/// 存储条目、刷新缺失的站点图标、应用保留策略并同步托盘红点徽标。向前端发送 fetch-progress / fetch-done 事件。
///
/// `background` = 定时器触发：不发送单源进度事件，并在有新文章（或规则命中“通知”）到达时发出聚合的桌面通知。
/// 单入口分流（ADR-0002）：本地 SQLite 为 UI 唯一数据源；周期决策收归刷新 Module 的单一 Interface，
/// 本函数仅保留 cycle 生成、分流、fan-out 事件 Adapter、托盘/通知 Adapter 与单行周期日志。
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
            let all = db::get_sources(&conn)?;
            build_targets(&all, ids.as_deref())
        };
        let dir = refresh_favicon_dir(&app)?;
        let feed_count = targets.len();
        log::info!("refresh start cycle={cycle} mode={mode} sync=false feeds={feed_count}");

        // 有界并发刷新：bound=6 semaphore + JoinSet fan-out/fan-in。
        let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(REFRESH_CONCURRENCY));
        // Fan-out Adapter over refresh Module per-source Interface; bound canonical in Module.
        let mut set = tokio::task::JoinSet::new();
        for (id, group_id, url, favicon) in targets {
            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| e.to_string())?;
            let task = RefreshTask {
                id,
                group_id,
                url,
                favicon,
                cycle: cycle.clone(),
                client: client.clone(),
                engine: engine.clone(),
                favicon_dir: dir.clone(),
                allow_third_party: settings.favicon_third_party,
            };
            let app_clone = app.clone();
            set.spawn(async move {
                let _permit = permit;
                refresh_one_source(app_clone, task, background).await
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
        // Retention via refresh Module seam (threshold 200 baked in); storage is Adapter.
        crate::services::source::apply_refresh_retention(
            &conn,
            settings.retention_days,
            settings.max_items_per_source,
        );
    }

    tray::update_tray(&app).await;

    let is_sync = settings.sync_account.as_ref().is_some_and(|a| a.provider == "greader");
    if background {
        let _ = app.emit(
            "fetch-done",
            serde_json::json!({ "newItems": total_new, "failures": failures, "background": true, "sync": is_sync }),
        );
        if should_notify(background, settings.notify_on_new, total_new, &notified) {
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
    let body = notify_body(&settings.locale, new_count, rule_titles);
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

    #[test]
    fn merge_outcome_mixes_fail_kinds() {
        // 给定：多类失败混合（含缺省 other）
        let outcomes = vec![
            SourceRefreshOutcome { inserted: 0, notified: vec![], failed: true, error_kind: Some("timeout".into()) },
            SourceRefreshOutcome { inserted: 0, notified: vec![], failed: true, error_kind: Some("auth".into()) },
            SourceRefreshOutcome { inserted: 0, notified: vec![], failed: true, error_kind: None },
            SourceRefreshOutcome { inserted: 1, notified: vec!["x".into()], failed: false, error_kind: None },
            SourceRefreshOutcome { inserted: 0, notified: vec![], failed: true, error_kind: Some("timeout".into()) },
        ];
        // 当：折叠时
        let (mut total, mut failures, mut notified) = (0usize, 0usize, Vec::new());
        let mut fail_kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for out in outcomes {
            merge_outcome(&mut total, &mut failures, &mut notified, &mut fail_kinds, out);
        }
        // 那么：计数与分类明细正确
        assert_eq!(total, 1);
        assert_eq!(failures, 4);
        assert_eq!(notified, vec!["x".to_string()]);
        assert_eq!(fail_kinds.get("timeout"), Some(&2));
        assert_eq!(fail_kinds.get("auth"), Some(&1));
        assert_eq!(fail_kinds.get("other"), Some(&1));
    }

    fn mock_source(id: i64, group_id: Option<i64>, url: &str, favicon: Option<&str>) -> crate::models::Source {
        crate::models::Source {
            id,
            url: url.into(),
            title: format!("S{id}"),
            description: None,
            favicon: favicon.map(String::from),
            group_id,
            last_fetched: None,
            error_count: 0,
            unread: 0,
            remote_id: None,
            last_error: None,
        }
    }

    #[test]
    fn build_targets_maps_all_and_subset() {
        // 给定：三源
        let all = vec![
            mock_source(1, None, "https://a.example/feed", None),
            mock_source(2, Some(7), "https://b.example/feed", Some("/f.png")),
            mock_source(3, None, "https://c.example/feed", None),
        ];
        // 当：全量时
        let full = build_targets(&all, None);
        // 那么：保持输入顺序全映射
        assert_eq!(full, vec![
            (1, None, "https://a.example/feed".to_string(), None),
            (2, Some(7), "https://b.example/feed".to_string(), Some("/f.png".to_string())),
            (3, None, "https://c.example/feed".to_string(), None),
        ]);
        // 当：ids 子集时
        let sub = build_targets(&all, Some(&[3, 1]));
        // 那么：仅命中子集并保持源顺序
        assert_eq!(sub, vec![
            (1, None, "https://a.example/feed".to_string(), None),
            (3, None, "https://c.example/feed".to_string(), None),
        ]);
        // 当：空子集时
        assert!(build_targets(&all, Some(&[])).is_empty());
    }

    #[test]
    fn should_notify_matrix() {
        let empty: Vec<String> = vec![];
        let some = vec!["a".to_string()];
        // 手动周期一律不弹
        assert!(!should_notify(false, true, 5, &empty));
        assert!(!should_notify(false, true, 5, &some));
        // 后台 + 开关开 + 有新文章 → 弹
        assert!(should_notify(true, true, 1, &empty));
        // 后台 + 开关关 + 有新文章但无规则通知 → 不弹
        assert!(!should_notify(true, false, 3, &empty));
        // 后台 + 无新文章但规则通知非空 → 弹（开关无关）
        assert!(should_notify(true, false, 0, &some));
        assert!(should_notify(true, true, 0, &some));
        // 后台 + 无新无通知 → 不弹
        assert!(!should_notify(true, true, 0, &empty));
        assert!(!should_notify(true, false, 0, &empty));
    }

    #[test]
    fn notify_body_matrix() {
        // 空标题：聚合计数（zh/en）
        assert_eq!(notify_body("zh-CN", 5, &[]), "获取到 5 篇新文章");
        assert_eq!(notify_body("en-US", 5, &[]), "Fetched 5 new articles");
        // 1 标题：无余数后缀
        assert_eq!(notify_body("zh-CN", 2, &["a".to_string()]), "命中通知规则：a");
        assert_eq!(notify_body("en-US", 2, &["a".to_string()]), "Notify rules matched: a");
        // 3 标题：恰好取满，无余数
        let three = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(notify_body("zh-CN", 3, &three), "命中通知规则：a、b、c");
        assert_eq!(notify_body("en-US", 3, &three), "Notify rules matched: a, b, c");
        // 5 标题：前3预览 + 余数 2
        let five = vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string(), "e".to_string()];
        assert_eq!(notify_body("zh-CN", 5, &five), "命中通知规则：a、b、c 等 2 篇");
        assert_eq!(notify_body("en-US", 5, &five), "Notify rules matched: a, b, c and 2 more");
        // 非空标题忽略 new_count（沿用既有语义）
        assert_eq!(notify_body("zh-CN", 0, &["x".to_string()]), "命中通知规则：x");
    }
}
