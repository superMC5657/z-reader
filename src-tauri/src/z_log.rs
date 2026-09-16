//! Unified logging backend for z-reader (tauri-plugin-log v2).
//!
//! - Backend `log::*!` and frontend records sent via `@tauri-apps/plugin-log`
//!   land in a single unified log file named after the application (`z-reader.log`).
//! - Log files live in the OS log dir ([`log_dir`]); business data stays in
//!   sqlite (`zreader.db`). Logs never enter the database.
//! - Retention is best-effort file hygiene: files older than 14 days are
//!   removed, then oldest-first until the directory fits into 25 MiB.
//! - No automatic upload anywhere. Remote reporting stays an explicit,
//!   opt-in feature (TODO: add a user-gated "export & send" flow).

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Drop log files older than this (seconds).
const MAX_AGE_SECS: u64 = 14 * 24 * 60 * 60;
/// Best-effort cap for the whole log directory (25 MiB).
const MAX_DIR_BYTES: u64 = 25 * 1024 * 1024;
/// Per-file rotation threshold (5 MiB).
const MAX_FILE_BYTES: u128 = 5 * 1024 * 1024;
/// Rotated siblings kept per log file.
const KEEP_ROTATED: usize = 5;

/// Env knob override with a compiled default fallback; unknown values fall
/// back silently (a typo must never break startup logging).
fn parse_level(s: &str) -> Option<log::LevelFilter> {
    // Accept `target=level` pairs (e.g. `RUST_LOG="zreader_lib=debug"`);
    // the last segment wins.
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

/// Build the log plugin.
///
/// Feed / sync / net targets stay at `Info` in release so refresh failures
/// remain diagnosable; noisy transport crates are capped at `Warn`.
///
/// Two env knobs override the compiled defaults without touching business
/// logic: `RUST_LOG` drives the root level, `ZREADER_LOG` drives the
/// first-party (`zreader_lib*`) level and falls back to `RUST_LOG`.
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{Target, TargetKind};

    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    // First-party modules: Debug in dev (second-layer points visible),
    // Info in release (first layer only).
    let inner = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let level = level_from_env("RUST_LOG", level);
    let inner = level_from_env("ZREADER_LOG", level_from_env("RUST_LOG", inner));

    let builder = tauri_plugin_log::Builder::new()
        .level(level)
        // First-party modules: keep Info in release (feed refresh failures
        // must stay visible).
        .level_for("zreader_lib", inner)
        .level_for("zreader_lib::feed", inner)
        .level_for("zreader_lib::sync", inner)
        .level_for("zreader_lib::net", inner)
        // Noisy third-party crates.
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
            // Final-value scrub: secrets (password/token/Bearer) never reach
            // the log file even when a call site forgets to redact.
            let line = format!("[{time}] [{:5}] [{tag}] {msg}", record.level());
            out.finish(format_args!("{}", redact(&line)));
        })
        .targets([Target::new(TargetKind::LogDir { file_name: None })]);

    #[cfg(debug_assertions)]
    let builder = builder.target(Target::new(TargetKind::Stdout));

    builder.build()
}

/// Resolve the OS log directory for this app.
pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_log_dir().map_err(|e| e.to_string())
}

/// Prune the app log dir; failures are swallowed (logging must never break
/// the app). Call once during startup.
pub fn prune_app_dir(app: &AppHandle) {
    match log_dir(app) {
        Ok(dir) => prune_with(&dir, MAX_AGE_SECS, MAX_DIR_BYTES),
        Err(e) => log::warn!("zlog: cannot resolve log dir: {e}"),
    }
}

/// Retention worker: drop files older than `max_age_secs`, then delete
/// oldest-first until the directory fits into `max_bytes`.
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
        // Only manage our own log files; never touch anything else.
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
    // Oldest first; file name breaks mtime ties deterministically.
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

