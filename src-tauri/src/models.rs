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
    /// Remote stream id when the source is synced (e.g. "feed/…"), None for local-only.
    pub remote_id: Option<String>,
    /// Last fetch/store failure message; cleared on the next success.
    pub last_error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub expanded: bool,
    pub sort: i64,
    pub remote_id: Option<String>,
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
    /// Remote item id (normalized hex form) when the article came from a sync server.
    pub remote_id: Option<String>,
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

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RuleTargetField {
    Title,
    Content,
    Author,
    SourceUrl,
    Any,
}

impl RuleTargetField {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleTargetField::Title => "title",
            RuleTargetField::Content => "content",
            RuleTargetField::Author => "author",
            RuleTargetField::SourceUrl => "source_url",
            RuleTargetField::Any => "any",
        }
    }
}

impl std::fmt::Display for RuleTargetField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for RuleTargetField {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "title" => Ok(RuleTargetField::Title),
            "content" => Ok(RuleTargetField::Content),
            "author" => Ok(RuleTargetField::Author),
            "source_url" => Ok(RuleTargetField::SourceUrl),
            "any" => Ok(RuleTargetField::Any),
            _ => Err(format!("unknown target_field: {s}")),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RuleActionType {
    MarkRead,
    Star,
    Hide,
    Notify,
}

impl RuleActionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleActionType::MarkRead => "mark_read",
            RuleActionType::Star => "star",
            RuleActionType::Hide => "hide",
            RuleActionType::Notify => "notify",
        }
    }
}

impl std::fmt::Display for RuleActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for RuleActionType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mark_read" => Ok(RuleActionType::MarkRead),
            "star" => Ok(RuleActionType::Star),
            "hide" => Ok(RuleActionType::Hide),
            "notify" => Ok(RuleActionType::Notify),
            _ => Err(format!("unknown action_type: {s}")),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum RuleSourceScope {
    All,
    Source(i64),
    Group(i64),
}

impl std::fmt::Display for RuleSourceScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleSourceScope::All => write!(f, "all"),
            RuleSourceScope::Source(id) => write!(f, "source:{id}"),
            RuleSourceScope::Group(id) => write!(f, "group:{id}"),
        }
    }
}

impl std::str::FromStr for RuleSourceScope {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "all" {
            Ok(RuleSourceScope::All)
        } else if let Some(id) = s.strip_prefix("source:") {
            id.parse::<i64>().map(RuleSourceScope::Source).map_err(|e| e.to_string())
        } else if let Some(id) = s.strip_prefix("group:") {
            id.parse::<i64>().map(RuleSourceScope::Group).map_err(|e| e.to_string())
        } else {
            Err(format!("invalid source_scope: {s}"))
        }
    }
}

impl Serialize for RuleSourceScope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RuleSourceScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
    MarkAllRead,
}

impl SyncAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncAction::MarkRead => "mark_read",
            SyncAction::MarkUnread => "mark_unread",
            SyncAction::Star => "star",
            SyncAction::Unstar => "unstar",
            SyncAction::MarkAllRead => "mark_all_read",
        }
    }
}

impl std::fmt::Display for SyncAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for SyncAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mark_read" => Ok(SyncAction::MarkRead),
            "mark_unread" => Ok(SyncAction::MarkUnread),
            "star" => Ok(SyncAction::Star),
            "unstar" => Ok(SyncAction::Unstar),
            "mark_all_read" => Ok(SyncAction::MarkAllRead),
            _ => Err(format!("unknown sync action: {s}")),
        }
    }
}

/// A user-defined regex automation rule.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: i64,
    pub name: String,
    pub pattern: String,
    pub target_field: RuleTargetField,
    pub action_type: RuleActionType,
    pub is_case_sensitive: bool,
    pub is_enabled: bool,
    pub source_scope: RuleSourceScope,
    pub created_at: i64,
}

/// Cloud sync account credentials. Only "greader" is supported for now.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncAccount {
    /// "greader" (Google Reader compatible API)
    pub provider: String,
    /// API base URL, e.g. "https://host/api/greader.php" for FreshRSS
    pub server_url: String,
    pub username: String,
    pub password: String,
}

/// Editable subset of a rule sent from the frontend on create/update.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    pub name: String,
    pub pattern: String,
    pub target_field: RuleTargetField,
    pub action_type: RuleActionType,
    pub is_case_sensitive: bool,
    pub is_enabled: bool,
    pub source_scope: RuleSourceScope,
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
    /// Allow favicon lookup via third-party services (Google/DUCKDUCKGO),
    /// which discloses subscribed domains to them. Off = origin servers only.
    #[serde(default = "default_favicon_third_party")]
    pub favicon_third_party: bool,
    /// Cloud sync account; None = pure local mode.
    pub sync_account: Option<SyncAccount>,
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
            favicon_third_party: true,
            sync_account: None,
        }
    }
}

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Pre-redaction default: third-party favicon fallback stays on for settings
/// files written before the toggle existed.
fn default_favicon_third_party() -> bool {
    true
}

/// Strip all HTML tags, keeping only text.
pub fn html_to_text(html: &str) -> String {
    ammonia::Builder::new()
        .tags(std::collections::HashSet::new())
        .tag_attributes(std::collections::HashMap::new())
        .clean(html)
        .to_string()
}
