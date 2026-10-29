//! 系统通知出口。Windows 安装版直用 WinRT toast 注入点击激活回调；
//! 其余平台（及开发目录运行）走 tauri-plugin-notification——该插件桌面端
//! 不暴露任何点击回调 API，"点击通知唤起" 仅 Windows 可达。
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

/// 发送系统通知。Windows（安装版）上点击通知会唤起主窗口并向前端
/// 发送 `notify-click` 事件（前端据此切到未读视图）。
pub fn show_toast(app: &AppHandle, title: &str, body: &str) {
    #[cfg(windows)]
    if let Some(toast) = clickable_toast(app, title, body) {
        let _ = toast.show();
        return;
    }
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(windows)]
fn clickable_toast(
    app: &AppHandle,
    title: &str,
    body: &str,
) -> Option<tauri_winrt_notification::Toast> {
    use std::path::MAIN_SEPARATOR as SEP;
    use tauri::Emitter;
    // 仅安装版设置 AUMID（与 tauri-plugin-notification 同判定）：开发目录
    // 没有对应 AUMID 的开始菜单快捷方式，toast 会直接失败，回落插件路径。
    let exe = tauri::utils::platform::current_exe().ok()?;
    let exe_dir = exe.parent()?.display().to_string();
    if exe_dir.ends_with(format!("{SEP}target{SEP}debug").as_str())
        || exe_dir.ends_with(format!("{SEP}target{SEP}release").as_str())
    {
        return None;
    }
    let handle = app.clone();
    Some(
        tauri_winrt_notification::Toast::new(app.config().identifier.as_str())
            .title(title)
            .text1(body)
            .on_activated(move |_| {
                crate::tray::show_main_window(&handle);
                let _ = handle.emit("notify-click", ());
                Ok(())
            }),
    )
}