/// Mask secrets (passwords, tokens, bearer credentials, api keys) before a
/// message is logged. Value runs until a delimiter (`whitespace`, `"`,
/// `'`, `,`, `;`, `&`) or end of string.
pub fn redact(msg: &str) -> String {
    const KEYS: &[&str] = &[
        "password", "passwd", "pwd", "token", "secret", "api_key", "apikey", "api-key",
    ];
    let mut out = String::with_capacity(msg.len());
    let bytes = msg.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(val_start) = match_pair_at(msg, i, KEYS) {
            // Re-emit the raw `key = ` prefix untouched, mask only the value.
            out.push_str(&msg[i..val_start]);
            let mut j = val_start;
            // Skip optional quote, remember it to require the same closer.
            let quote = if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                out.push(bytes[j] as char);
                j += 1;
                Some(bytes[j - 1])
            } else {
                None
            };
            out.push_str("***");
            // Skip the secret value.
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
            // Advance by one char (keep UTF-8 boundaries).
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

/// Case-insensitive `key = value` / `key: value` match at byte offset `at`;
/// returns the value start. A bare word without `=`/`:` is NOT a pair, so
/// plain prose like "nothing secret here" passes through untouched.
fn match_pair_at(msg: &str, at: usize, keys: &[&str]) -> Option<usize> {
    if !msg.is_char_boundary(at) {
        return None;
    }
    // Require a word boundary before the key.
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

/// Capture panics into the log file. Call as the first line of `run()` so
/// even early-startup panics are recorded. Best-effort under
/// `panic = "abort"`: the hook still runs, plus a stderr fallback.
pub fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Panic payloads can echo request URLs / credentials; scrub first.
            let raw = info.to_string();
            let msg = std::panic::catch_unwind(|| redact(&raw)).unwrap_or(raw);
            log::error!("[BE] panic: {msg}");
            eprintln!("[BE] panic: {msg}");
            prev(info);
        }));
    });
}

/// Frontend command: absolute path of the log directory.
#[tauri::command]
pub fn zlog_get_dir(app: AppHandle) -> Result<String, String> {
    Ok(log_dir(&app)?.to_string_lossy().into_owned())
}

/// Frontend command: concatenate recent `*.log` files into one bundle file
/// and return its path. Opt-in only — the app never uploads logs by itself.
///
/// TODO(opt-in): add a user-gated "export & send" flow on top of this.
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
            // Skip previous bundles so repeated exports don't nest
            // ever-growing copies inside each new bundle.
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
        // Retention waterline: 5 rotated siblings, 5 MiB per file,
        // 25 MiB dir cap, 14-day max age. Any change re-tunes log volume.
        assert_eq!(KEEP_ROTATED, 5);
        assert_eq!(MAX_FILE_BYTES, 5 * 1024 * 1024);
        assert_eq!(MAX_DIR_BYTES, 25 * 1024 * 1024);
        assert_eq!(MAX_AGE_SECS, 14 * 24 * 60 * 60);
    }

    #[test]
    fn redact_combined_bearer_and_password() {
        // Given: one line carrying both a key=value secret and a Bearer token
        let msg = r#"sync failed password="hunter2" with Bearer s3cr3t-value retry"#;
        // When: scrubbed (as the format closure does for every final line)
        let got = redact(msg);
        // Then: both secrets masked, structure preserved
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
        // Key glued to other word chars must not match.
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
        // Non-log files are never touched.
        assert!(dir.join("notes.txt").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn prune_missing_dir_is_noop() {
        prune_with(Path::new("/definitely/not/here/zreader-logs"), 0, 0);
    }

    #[test]
    fn redact_multibyte_utf8_does_not_panic() {
        // Exact user interaction log with Chinese characters and quotes
        let msg1 = r#"Select article "少数派年度征文：从效率工具到生活方式" [Feed: "少数派", id=24]"#;
        let got1 = redact(msg1);
        assert_eq!(got1, msg1);

        // Chinese text mixed with password/token pairs
        let msg2 = r#"用户数据同步失败 password="我的密码123" token: abc789 状态正常"#;
        let got2 = redact(msg2);
        assert!(got2.contains(r#"password="***""#), "{got2}");
        assert!(got2.contains("token: ***"), "{got2}");
        assert!(!got2.contains("我的密码123"), "{got2}");
        assert!(!got2.contains("abc789"), "{got2}");
        assert!(got2.contains("用户数据同步失败"), "{got2}");
        assert!(got2.contains("状态正常"), "{got2}");

        // Emojis and CJK texts
        let msg3 = "🎉 欢迎阅读 【极客公园】 科技早报 🚀 token=sec-ret-999 end";
        let got3 = redact(msg3);
        assert_eq!(got3, "🎉 欢迎阅读 【极客公园】 科技早报 🚀 token=*** end");
    }
}
