use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub favicon: Option<String>,
    pub group_id: Option<i64>,
    pub last_fetched: Option<i64>,
    pub error_count: i64,
    pub unread: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub expanded: bool,
    pub sort: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub source_id: i64,
    pub guid: String,
    pub title: String,
    pub url: Option<String>,
    pub author: Option<String>,
    pub published_at: i64,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub snippet: Option<String>,
    pub image: Option<String>,
    pub has_been_read: bool,
    pub starred: bool,
}

/// Item query scope: everything, one source, or one group.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetItemsParams {
    pub scope: Option<String>, // "all" | "source" | "group"
    pub scope_id: Option<i64>,
    /// 0 = all, 1 = unread, 2 = starred
    pub filter: Option<u8>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: String,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// "cards" | "list" | "magazine" | "compact"
    pub view: String,
    pub locale: String,
    pub font_size: f64,
    /// background refresh interval in minutes
    pub fetch_interval: u64,
    /// 0 = all, 1 = unread, 2 = starred
    pub filter_type: u8,
    /// bit0 = showCover, bit1 = showSnippet, bit2 = fadeRead
    pub view_configs: u32,
    pub menu_on: bool,
    pub shortcuts: std::collections::HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut shortcuts = std::collections::HashMap::new();
        shortcuts.insert("nextArticle".into(), "j".into());
        shortcuts.insert("prevArticle".into(), "k".into());
        shortcuts.insert("toggleRead".into(), "m".into());
        shortcuts.insert("toggleStar".into(), "s".into());
        shortcuts.insert("fetchFull".into(), "f".into());
        shortcuts.insert("openInBrowser".into(), "o".into());
        shortcuts.insert("refresh".into(), "r".into());
        shortcuts.insert("closeArticle".into(), "Escape".into());
        shortcuts.insert("addSource".into(), "a".into());
        shortcuts.insert("toggleSidebar".into(), "b".into());

        Settings {
            version: env!("CARGO_PKG_VERSION").to_string(),
            theme: "system".into(),
            view: "cards".into(),
            // Empty means "not chosen yet"; the frontend fills it from the system locale.
            locale: String::new(),
            font_size: 16.0,
            fetch_interval: 30,
            filter_type: 0,
            view_configs: 0b111,
            menu_on: true,
            shortcuts,
        }
    }
}

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Strip all HTML tags, keeping only text.
pub fn html_to_text(html: &str) -> String {
    ammonia::Builder::new()
        .tags(std::collections::HashSet::new())
        .tag_attributes(std::collections::HashMap::new())
        .clean(html)
        .to_string()
}
