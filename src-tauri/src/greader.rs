//! 兼容 Google Reader API 的客户端（支持 FreshRSS、Bazqux、Inoreader、
//! TT-RSS 插件等）。纯解析辅助函数与网络调用分离，
//! 以便针对录制的响应夹具进行独立测试。

use serde_json::Value;

pub const STREAM_READING_LIST: &str = "user/-/state/com.google/reading-list";
pub const STATE_READ: &str = "user/-/state/com.google/read";
pub const STATE_STARRED: &str = "user/-/state/com.google/starred";
const ITEM_ID_PREFIX: &str = "tag:google.com,2005:reader/item/";

#[derive(Debug)]
pub enum GReaderError {
    /// 401/403 认证错误 —— 会话已失效，应通过重新登录刷新。
    Auth(String),
    /// 其他任何错误（网络、服务端异常、响应格式畸形）。
    Other(String),
}

impl std::fmt::Display for GReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GReaderError::Auth(m) => write!(f, "auth failed: {m}"),
            GReaderError::Other(m) => write!(f, "{m}"),
        }
    }
}

pub struct GSubscription {
    pub stream_id: String,
    pub title: String,
    /// 服务端上报的订阅源 URL。
    pub url: Option<String>,
    /// 分类流 ID（例如 "user/-/label/Tech"）。
    pub category_id: Option<String>,
    /// 分类标签名（映射到本地分组）。
    pub category: Option<String>,
}

pub struct GItem {
    pub remote_id: String,
    pub stream_id: String,
    pub title: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub published_at: i64,
    pub content: String,
    pub read: bool,
    pub starred: bool,
}

// ---------- ID 规范化 ----------

/// 规范化远程 ID：从长格式条目 ID 中提取的 16 位小写十六进制字符串。
/// `stream/items/ids` 返回十进制 int64 ID，而 `contents` 返回长格式；
/// 两者规范化后得到相同的值。
pub fn normalize_item_id(raw: &str) -> Option<String> {
    if let Some(hex) = raw.strip_prefix(ITEM_ID_PREFIX) {
        let hex = hex.trim().to_ascii_lowercase();
        if hex.len() == 16 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hex);
        }
    }
    // 同时也直接接受 16 位十六进制字符。
    let hex = raw.trim().to_ascii_lowercase();
    if hex.len() == 16 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(hex);
    }
    // 尝试十进制 int64 -> 十六进制格式转换。
    if let Ok(val) = raw.trim().parse::<u64>() {
        return Some(format!("{val:016x}"));
    }
    None
}

/// 将规范化十六进制远程 ID 转换回 GReader 长格式 ID，供 edit-tag 调用使用。
pub fn long_form_id(hex_id: &str) -> String {
    format!("{ITEM_ID_PREFIX}{hex_id}")
}

// ---------- 解析辅助函数 ----------

