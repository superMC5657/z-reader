use crate::models::Settings;

/// Extract host and sanitized path (stripping sensitive query params like tokens/keys/passwords)
pub fn sanitize_url(raw_url: &str) -> (String, String) {
    if let Ok(mut u) = url::Url::parse(raw_url) {
        let host = u.host_str().unwrap_or("unknown").to_string();
        let _ = u.set_username("");
        let _ = u.set_password(None);

        let query_pairs: Vec<(String, String)> = u
            .query_pairs()
            .map(|(k, v)| {
                let key_lower = k.to_ascii_lowercase();
                if key_lower.contains("token")
                    || key_lower.contains("secret")
                    || key_lower.contains("key")
                    || key_lower.contains("auth")
                    || key_lower.contains("pass")
                {
                    (k.into_owned(), "***".to_string())
                } else {
                    (k.into_owned(), v.into_owned())
                }
            })
            .collect();

        let path = if !query_pairs.is_empty() {
            u.query_pairs_mut().clear().extend_pairs(
                query_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            format!(
                "{}{}",
                u.path(),
                u.query().map(|q| format!("?{q}")).unwrap_or_default()
            )
        } else {
            u.path().to_string()
        };
        (host, path)
    } else {
        ("unknown".into(), raw_url.to_string())
    }
}

pub struct RequestLog {
    pub kind: &'static str,
    pub method: &'static str,
    pub host: String,
    pub path: String,
    pub start: std::time::Instant,
}

impl RequestLog {
    pub fn start(kind: &'static str, method: &'static str, url: &str) -> Self {
        let (host, path) = sanitize_url(url);
        let url_disp = if host == "unknown" {
            path.clone()
        } else if path.is_empty() || path == "/" {
            format!("https://{host}")
        } else {
            format!("https://{host}{path}")
        };
        log::debug!("{method} {url_disp} [{kind}]");
        Self {
            kind,
            method,
            host,
            path,
            start: std::time::Instant::now(),
        }
    }

    fn url_display(&self) -> String {
        if self.host == "unknown" {
            self.path.clone()
        } else if self.path.is_empty() || self.path == "/" {
            format!("https://{}", self.host)
        } else {
            format!("https://{}{}", self.host, self.path)
        }
    }

    pub fn finish(&self, status: reqwest::StatusCode, bytes: Option<usize>) {
        let elapsed_ms = self.start.elapsed().as_millis();
        let url = self.url_display();
        let bytes_disp = match bytes {
            Some(b) if b >= 1024 * 1024 => format!(", {:.1} MB", b as f64 / (1024.0 * 1024.0)),
            Some(b) if b >= 1024 => format!(", {:.1} KB", b as f64 / 1024.0),
            Some(b) => format!(", {b} B"),
            None => String::new(),
        };

        if status.is_success() {
            log::info!(
                "{} {} -> {} ({elapsed_ms}ms{bytes_disp}) [{}]",
                self.method,
                url,
                status,
                self.kind
            );
        } else if self.kind == "favicon" {
            log::debug!(
                "{} {} -> {} ({elapsed_ms}ms{bytes_disp}) [{}]",
                self.method,
                url,
                status,
                self.kind
            );
        } else {
            log::warn!(
                "{} {} -> {} ({elapsed_ms}ms{bytes_disp}) [{}]",
                self.method,
                url,
                status,
                self.kind
            );
        }
    }

    pub fn finish_err(&self, reason: &str) {
        let elapsed_ms = self.start.elapsed().as_millis();
        let url = self.url_display();
        if self.kind == "favicon" {
            log::debug!(
                "{} {} -> ERR: {reason} ({elapsed_ms}ms) [{}]",
                self.method,
                url,
                self.kind
            );
        } else {
            log::warn!(
                "{} {} -> ERR: {reason} ({elapsed_ms}ms) [{}]",
                self.method,
                url,
                self.kind
            );
        }
    }
}

pub async fn send_logged(
    kind: &'static str,
    method: &'static str,
    url: &str,
    builder: reqwest::RequestBuilder,
) -> Result<reqwest::Response, reqwest::Error> {
    let req_log = RequestLog::start(kind, method, url);
    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let bytes = resp.content_length().map(|l| l as usize);
            req_log.finish(status, bytes);
            Ok(resp)
        }
        Err(e) => {
            let reason = http_err_reason("", &e);
            req_log.finish_err(&reason);
            Err(e)
        }
    }
}

/// One-line reason for a reqwest failure with the echoed request URL reduced
/// to its host: reqwest's `Error` Display appends `for url (<full>)`, which
/// would otherwise leak full URLs (query/userinfo) into logs. Callers still
/// wrap the result in `short_reason` at the log site.
pub fn http_err_reason(prefix: &str, e: &reqwest::Error) -> String {
    let mut msg = e.to_string();
    if let Some(url) = e.url() {
        let host = url.host_str().unwrap_or("unknown");
        msg = msg.replace(url.as_str(), host);
    }
    if prefix.is_empty() {
        msg
    } else {
        format!("{prefix}: {msg}")
    }
}

/// Validate a manual proxy URL before it is saved or tested, so a typo fails
/// loudly instead of silently falling back to a direct connection.
pub fn validate_proxy(settings: &Settings) -> Result<(), String> {
    if settings.proxy_mode == "manual" {
        let url = settings.proxy_url.trim();
        if url.is_empty() {
            return Err("manual proxy URL is empty".into());
        }
        reqwest::Proxy::all(url).map_err(|e| format!("invalid proxy URL: {e}"))?;
    }
    Ok(())
}

/// Build the shared HTTP client honoring the user's proxy configuration.
///
/// Note: an invalid manual URL falls back to direct here silently; the
/// settings and connectivity-test paths reject it upfront via
/// [`validate_proxy`], so this branch only covers hand-edited config files.
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
            } else {
                builder = builder.no_proxy();
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

    #[test]
    fn validate_proxy_rejects_bad_manual_url() {
        // Given: manual mode with a typo'd URL
        // When/Then: validation fails instead of silently going direct
        assert!(validate_proxy(&settings("manual", "not a url")).is_err());
        assert!(validate_proxy(&settings("manual", "")).is_err());
        assert!(validate_proxy(&settings("manual", "http://127.0.0.1:7890")).is_ok());
        assert!(validate_proxy(&settings("system", "")).is_ok());
        assert!(validate_proxy(&settings("none", "")).is_ok());
    }

    #[test]
    fn test_sanitize_url() {
        let (host, path) = sanitize_url("https://user:secretpass@example.com:8080/feed/rss.xml?token=my_secret_token&limit=20&auth_key=abc");
        assert_eq!(host, "example.com");
        assert!(path.starts_with("/feed/rss.xml?"));
        assert!(path.contains("token=***"));
        assert!(path.contains("auth_key=***"));
        assert!(path.contains("limit=20"));
        assert!(!path.contains("secretpass"));
        assert!(!path.contains("my_secret_token"));
        assert!(!path.contains("abc"));

        let (host2, path2) = sanitize_url("http://example.org/path/no_query");
        assert_eq!(host2, "example.org");
        assert_eq!(path2, "/path/no_query");
    }
}
