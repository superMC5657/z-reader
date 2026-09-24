//! z-reader 的统一日志后端（基于 tauri-plugin-log v2）。
//!
//! - 后端 `log::*!` 与通过 `@tauri-apps/plugin-log` 发送的前端日志记录均汇入以应用命名的单个统一日志文件（`z-reader.log`）。
//! - 日志文件保存在操作系统的日志目录中（[`log_dir`]）；业务数据保存在 SQLite（`zreader.db`）中。日志绝不会写入数据库。
//! - 保留策略为尽力而为的文件清理：删除超过 14 天的文件，然后按最旧优先的顺序删除，直至整个目录小于 25 MiB。
//! - 绝不自动向任何远程上传日志。远程上报保持为显式选择加入的功能（待办：添加需用户确认的“导出并发送”流程）。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// 丢弃超过此时长的日志文件（秒）。
const MAX_AGE_SECS: u64 = 14 * 24 * 60 * 60;
/// 整个日志目录的软性上限（25 MiB）。
const MAX_DIR_BYTES: u64 = 25 * 1024 * 1024;
/// 单个日志文件轮转阈值（5 MiB）。
const MAX_FILE_BYTES: u128 = 5 * 1024 * 1024;
/// 每个日志文件保留的轮转副本数量。
const KEEP_ROTATED: usize = 5;

/// 环境变量覆盖日志级别，回退到编译时默认值；未知值会静默回退（拼写错误绝不能破坏启动日志记录）。
fn parse_level(s: &str) -> Option<log::LevelFilter> {
    // 接受 `target=level` 键值对（例如 `RUST_LOG="zreader_lib=debug"`）；
    // 以最后一段为准。
    let v = s.rsplit([',', '=', ';']).next().unwrap_or(s);
    match v.trim().to_ascii_lowercase().as_str() {
        "off" => Some(log::LevelFilter::Off),
        "error" => Some(log::LevelFilter::Error),
        "warn" | "warning" => Some(log::LevelFilter::Warn),
        "info" => Some(log::LevelFilter::Info),
        "debug" => Some(log::LevelFilter::Debug),
        "trace" => Some(log::LevelFilter::Trace),
        _ => None,
    }
}

fn level_from_env(var: &str, default: log::LevelFilter) -> log::LevelFilter {
    std::env::var(var)
        .ok()
        .and_then(|v| parse_level(&v))
        .unwrap_or(default)
}

/// 构建日志插件。
///
/// 在 release 模式下，Feed / sync / net 目标保持为 `Info` 级别，以便刷新失败可供诊断；嘈杂的传输 crate 限制为 `Warn` 级别。
///
/// 两个环境变量可在不改动业务逻辑的前提下覆盖编译默认值：`RUST_LOG` 控制根级别，`ZREADER_LOG` 控制己方（`zreader_lib*`）级别，并回退到 `RUST_LOG`。
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{Target, TargetKind};

    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    // 己方模块：开发模式下为 Debug（展示第二层细节），
    // release 模式下为 Info（仅展示第一层主要信息）。
    let inner = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let level = level_from_env("RUST_LOG", level);
    let inner = level_from_env("ZREADER_LOG", level_from_env("RUST_LOG", inner));

    let builder = tauri_plugin_log::Builder::new()
        .level(level)
        // 己方模块：在 release 模式下保持 Info（订阅源刷新失败必须保持可见）。
        .level_for("zreader_lib", inner)
        .level_for("zreader_lib::feed", inner)
        .level_for("zreader_lib::sync", inner)
        .level_for("zreader_lib::net", inner)
        // 嘈杂的第三方 crate。
        .level_for("hyper", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("tungstenite", log::LevelFilter::Warn)
        .level_for("rustls", log::LevelFilter::Warn)
        .level_for("html5ever", log::LevelFilter::Error)
        .level_for("selectors", log::LevelFilter::Error)
        .level_for("markup5ever", log::LevelFilter::Error)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(KEEP_ROTATED))
        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
        .format(|out, message, record| {
            let target = record.target();
            let tag = if target.starts_with("webview") {
                "UI"
            } else if let Some(sub) = target.strip_prefix("zreader_lib::") {
                match sub {
                    "commands" => "CMD",
                    "net" => "NET",
                    "sync" => "SYNC",
                    "feed" => "FEED",
                    "db" => "DB",
                    "z_log" => "LOG",
                    "extractor" => "EXTRACT",
                    "rules" => "RULES",
                    "opml_io" => "OPML",
                    other => other,
                }
            } else if target == "zreader_lib" {
                "APP"
            } else {
                target.split("::").next().unwrap_or(target)
            };

            let msg_str = message.to_string();
            let msg = if let Some(stripped) = msg_str
                .strip_prefix("[UI] ")
                .or_else(|| msg_str.strip_prefix("[CMD] "))
                .or_else(|| msg_str.strip_prefix("[NET] "))
                .or_else(|| msg_str.strip_prefix("[BE] "))
            {
                stripped
            } else {
                &msg_str
            };

            let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            // 最终值脱敏：即使调用处忘记脱敏，机密信息（密码/令牌/Bearer）也绝不会写入日志文件。
            let line = format!("[{time}] [{:5}] [{tag}] {msg}", record.level());
            out.finish(format_args!("{}", redact(&line)));
        })
        .targets([Target::new(TargetKind::LogDir { file_name: None })]);

    #[cfg(debug_assertions)]
    let builder = builder.target(Target::new(TargetKind::Stdout));

    builder.build()
}