pub fn parse_login(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("Auth="))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn parse_subscriptions(body: &str) -> Result<Vec<GSubscription>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let subs = v
        .get("subscriptions")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(subs
        .iter()
        .filter_map(|s| {
            let stream_id = s.get("id")?.as_str()?.to_string();
            let title = s
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let url = s.get("url").and_then(|u| u.as_str()).map(String::from);
            let (category_id, category) = s
                .get("categories")
                .and_then(|c| c.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|c| {
                        let id = c.get("id").and_then(|i| i.as_str()).map(String::from);
                        let label = c.get("label").and_then(|l| l.as_str()).map(String::from);
                        if id.is_some() || label.is_some() {
                            Some((id, label))
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or((None, None));
            Some(GSubscription { stream_id, title, url, category_id, category })
        })
        .collect())
}

/// 返回（十进制条目 ID 列表，存在更多分页时的 continuation 续订令牌）。
pub fn parse_item_ids(body: &str) -> Result<(Vec<String>, Option<String>), String> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let refs = v
        .get("itemRefs")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let ids = refs
        .iter()
        .filter_map(|r| r.get("id").and_then(|i| i.as_str()).map(String::from))
        .collect();
    let continuation = v.get("continuation").and_then(|c| c.as_str()).map(String::from);
    Ok((ids, continuation))
}

pub fn parse_contents(body: &str) -> Result<Vec<GItem>, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let items = v
        .get("items")
        .and_then(|i| i.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|i| {
            let remote_id = normalize_item_id(i.get("id")?.as_str()?)?;
            let stream_id = i
                .get("origin")
                .and_then(|o| o.get("streamId"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let title = i.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
            let url = i
                .get("canonical")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|c| c.get("href"))
                .and_then(|h| h.as_str())
                .map(String::from)
                .or_else(|| {
                    i.get("alternate")
                        .and_then(|c| c.as_array())
                        .and_then(|a| a.first())
                        .and_then(|c| c.get("href"))
                        .and_then(|h| h.as_str())
                        .map(String::from)
                });
            let author = i
                .get("author")
                .and_then(|a| a.as_str())
                .filter(|a| !a.is_empty())
                .map(String::from);
            let published_at = i.get("published").and_then(|p| p.as_i64()).unwrap_or(0);
            let content = i
                .get("summary")
                .and_then(|s| s.get("content"))
                .and_then(|c| c.as_str())
                .or_else(|| {
                    i.get("content")
                        .and_then(|c| c.get("content"))
                        .and_then(|c| c.as_str())
                })
                .unwrap_or("")
                .to_string();
            let read = category_states(i).contains(&STATE_READ);
            let starred = category_states(i).contains(&STATE_STARRED);
            Some(GItem {
                remote_id,
                stream_id,
                title,
                url,
                author,
                published_at,
                content,
                read,
                starred,
            })
        })
        .collect())
}

fn category_states(item: &Value) -> Vec<&str> {
    item.get("categories")
        .and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|c| c.as_str()).collect())
        .unwrap_or_default()
}

// ---------- 网络调用 ----------

pub fn base_url(base: &str) -> String {
    base.trim().trim_end_matches('/').to_string()
}

pub fn login_body(email: &str, passwd: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("Email", email)
        .append_pair("Passwd", passwd)
        .finish()
}

async fn read_authed(
    client: &reqwest::Client,
    auth: &str,
    url: &str,
) -> Result<String, GReaderError> {
    let req = client
        .get(url)
        .header("Authorization", format!("GoogleLogin auth={auth}"));
    let resp = crate::net::send_logged("sync", "GET", url, req)
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(GReaderError::Auth(text));
    }
    if !status.is_success() {
        return Err(GReaderError::Other(format!("HTTP {status}")));
    }
    Ok(text)
}

pub async fn login(
    client: &reqwest::Client,
    base: &str,
    email: &str,
    passwd: &str,
) -> Result<String, GReaderError> {
    let url = format!("{}/accounts/ClientLogin", base_url(base));
    let req = client
        .post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(login_body(email, passwd));
    let resp = crate::net::send_logged("sync", "POST", &url, req)
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(GReaderError::Auth(text));
    }
    if !status.is_success() {
        return Err(GReaderError::Other(format!("HTTP {status}")));
    }
    parse_login(&text).ok_or_else(|| GReaderError::Other("no Auth in ClientLogin response".into()))
}

/// edit-tag 调用所需的短期 CSRF "T" 令牌。
pub async fn get_token(client: &reqwest::Client, base: &str, auth: &str) -> Result<String, GReaderError> {
    let url = format!("{}/reader/api/0/token", base_url(base));
    let text = read_authed(client, auth, &url).await?;
    let token = text.trim().to_string();
    if token.is_empty() {
        return Err(GReaderError::Other("empty edit-tag token".into()));
    }
    Ok(token)
}

pub async fn subscriptions(
    client: &reqwest::Client,
    base: &str,
    auth: &str,
) -> Result<Vec<GSubscription>, GReaderError> {
    let url = format!("{}/reader/api/0/subscription/list?output=json", base_url(base));
    let text = read_authed(client, auth, &url).await?;
    parse_subscriptions(&text).map_err(GReaderError::Other)
}

