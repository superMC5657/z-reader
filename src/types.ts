export interface Source {
  id: number
  url: string
  title: string
  description: string | null
  favicon: string | null
  groupId: number | null
  lastFetched: number | null
  errorCount: number
  unread: number
  /** 从云端服务同步时的远程流 ID */
  remoteId: string | null
  /** 最近一次抓取/存储失败的错误信息；正常时为 null */
  lastError: string | null
}

export interface Group {
  id: number
  name: string
  expanded: boolean
  sort: number
  remoteId?: string | null
}

export interface Item {
  id: number
  sourceId: number
  guid: string
  title: string
  url: string | null
  author: string | null
  publishedAt: number
  content: string | null
  summary: string | null
  snippet: string | null
  image: string | null
  hasBeenRead: boolean
  starred: boolean
  hidden: boolean
  /** 文章来自同步服务器时的远程条目 ID（十六进制） */
  remoteId: string | null
}

export interface Settings {
  version: string
  theme: 'system' | 'light' | 'dark'
  view: 'cards' | 'magazine' | 'list'
  locale: string
  uiScale: number
  fontSize: number
  fetchInterval: number
  /** 0 = 全部，1 = 未读，2 = 星标 */
  filterType: number
  /** 位标记：bit0 = 显示封面图，bit1 = 显示摘要，bit2 = 已读变暗 */
  viewConfigs: number
  menuOn: boolean
  readerMode: 'split' | 'focus'
  shortcuts: Record<string, string>
  /** 代理模式："system"（环境变量 + 系统代理）| "none"（直连）| "manual"（手动配置） */
  proxyMode: 'system' | 'none' | 'manual'
  proxyUrl: string
  proxyUsername: string
  proxyPassword: string
  notifyOnNew: boolean
  closeToTray: boolean
  /** 自动清理超过 N 天且未加星标的已读文章；0 = 从不清理 */
  retentionDays: number
  /** 每个订阅源保留未星标文章的最大上限；0 = 不限制 */
  maxItemsPerSource: number
  /** 允许通过 Google/DuckDuckGo 获取网站图标（会外发域名请求）；关闭则仅从源站获取 */
  faviconThirdParty: boolean
  /** 云同步账户；null 表示纯本地模式 */
  syncAccount: SyncAccount | null
}

export interface SyncAccount {
  /** "greader"（兼容 Google Reader API） */
  provider: 'greader'
  /** API 基地址，例如 FreshRSS 的 "https://host/api/greader.php" */
  serverUrl: string
  username: string
  password: string
}

export interface SyncStatus {
  lastSync: number | null
  queueLen: number
}

export interface GetItemsParams {
  scope?: 'all' | 'source' | 'group'
  scopeId?: number | null
  filter?: number
  search?: string
  limit?: number
  offset?: number
}

export type RuleTargetField = 'title' | 'content' | 'author' | 'source_url' | 'any'
export type RuleActionType = 'mark_read' | 'star' | 'hide' | 'notify'

export interface Rule {
  id: number
  name: string
  pattern: string
  targetField: RuleTargetField
  actionType: RuleActionType
  isCaseSensitive: boolean
  isEnabled: boolean
  /** 作用范围："all" | "source:{id}" | "group:{id}" */
  sourceScope: string
  createdAt: number
}

export interface RuleInput {
  name: string
  pattern: string
  targetField: RuleTargetField
  actionType: RuleActionType
  isCaseSensitive: boolean
  isEnabled: boolean
  sourceScope: string
}

export interface RuleBackfillResult {
  markedRead: number
  starred: number
  hidden: number
  notified: number
}

export interface AppStats {
  articles: number
  unread: number
  dbSize: number
}