/// 解析本应用对应的操作系统日志目录。
pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_log_dir().map_err(|e| e.to_string())
}

/// 清理应用日志目录；失败会被静默忽略（日志逻辑绝不能导致应用崩溃）。在启动时调用一次。
pub fn prune_app_dir(app: &AppHandle) {
    match log_dir(app) {
        Ok(dir) => prune_with(&dir, MAX_AGE_SECS, MAX_DIR_BYTES),
        Err(e) => log::warn!("zlog: cannot resolve log dir: {e}"),
    }
}

/// 保留清理工作器：删除超过 `max_age_secs` 的文件，然后按最旧优先删除，直至目录总大小容纳于 `max_bytes` 内。
fn prune_with(dir: &Path, max_age_secs: u64, max_bytes: u64) {
    let entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };
    let now = std::time::SystemTime::now();
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // 仅管理应用自身的日志文件；绝不触碰其他任何文件。
        if path.extension().and_then(|e| e.to_str()) != Some("log") {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta.modified().unwrap_or(now);
        let age_ok = now
            .duration_since(mtime)
            .map(|d| d.as_secs() <= max_age_secs)
            .unwrap_or(true);
        if !age_ok {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        files.push((path, meta.len(), mtime));
    }
    // 最旧的文件优先；当修改时间相同时以文件名确定性打破平局。
    files.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    for (path, size, _) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

/// 在消息被记录之前遮盖机密信息（密码、令牌、Bearer 凭证、API 密钥）。值的范围持续到分隔符（`空白字符`、`"`、`'`、`,`、`;`、`&`）或字符串末尾。
pub fn redact(msg: &str) -> String {
    const KEYS: &[&str] = &[
        "password", "passwd", "pwd", "token", "secret", "api_key", "apikey", "api-key",
    ];
    let mut out = String::with_capacity(msg.len());
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(val_start) = match_pair_at(msg, i, KEYS) {
            // 原样重新输出未遮盖的 `key = ` 前缀，仅遮盖值部分。
            out.push_str(&msg[i..val_start]);
            let mut j = val_start;
            // 跳过可选的引号，并记录它以便匹配相同的闭合引号。
            let quote = if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                out.push(bytes[j] as char);
                j += 1;
                Some(bytes[j - 1])
            } else {
                None
            };
            out.push_str("***");
            // 跳过机密值。
            while j < bytes.len() {
                let c = bytes[j];
                if Some(c) == quote || is_value_delim(c) {
                    break;
                }
                j += 1;
            }
            if let Some(q) = quote {
                if j < bytes.len() && bytes[j] == q {
                    out.push(q as char);
                    j += 1;
                }
            }
            i = j;
        } else if bytes[i..].len() >= 7
            && (bytes[i..].starts_with(b"Bearer ") || bytes[i..].starts_with(b"bearer "))
        {
            out.push_str(&msg[i..i + 7]);
            i += 7;
            while i < bytes.len() && !is_value_delim(bytes[i]) {
                i += 1;
            }
            out.push_str("***");
        } else {
            // 推进一个字符（保持 UTF-8 字符边界）。
            if let Some(ch) = msg.get(i..).and_then(|s| s.chars().next()) {
                out.push(ch);
                i += ch.len_utf8();
            } else {
                i += 1;
                while i < bytes.len() && !msg.is_char_boundary(i) {
                    i += 1;
                }
            }
        }
    }
    out
}