/// 获取 reading-list 流的单页数据，仅限于 `ot`（Unix 秒）之后变更的条目。
/// 持续沿用 `continuation` 直至返回 None。
pub async fn stream_ids(
    client: &reqwest::Client,
    base: &str,
    auth: &str,
    stream: &str,
    ot: i64,
    n: u32,
    continuation: Option<&str>,
) -> Result<(Vec<String>, Option<String>), GReaderError> {
    let mut url = url::Url::parse(&format!("{}/reader/api/0/stream/items/ids", base_url(base)))
        .map_err(|e| GReaderError::Other(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("output", "json")
        .append_pair("s", stream)
        .append_pair("n", &n.to_string());
    if ot > 0 {
        url.query_pairs_mut().append_pair("ot", &ot.to_string());
    }
    if let Some(c) = continuation {
        url.query_pairs_mut().append_pair("continuation", c);
    }
    let text = read_authed(client, auth, url.as_str()).await?;
    parse_item_ids(&text).map_err(GReaderError::Other)
}

pub async fn contents(
    client: &reqwest::Client,
    base: &str,
    auth: &str,
    ids: &[String],
) -> Result<Vec<GItem>, GReaderError> {
    let url = format!("{}/reader/api/0/stream/items/contents", base_url(base));
    // 序列化器持有非 Send 闭包且带有 drop 守卫；
    // 在独立作用域内将其完成，使其生命周期不跨越下方的 await。
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("output", "json");
        for id in ids {
            if let Some(hex) = normalize_item_id(id) {
                form.append_pair("i", &long_form_id(&hex));
            }
        }
        form.finish()
    };
    let req = client
        .post(&url)
        .header("Authorization", format!("GoogleLogin auth={auth}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body);
    let resp = crate::net::send_logged("sync", "POST", &url, req)
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(GReaderError::Auth(text));
    }
    if !status.is_success() {
        return Err(GReaderError::Other(format!("HTTP {status}")));
    }
    parse_contents(&text).map_err(GReaderError::Other)
}

/// 推送指定条目的状态变更。`add`/`remove` 为通过表单字段 `a`/`r` 应用的状态流 ID（例如 STATE_READ）。
pub async fn edit_tag(
    client: &reqwest::Client,
    base: &str,
    auth: &str,
    token: &str,
    ids: &[String],
    add: &[&str],
    remove: &[&str],
) -> Result<(), GReaderError> {
    let url = format!("{}/reader/api/0/edit-tag", base_url(base));
    // 序列化器为带有 drop 守卫的非 Send 类型 —— 限制在独立作用域内。
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        for id in ids {
            if let Some(hex) = normalize_item_id(id) {
                form.append_pair("i", &long_form_id(&hex));
            }
        }
        for a in add {
            form.append_pair("a", a);
        }
        for r in remove {
            form.append_pair("r", r);
        }
        form.append_pair("T", token);
        form.finish()
    };
    post_authed(client, auth, &url, &body).await
}

/// 流级别的 edit-tag，用于全部标记已读（s = feed/label/reading-list）。
pub async fn edit_tag_stream(
    client: &reqwest::Client,
    base: &str,
    auth: &str,
    token: &str,
    stream: &str,
    add: &str,
) -> Result<(), GReaderError> {
    let url = format!("{}/reader/api/0/edit-tag", base_url(base));
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("s", stream);
        form.append_pair("a", add);
        form.append_pair("T", token);
        form.finish()
    };
    post_authed(client, auth, &url, &body).await
}

