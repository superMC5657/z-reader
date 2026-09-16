use crate::models::Settings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn secrets_path(settings_path: &Path) -> PathBuf {
    settings_path.with_file_name("secrets.json")
}

/// On-disk secrets stored in a dedicated file (`secrets.json`).
/// NEVER packed into backups (see `backup::write_archive`) and written
/// with owner-only permissions on Unix. Keeps credentials physically
/// isolated from public configuration.
#[derive(Serialize, Deserialize, Default)]
struct SecretStore {
    #[serde(default)]
    sync_password: String,
    #[serde(default)]
    proxy_password: String,
}

fn read_secrets(settings_path: &Path) -> SecretStore {
    let path = secrets_path(settings_path);
    match std::fs::read_to_string(&path) {
        // Missing file is the normal first-run state: stay silent.
        Err(_) => SecretStore::default(),
        Ok(t) => match serde_json::from_str(&t) {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "secrets corrupt file={} reason={}",
                    file_base(&path.to_string_lossy()),
                    short_reason(&e.to_string())
                );
                SecretStore::default()
            }
        },
    }
}

fn write_secrets(settings_path: &Path, store: &SecretStore) -> Result<(), String> {
    let path = secrets_path(settings_path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Drop the synced account secret (used on logout so a stale password does
/// not survive next to a removed account).
pub fn clear_sync_secret(settings_path: &Path) -> Result<(), String> {
    let mut store = read_secrets(settings_path);
    store.sync_password.clear();
    write_secrets(settings_path, &store)
}

pub fn load(path: &PathBuf) -> Settings {
    let mut s = match std::fs::read_to_string(path) {
        // Missing file is the normal first-run state: stay silent.
        Err(_) => Settings::default(),
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                log::warn!(
                    "settings corrupt file={} reason={}",
                    file_base(&path.to_string_lossy()),
                    short_reason(&e.to_string())
                );
                Settings::default()
            }
        },
    };
    let secrets = read_secrets(path);
    if let Some(acct) = s.sync_account.as_mut() {
        acct.password = secrets.sync_password;
    }
    s.proxy_password = secrets.proxy_password;
    s.version = env!("CARGO_PKG_VERSION").to_string();
    s
}

pub fn save(path: &PathBuf, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut s = settings.clone();
    let store = SecretStore {
        sync_password: s
            .sync_account
            .as_ref()
            .map(|a| a.password.clone())
            .unwrap_or_default(),
        proxy_password: s.proxy_password.clone(),
    };
    write_secrets(path, &store)?;
    if let Some(acct) = s.sync_account.as_mut() {
        acct.password.clear();
    }
    s.proxy_password.clear();
    s.version = env!("CARGO_PKG_VERSION").to_string();
    let text = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// First line of an error message, truncated: reasons never carry file
/// paths, tokens or article text.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zreader-settings-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    fn settings_with_secrets() -> Settings {
        Settings {
            sync_account: Some(crate::models::SyncAccount {
                provider: "greader".into(),
                server_url: "https://x.example".into(),
                username: "u".into(),
                password: "hunter2".into(),
            }),
            proxy_password: "s3cret".into(),
            ..Settings::default()
        }
    }

    #[test]
    fn save_redacts_json_and_load_hydrates() {
        // Given: settings carrying secrets
        let path = temp_settings("roundtrip");
        // When: saved
        save(&path, &settings_with_secrets()).unwrap();
        // Then: the JSON on disk carries no secret…
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("hunter2") && !raw.contains("s3cret"));
        // …while load() still returns them from the sidecar file
        let loaded = load(&path);
        assert_eq!(loaded.sync_account.as_ref().unwrap().password, "hunter2");
        assert_eq!(loaded.proxy_password, "s3cret");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn clear_sync_secret_drops_password() {
        let path = temp_settings("clear");
        save(&path, &settings_with_secrets()).unwrap();
        assert_eq!(load(&path).sync_account.as_ref().unwrap().password, "hunter2");
        clear_sync_secret(&path).unwrap();
        assert_eq!(load(&path).sync_account.as_ref().unwrap().password, "");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