fn is_value_delim(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b',' | b';' | b'&')
}

/// 在字节偏移量 `at` 处进行不区分大小写的 `key = value` / `key: value` 匹配；
/// 返回值的起始位置。没有 `=`/`:` 的裸词不会被视为键值对，因此诸如 "nothing secret here" 的普通文本会原样保留。
fn match_pair_at(msg: &str, at: usize, keys: &[&str]) -> Option<usize> {
    if !msg.is_char_boundary(at) {
        return None;
    }
    // 键之前必须存在单词边界。
    if at > 0 {
        let prev = msg[..at].chars().next_back().unwrap_or(' ');
        if prev.is_alphanumeric() || prev == '_' || prev == '-' {
            return None;
        }
    }
    let bytes = msg.as_bytes();
    let rest_bytes = &bytes[at..];
    for key in keys {
        let kbytes = key.as_bytes();
        if rest_bytes.len() >= kbytes.len()
            && rest_bytes[..kbytes.len()].eq_ignore_ascii_case(kbytes)
        {
            let mut j = at + kbytes.len();
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'=' || bytes[j] == b':') {
                j += 1;
                while j < bytes.len()
                    && (bytes[j] == b' ' || bytes[j] == b'=' || bytes[j] == b':')
                {
                    j += 1;
                }
                return Some(j);
            }
        }
    }
    None
}

/// 将恐慌（panic）捕获并写入日志文件。作为 `run()` 的第一行调用，以便甚至启动早期的 panic 也能被记录。在 `panic = "abort"` 模式下尽力而为：钩子仍会运行，并附带 stderr 回退输出。
pub fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Panic 负载可能会回显请求 URL / 凭据；先执行脱敏。
            let raw = info.to_string();
            let msg = std::panic::catch_unwind(|| redact(&raw)).unwrap_or(raw);
            log::error!("[BE] panic: {msg}");
            eprintln!("[BE] panic: {msg}");
            prev(info);
        }));
    });
}

/// 前端命令：获取日志目录的绝对路径。
#[tauri::command]
pub fn zlog_get_dir(app: AppHandle) -> Result<String, String> {
    Ok(log_dir(&app)?.to_string_lossy().into_owned())
}

