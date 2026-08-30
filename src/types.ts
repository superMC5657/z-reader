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
}

export interface Settings {
  version: string
  theme: 'system' | 'light' | 'dark'
  view: 'cards' | 'list' | 'magazine' | 'compact'
  locale: string
  fontSize: number
  fetchInterval: number
  /** 0 = all, 1 = unread, 2 = starred */
  filterType: number
  /** bit0 = showCover, bit1 = showSnippet, bit2 = fadeRead */
  viewConfigs: number
  menuOn: boolean
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
