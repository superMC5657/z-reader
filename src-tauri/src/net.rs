use crate::models::Settings;

/// Build the shared HTTP client honoring the user's proxy configuration.
pub fn build_http_client(settings: &Settings) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; ZReader/0.2)")
        .timeout(std::time::Duration::from_secs(30));

    match settings.proxy_mode.as_str() {
        // Explicitly bypass any system/env proxy.
        "none" => builder = builder.no_proxy(),
        "manual" => {
            let url = settings.proxy_url.trim();
            if url.is_empty() {
                builder = builder.no_proxy();
            } else if let Ok(mut proxy) = reqwest::Proxy::all(url) {
                if !settings.proxy_username.is_empty() {
                    proxy = proxy.basic_auth(&settings.proxy_username, &settings.proxy_password);
                }
                builder = builder.proxy(proxy);
            }
        }
        // "system": keep reqwest defaults (env vars + OS proxy integration).
        _ => {}
    }

    builder.build().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: &str, url: &str) -> Settings {
        Settings {
            proxy_mode: mode.into(),
            proxy_url: url.into(),
            ..Settings::default()
        }
    }

    #[test]
    fn builds_for_all_modes() {
        // All modes must produce a usable client (or default fallback), never panic.
        let _ = build_http_client(&settings("system", ""));
        let _ = build_http_client(&settings("none", ""));
        let _ = build_http_client(&settings("manual", ""));
    }

    #[test]
    fn proxy_urls_parse() {
        assert!(reqwest::Proxy::all("http://127.0.0.1:7890").is_ok());
        assert!(reqwest::Proxy::all("socks5://127.0.0.1:1080").is_ok());
        assert!(reqwest::Proxy::all("not a url").is_err());
    }
}
