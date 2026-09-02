use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const ID_UNREAD: &str = "zreader-unread";
const ID_REFRESH: &str = "zreader-refresh";
const ID_MARK_ALL: &str = "zreader-mark-all";
const ID_SHOW: &str = "zreader-show";
const ID_QUIT: &str = "zreader-quit";

/// Tray handles kept alive for the whole app lifetime.
pub struct TrayHandles {
    pub icon: TrayIcon,
    pub unread_item: MenuItem<Wry>,
    pub base_icon: Option<tauri::image::Image<'static>>,
}

pub struct TrayState(pub Mutex<Option<TrayHandles>>);

fn label(zh: bool, zh_text: &str, en_text: &str) -> String {
    if zh { zh_text.to_string() } else { en_text.to_string() }
}

fn locale_is_zh(app: &AppHandle) -> bool {
    crate::settings::load(&crate::settings::settings_path(app).unwrap_or_default())
        .locale
        .starts_with("zh")
}

pub fn create_tray(app: &AppHandle) -> Result<(), tauri::Error> {
    let zh = locale_is_zh(app);

    let unread_item = MenuItem::with_id(app, ID_UNREAD, label(zh, "未读 0 篇", "0 unread"), false, None::<&str>)?;
    let refresh = MenuItem::with_id(app, ID_REFRESH, label(zh, "立即刷新所有订阅", "Refresh All"), true, None::<&str>)?;
    let mark_all = MenuItem::with_id(app, ID_MARK_ALL, label(zh, "全部标记已读", "Mark All as Read"), true, None::<&str>)?;
    let show = MenuItem::with_id(app, ID_SHOW, label(zh, "显示主窗口", "Show ZReader"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, label(zh, "退出", "Quit"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&unread_item, &refresh, &mark_all, &show, &quit])?;

    let base_icon = app.default_window_icon().map(|img| {
        tauri::image::Image::new_owned(img.rgba().to_vec(), img.width(), img.height())
    });

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("ZReader")
        .on_menu_event(|app, event| handle_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(img) = &base_icon {
        builder = builder.icon(img.clone());
    }
    let icon = builder.build(app)?;

    app.manage(TrayState(Mutex::new(Some(TrayHandles {
        icon,
        unread_item,
        base_icon,
    }))));
    Ok(())
}

fn handle_menu(app: &AppHandle, id: &str) {
    match id {
        ID_REFRESH => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::refresh_all_sources(handle, None, false).await;
            });
        }
        ID_MARK_ALL => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<crate::AppState>();
                {
                    let conn = state.db.lock().await;
                    let _ = crate::db::mark_all_read(&conn, None, None);
                }
                drop(state);
                use tauri::Emitter;
                let _ = handle.emit("unread-changed", ());
                update_tray(&handle).await;
            });
        }
        ID_SHOW => show_main_window(app),
        ID_QUIT => app.exit(0),
        _ => {}
    }
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Recompute the unread count and refresh the tray label + badge dot.
pub async fn update_tray(app: &AppHandle) {
    let Some(guard) = app.try_state::<TrayState>() else {
        return;
    };
    let state = app.state::<crate::AppState>();
    let unread = {
        let conn = state.db.lock().await;
        crate::db::total_unread(&conn).unwrap_or(0)
    };
    let mut handles = guard.0.lock().expect("tray state lock");
    let Some(handles) = handles.as_mut() else { return };
    let zh = locale_is_zh(app);
    let _ = handles.unread_item.set_text(&label(
        zh,
        &format!("未读 {unread} 篇"),
        &format!("{unread} unread"),
    ));
    if let Some(base) = &handles.base_icon {
        let _ = handles.icon.set_icon(Some(with_badge(base, unread > 0)));
    }
}

/// Draw a red dot with a white ring in the top-right corner of the icon.
fn with_badge(base: &tauri::image::Image, show: bool) -> tauri::image::Image<'static> {
    let w = base.width() as i32;
    let h = base.height() as i32;
    let mut rgba = base.rgba().to_vec();
    if show && w > 8 && h > 8 {
        let r = (w.min(h) / 5).clamp(3, 12);
        let cx = w - r - 2;
        let cy = r + 2;
        for y in (cy - r).max(0)..=(cy + r).min(h - 1) {
            for x in (cx - r).max(0)..=(cx + r).min(w - 1) {
                let dx = x - cx;
                let dy = y - cy;
                let d2 = dx * dx + dy * dy;
                if d2 > r * r {
                    continue;
                }
                let idx = ((y * w + x) as usize) * 4;
                if idx + 3 >= rgba.len() {
                    continue;
                }
                if d2 >= (r - 2) * (r - 2) {
                    rgba[idx] = 255;
                    rgba[idx + 1] = 255;
                    rgba[idx + 2] = 255;
                    rgba[idx + 3] = 255;
                } else {
                    rgba[idx] = 235;
                    rgba[idx + 1] = 59;
                    rgba[idx + 2] = 90;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }
    tauri::image::Image::new_owned(rgba, base.width(), base.height())
}