/// 前端命令：将近期的 `*.log` 文件合并为一个打包日志文件并返回其路径。仅供用户主动选择——应用绝不会自行上传日志。
///
/// 待办(opt-in)：在此基础上添加需用户确认的“导出并发送”流程。
#[tauri::command]
pub async fn zlog_export_bundle(app: AppHandle) -> Result<String, String> {
    let dir = log_dir(&app)?;
    let mut logs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            if !p.is_file() || p.extension().and_then(|e| e.to_str()) != Some("log") {
                return false;
            }
            // 跳过之前生成的打包文件，避免重复导出导致新包中嵌套无限膨胀的副本。
            !p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("zreader-log-bundle-"))
        })
        .collect();
    logs.sort();
    if logs.is_empty() {
        return Err("no log files found".into());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bundle = dir.join(format!("zreader-log-bundle-{stamp}.log"));
    let mut out = String::new();
    for path in &logs {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        out.push_str(&format!("===== {name} =====\n"));
        match std::fs::read_to_string(path) {
            Ok(text) => out.push_str(&text),
            Err(e) => out.push_str(&format!("<unreadable: {e}>\n")),
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    std::fs::write(&bundle, out).map_err(|e| e.to_string())?;
    Ok(bundle.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converged_waterline_locked() {
        // 保留水位线：保留 5 个轮转副本、单文件 5 MiB、目录上限 25 MiB、最长保留 14 天。任何更改都会重新调整日志体量。
        assert_eq!(KEEP_ROTATED, 5);
        assert_eq!(MAX_FILE_BYTES, 5 * 1024 * 1024);
        assert_eq!(MAX_DIR_BYTES, 25 * 1024 * 1024);
        assert_eq!(MAX_AGE_SECS, 14 * 24 * 60 * 60);
    }

    #[test]
    fn redact_combined_bearer_and_password() {
        // 设定：单行同时包含 key=value 形式的机密信息和 Bearer 令牌
        let msg = r#"sync failed password="hunter2" with Bearer s3cr3t-value retry"#;
        // 操作：进行脱敏（与日志格式闭包对最终每行日志的处理一致）
        let got = redact(msg);
        // 验证：两处机密均被遮盖，原有结构得到保留
        assert!(got.contains(r#"password="***""#), "{got}");
        assert!(got.contains("Bearer ***"), "{got}");
        assert!(!got.contains("hunter2"), "{got}");
        assert!(!got.contains("s3cr3t-value"), "{got}");
    }

    #[test]
    fn env_level_parse_locked() {
        assert_eq!(parse_level("debug"), Some(log::LevelFilter::Debug));
        assert_eq!(parse_level("INFO"), Some(log::LevelFilter::Info));
        assert_eq!(parse_level("warn"), Some(log::LevelFilter::Warn));
        assert_eq!(parse_level("error"), Some(log::LevelFilter::Error));
        assert_eq!(parse_level("off"), Some(log::LevelFilter::Off));
        assert_eq!(parse_level("zreader_lib=debug"), Some(log::LevelFilter::Debug));
        assert_eq!(parse_level("nope"), None);
        assert_eq!(level_from_env("ZREADER_LOG_DEFINITELY_UNSET", log::LevelFilter::Info), log::LevelFilter::Info);
    }
    #[test]
    fn redact_masks_password_and_token_pairs() {
        let msg = r#"login failed password="hunter2" user=alice token: abc123 ok=1"#;
        let got = redact(msg);
        assert!(got.contains(r#"password="***""#), "{got}");
        assert!(got.contains("token: ***"), "{got}");
        assert!(got.contains("user=alice"), "{got}");
        assert!(!got.contains("hunter2"), "{got}");
        assert!(!got.contains("abc123"), "{got}");
    }

    #[test]
    fn redact_masks_bearer_and_leaves_plain_text() {
        let got = redact("fetch https://x with Bearer s3cr3t-value ok");
        assert_eq!(got, "fetch https://x with Bearer *** ok");
        assert_eq!(redact("nothing secret here"), "nothing secret here");
        // 紧贴其他单词字符的键不能匹配。
        assert_eq!(redact("mytoken=x"), "mytoken=x");
    }

    #[test]
    fn prune_keeps_total_within_cap_oldest_first() {
        let dir = std::env::temp_dir().join(format!(
            "zreader-zlog-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a.log", "b.log", "c.log"] {
            std::fs::write(dir.join(name), [b'x'; 10]).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), [b'y'; 100]).unwrap();

        prune_with(&dir, u64::MAX, 25);

        assert!(!dir.join("a.log").exists());
        assert!(dir.join("b.log").exists());
        assert!(dir.join("c.log").exists());
        // 绝不触碰非日志文件。
        assert!(dir.join("notes.txt").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prune_missing_dir_is_noop() {
        prune_with(Path::new("/definitely/not/here/zreader-logs"), 0, 0);
    }

    #[test]
    fn redact_multibyte_utf8_does_not_panic() {
        // 包含中文字符与引号的用户交互日志精确测试
        let msg1 = r#"Select article "少数派年度征文：从效率工具到生活方式" [Feed: "少数派", id=24]"#;
        let got1 = redact(msg1);
        assert_eq!(got1, msg1);

        // 中文文本混杂 password/token 键值对
        let msg2 = r#"用户数据同步失败 password="我的密码123" token: abc789 状态正常"#;
        let got2 = redact(msg2);
        assert!(got2.contains(r#"password="***""#), "{got2}");
        assert!(got2.contains("token: ***"), "{got2}");
        assert!(!got2.contains("我的密码123"), "{got2}");
        assert!(!got2.contains("abc789"), "{got2}");
        assert!(got2.contains("用户数据同步失败"), "{got2}");
        assert!(got2.contains("状态正常"), "{got2}");

        // Emoji 表情符号与中日韩（CJK）文本
        let msg3 = "🎉 欢迎阅读 【极客公园】 科技早报 🚀 token=sec-ret-999 end";
        let got3 = redact(msg3);
        assert_eq!(got3, "🎉 欢迎阅读 【极客公园】 科技早报 🚀 token=*** end");
    }
}
