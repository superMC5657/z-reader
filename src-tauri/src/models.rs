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
    /// Set by the rule engine's "hide" action; excluded from normal lists.
    pub hidden: bool,
}

/// Item query scope: everything, one source, or one group.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetItemsParams {
    pub scope: Option<String>, // "all" | "source" | "group"
    pub scope_id: Option<i64>,
    /// 0 = all, 1 = unread, 2 = starred, 3 = hidden (rules review)
    pub filter: Option<u8>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// A user-defined regex automation rule.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: i64,
    pub name: String,
    pub pattern: String,
    /// "title" | "content" | "author" | "source_url" | "any"
    pub target_field: String,
    /// "mark_read" | "star" | "hide" | "notify"
    pub action_type: String,
    pub is_case_sensitive: bool,
    pub is_enabled: bool,
    /// "all" | "source:{id}" | "group:{id}"
    pub source_scope: String,
    pub created_at: i64,
}

/// Editable subset of a rule sent from the frontend on create/update.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    pub name: String,
    pub pattern: String,
    pub target_field: String,
    pub action_type: String,
    pub is_case_sensitive: bool,
    pub is_enabled: bool,
    pub source_scope: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: String,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// "cards" | "magazine" | "list"
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
    pub reader_mode: String,
    pub shortcuts: std::collections::HashMap<String, String>,
    /// "system" (env vars + OS proxy) | "none" (direct) | "manual"
    pub proxy_mode: String,
    pub proxy_url: String,
    pub proxy_username: String,
    pub proxy_password: String,
    /// Show an aggregated desktop notification after background refresh finds new articles.
    pub notify_on_new: bool,
    /// Closing the main window hides it to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Auto-delete unstarred read articles older than N days; 0 = never.
    pub retention_days: u32,
    /// Cap unstarred articles kept per source; 0 = unlimited.
    pub max_items_per_source: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let mut shortcuts = std::collections::HashMap::new();
        shortcuts.insert("nextArticle".into(), "ArrowRight".into());
        shortcuts.insert("prevArticle".into(), "ArrowLeft".into());
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
            reader_mode: "split".into(),
            shortcuts,
            proxy_mode: "system".into(),
            proxy_url: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            notify_on_new: true,
            close_to_tray: true,
            retention_days: 0,
            max_items_per_source: 0,
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
