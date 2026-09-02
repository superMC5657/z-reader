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
}

export interface Group {
  id: number
  name: string
  expanded: boolean
  sort: number
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
}

export interface Settings {
  version: string
  theme: 'system' | 'light' | 'dark'
  view: 'cards' | 'magazine' | 'list'
  locale: string
  fontSize: number
  fetchInterval: number
  /** 0 = all, 1 = unread, 2 = starred */
  filterType: number
  /** bit0 = showCover, bit1 = showSnippet, bit2 = fadeRead */
  viewConfigs: number
  menuOn: boolean
  readerMode: 'split' | 'focus'
  shortcuts: Record<string, string>
  /** "system" (env vars + OS proxy) | "none" (direct) | "manual" */
  proxyMode: 'system' | 'none' | 'manual'
  proxyUrl: string
  proxyUsername: string
  proxyPassword: string
  notifyOnNew: boolean
  closeToTray: boolean
  /** auto-delete unstarred read articles older than N days; 0 = never */
  retentionDays: number
  /** cap unstarred articles kept per source; 0 = unlimited */
  maxItemsPerSource: number
}

export interface GetItemsParams {
  scope?: 'all' | 'source' | 'group'
  scopeId?: number | null
  filter?: number
  search?: string
  limit?: number
  offset?: number
}

export interface MenuItem {
  label: string
  danger?: boolean
  action: () => void
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
  /** "all" | "source:{id}" | "group:{id}" */
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
