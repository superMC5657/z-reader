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

/// 存储在独立文件（`secrets.json`）中的磁盘密钥。
/// 绝不打包进备份文件（参见 `backup::write_archive`），并在 Unix 系统上
/// 设置仅所有者可读写权限。使敏感凭据在物理层面与公开配置隔离。
#[derive(Serialize, Deserialize, Default)]
struct SecretStore {
    #[serde(default)]
    sync_password: String,
    #[serde(default)]
    proxy_password: String,
}

/// 读取 JSON 文件并在失败时回退为默认值：文件缺失视为首次运行保持静默，
/// 解析失败则记录脱敏警告后回退。`label` 为日志前缀（如 "secrets corrupt"）。
fn read_json_safe<T>(path: &Path, label: &str) -> T
where
    T: Default + serde::de::DeserializeOwned,
{
    match std::fs::read_to_string(path) {
        // 文件不存在是首次运行的正常状态：保持静默。
        Err(_) => T::default(),
        Ok(text) => match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                log::warn!(
                    "{label} file={} reason={}",
                    crate::utils::file_base(&path.to_string_lossy()),
                    crate::utils::short_reason(&e.to_string())
                );
                T::default()
            }
        },
    }
}

/// 将值序列化为易读 JSON 并写入文件，写入前确保父目录存在。
fn write_json_safe<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn read_secrets(settings_path: &Path) -> SecretStore {
    read_json_safe(&secrets_path(settings_path), "secrets corrupt")
}

fn write_secrets(settings_path: &Path, store: &SecretStore) -> Result<(), String> {
    let path = secrets_path(settings_path);
    write_json_safe(&path, store)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 清除同步账户的密码凭据（用于退出登录，避免已移除账户的残留密码
/// 继续驻留在磁盘中）。
pub fn clear_sync_secret(settings_path: &Path) -> Result<(), String> {
    let mut store = read_secrets(settings_path);
    store.sync_password.clear();
    write_secrets(settings_path, &store)
}

pub fn load(path: &Path) -> Settings {
    let mut s: Settings = read_json_safe(path, "settings corrupt");
    let secrets = read_secrets(path);
    if let Some(acct) = s.sync_account.as_mut() {
        acct.password = secrets.sync_password;
    }
    s.proxy_password = secrets.proxy_password;
    s.version = env!("CARGO_PKG_VERSION").to_string();
    s
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
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
    write_json_safe(path, &s)
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
        // 给定：带有凭据的设置
        let path = temp_settings("roundtrip");
        // 当：保存时
        save(&path, &settings_with_secrets()).unwrap();
        // 那么：磁盘上的 JSON 不包含任何凭据…
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("hunter2") && !raw.contains("s3cret"));
        // …同时 load() 仍能从附随的密钥文件中读取它们
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