async fn post_authed(
    client: &reqwest::Client,
    auth: &str,
    url: &str,
    body: &str,
) -> Result<(), GReaderError> {
    let req = client
        .post(url)
        .header("Authorization", format!("GoogleLogin auth={auth}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body.to_string());
    let resp = crate::net::send_logged("sync", "POST", url, req)
        .await
        .map_err(|e| GReaderError::Other(crate::net::http_err_reason("", &e)))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        let text = resp.text().await.unwrap_or_default();
        return Err(GReaderError::Auth(text));
    }
    if !status.is_success() {
        return Err(GReaderError::Other(format!("HTTP {status}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_item_id() {
        // 十进制（来自 stream/items/ids）与长格式（来自 contents）解析结果一致
        let decimal = 0x00000000175c6d3b_i64;
        assert_eq!(
            normalize_item_id(&decimal.to_string()).unwrap(),
            "00000000175c6d3b"
        );
        assert_eq!(
            normalize_item_id("tag:google.com,2005:reader/item/00000000175C6D3B").unwrap(),
            "00000000175c6d3b"
        );
        assert_eq!(normalize_item_id("garbage"), None);
        assert_eq!(normalize_item_id(""), None);
        assert_eq!(
            long_form_id("00000000175c6d3b"),
            "tag:google.com,2005:reader/item/00000000175c6d3b"
        );
    }

    #[test]
    fn test_parse_login() {
        let body = "SID=abc\nLSID=def\nAuth=afcf2b91c0d4\n";
        assert_eq!(parse_login(body).unwrap(), "afcf2b91c0d4");
        assert_eq!(parse_login("Error=BadAuthentication\n"), None);
    }

    #[test]
    fn test_parse_subscriptions() {
        let body = r#"{"subscriptions":[
            {"id":"feed/2","title":"Example","categories":[{"id":"user/-/label/Tech","label":"Tech"}],"url":"https://example.com/feed.xml","htmlUrl":"https://example.com/","iconUrl":""},
            {"id":"feed/7","title":"No Group","url":"https://other.example/rss.xml"}
        ]}"#;
        let subs = parse_subscriptions(body).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].stream_id, "feed/2");
        assert_eq!(subs[0].category_id.as_deref(), Some("user/-/label/Tech"));
        assert_eq!(subs[0].category.as_deref(), Some("Tech"));
        assert_eq!(subs[0].url.as_deref(), Some("https://example.com/feed.xml"));
        assert_eq!(subs[1].category, None);
        assert_eq!(subs[1].title, "No Group");
    }

    #[test]
    fn test_parse_item_ids() {
        let body = r#"{"itemRefs":[{"id":"1692932246948035499","timestampUsec":"1692932246000000"},{"id":"1692932246948035500"}],"continuation":"CkIIBh"}"#;
        let (ids, cont) = parse_item_ids(body).unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], "1692932246948035499");
        assert_eq!(cont.as_deref(), Some("CkIIBh"));

        let (ids2, cont2) = parse_item_ids(r#"{"itemRefs":[]}"#).unwrap();
        assert!(ids2.is_empty());
        assert!(cont2.is_none());
    }

    #[test]
    fn test_parse_contents() {
        let body = r#"{"id":"user/-/state/com.google/reading-list","updated":1700000000,"items":[
            {"id":"tag:google.com,2005:reader/item/00000000175c6d3b","crawlTimeMsec":"1700000000000","timestampUsec":"1700000000000000","published":1700000000,"title":"Hello world","canonical":[{"href":"https://example.com/a"}],"categories":["user/-/state/com.google/reading-list","user/-/state/com.google/read","user/-/label/Tech"],"origin":{"streamId":"feed/2","title":"Ex","htmlUrl":"https://example.com/"},"summary":{"direction":"ltr","content":"<p>body text</p>"},"author":"Alice"},
            {"id":"1692932246948035500","published":1700000100,"title":"Starred one","alternate":[{"href":"https://example.com/b","type":"text/html"}],"categories":["user/-/state/com.google/reading-list","user/-/state/com.google/starred"],"origin":{"streamId":"feed/7","title":"Ex2"},"content":{"content":"<p>content form</p>"}}
        ]}"#;
        let items = parse_contents(body).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].remote_id, "00000000175c6d3b");
        assert_eq!(items[0].stream_id, "feed/2");
        assert!(items[0].read);
        assert!(!items[0].starred);
        assert_eq!(items[0].url.as_deref(), Some("https://example.com/a"));
        assert_eq!(items[0].content, "<p>body text</p>");
        assert_eq!(items[0].author.as_deref(), Some("Alice"));
        // 十进制 ID 规范化为十六进制，并成功提取 content 格式的正文
        assert_eq!(items[1].remote_id, format!("{:016x}", 1692932246948035500i64));
        assert!(!items[1].read);
        assert!(items[1].starred);
        assert_eq!(items[1].content, "<p>content form</p>");
        assert_eq!(items[1].url.as_deref(), Some("https://example.com/b"));
    }

    #[test]
    fn test_login_body_encoding() {
        let body = login_body("user@example.com", "p&ss=w rd");
        assert!(body.contains("Email=user%40example.com"));
        assert!(body.contains("Passwd=p%26ss%3Dw+rd"));
    }
}
