//! Unified logging backend for z-reader (tauri-plugin-log v2).
//!
//! - Backend `log::*!` records land in `rust.log`; frontend records sent via
//!   `@tauri-apps/plugin-log` arrive with target `webview:*` and land in
//!   `webview.log`.
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
const KEEP_ROTATED: usize = 6;

/// Build the log plugin.
///
/// Feed / sync / net targets stay at `Info` in release so refresh failures
/// remain diagnosable; noisy transport crates are capped at `Warn`.
pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{Target, TargetKind, WEBVIEW_TARGET};

    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let builder = tauri_plugin_log::Builder::new()
        .level(level)
        // First-party modules: keep Info in release (feed refresh failures
        // must stay visible).
        .level_for("zreader_lib", log::LevelFilter::Info)
        .level_for("zreader_lib::feed", log::LevelFilter::Info)
        .level_for("zreader_lib::sync", log::LevelFilter::Info)
        .level_for("zreader_lib::net", log::LevelFilter::Info)
        // Noisy third-party crates.
        .level_for("hyper", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("tungstenite", log::LevelFilter::Warn)
        .level_for("rustls", log::LevelFilter::Warn)
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(KEEP_ROTATED))
        .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
        .targets([
            Target::new(TargetKind::LogDir {
                file_name: Some("rust".into()),
            })
            .filter(move |md| !md.target().starts_with(WEBVIEW_TARGET)),
            Target::new(TargetKind::LogDir {
                file_name: Some("webview".into()),
            })
            .filter(move |md| md.target().starts_with(WEBVIEW_TARGET)),
        ]);

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
        } else if msg[i..].len() >= 7
            && (msg[i..].starts_with("Bearer ") || msg[i..].starts_with("bearer "))
        {
            out.push_str(&msg[i..i + 7]);
            i += 7;
            while i < bytes.len() && !is_value_delim(bytes[i]) {
                i += 1;
            }
            out.push_str("***");
        } else {
            // Advance by one char (keep UTF-8 boundaries).
            let ch = msg[i..].chars().next().unwrap_or('\0');
            out.push(ch);
            i += ch.len_utf8().max(1);
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
    let rest = &msg[at..];
    // Require a word boundary before the key.
    if at > 0 {
        let prev = msg[..at].chars().next_back().unwrap_or(' ');
        if prev.is_alphanumeric() || prev == '_' || prev == '-' {
            return None;
        }
    }
    for key in keys {
        if rest.len() >= key.len() && rest[..key.len()].eq_ignore_ascii_case(key) {
            let mut j = at + key.len();
            let bytes = msg.as_bytes();
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
            let msg = redact(&info.to_string());
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
}
