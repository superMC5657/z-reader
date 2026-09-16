import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import * as api from '../lib/tauri'
import type { Group, Item, Source } from '../types'
import { useAppStore } from './app'
import { zlog } from '../lib/z-log'

interface UiState {
  menuVisible: boolean
  x: number
  y: number
  items: { label: string; icon?: string; checked?: boolean; danger?: boolean; action: () => void }[]
}

export const useUiStore = defineStore('ui', {
  state: (): UiState => ({ menuVisible: false, x: 0, y: 0, items: [] }),
  actions: {
    openMenu(x: number, y: number, items: UiState['items']) {
      this.x = x
      this.y = y
      this.items = items
      this.menuVisible = true
    },
    closeMenu() {
      this.menuVisible = false
    },
  },
})

interface Scope {
  type: 'all' | 'group' | 'source'
  id: number | null
}

const PAGE_SIZE = 300

function truncate(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str
  return str.slice(0, maxLen) + '...'
}

function listParams(scope: Scope, filter: number, search: string, offset: number) {
  return {
    scope: scope.type,
    scopeId: scope.id,
    filter,
    search: search || undefined,
    limit: PAGE_SIZE,
    offset,
  }
}

export const useDataStore = defineStore('data', {
  state: () => ({
    sources: [] as Source[],
    groups: [] as Group[],
    items: [] as Item[],
    scope: { type: 'all', id: null } as Scope,
    selectedId: null as number | null,
    selectedItem: null as Item | null,
    itemLoading: false,
    loading: false,
    fetching: false,
    loadingMore: false,
    hasMore: true,
    search: '',
  }),
  getters: {
    sourceById: (state) => {
      const map = new Map<number, Source>()
      for (const s of state.sources) map.set(s.id, s)
      return (id: number) => map.get(id)
    },
    unreadOf(): (scope: Scope) => number {
      const sources = this.sources
      return (scope: Scope) => {
        const inScope = sources.filter((s) => {
          if (scope.type === 'all') return true
          if (scope.type === 'source') return s.id === scope.id
          return s.groupId === scope.id
        })
        return inScope.reduce((sum, s) => sum + s.unread, 0)
      }
    },
    totalUnread(): number {
      return this.sources.reduce((sum, s) => sum + s.unread, 0)
    },
  },
  actions: {
    async init() {
      await Promise.all([this.loadSources(), this.loadGroups()])
      await this.loadItems()
      await listen<unknown>('fetch-progress', () => {
        this.fetching = true
      })
      await listen<{ background?: boolean }>('fetch-done', () => {
        this.fetching = false
        this.loadSources()
        this.loadItems()
      })
      // Tray "mark all as read" and similar Rust-side mutations.
      await listen<unknown>('unread-changed', () => {
        this.loadSources().catch(() => {})
        this.loadItems().catch(() => {})
      })
      // A backup restore swapped the database underneath us.
      await listen<unknown>('data-restored', () => {
        this.loadSources().catch(() => {})
        this.loadGroups().catch(() => {})
        this.loadItems().catch(() => {})
      })
    },
    async loadSources() {
      this.sources = await api.getSources()
    },
    async loadGroups() {
      this.groups = await api.getGroups()
    },
    async loadItems() {
      this.loading = true
      try {
        this.items = await api.getItems(
          listParams(this.scope, useAppStore().s.filterType, this.search, 0),
        )
        this.hasMore = this.items.length >= PAGE_SIZE
      } finally {
        this.loading = false
      }
    },
    async loadMore() {
      if (this.loading || this.loadingMore || !this.hasMore) return
      this.loadingMore = true
      try {
        const page = await api.getItems(
          listParams(this.scope, useAppStore().s.filterType, this.search, this.items.length),
        )
        this.items.push(...page)
        this.hasMore = page.length >= PAGE_SIZE
      } finally {
        this.loadingMore = false
      }
    },
    async selectScope(type: Scope['type'], id: number | null = null) {
      if (type === 'all') {
        zlog.ui('Switch scope to "All Articles"')
      } else if (type === 'source' && id !== null) {
        const name = this.sourceById(id)?.title ?? `feed #${id}`
        zlog.ui(`Switch scope to feed "${name}"`)
      } else if (type === 'group' && id !== null) {
        const name = this.groups.find((g) => g.id === id)?.name ?? `folder #${id}`
        zlog.ui(`Switch scope to folder "${name}"`)
      }
      this.scope = { type, id }
      this.selectedId = null
      this.selectedItem = null
      await this.loadItems()
    },
    async setFilter(filter: number) {
      const filterNames = ['All', 'Unread', 'Starred']
      zlog.ui(`Switch filter to "${filterNames[filter] ?? filter}"`)
      await useAppStore().patch({ filterType: filter })
      await this.loadItems()
    },
    async search_(q: string) {
      const trimmed = q.trim()
      zlog.ui(trimmed ? `Search articles: "${truncate(trimmed, 40)}"` : 'Clear article search')
      this.search = q
      await this.loadItems()
    },
    async selectItem(id: number) {
      const isAlreadySelected = this.selectedItem?.id === id
      this.selectedId = id
      this.itemLoading = true
      try {
        const item = await api.getItem(id)
        // Drop stale responses when the user navigated away mid-flight.
        if (this.selectedId !== id) return
        this.selectedItem = item
        if (!isAlreadySelected) {
          const sourceTitle = this.sourceById(item.sourceId)?.title ?? 'Unknown'
          zlog.ui(`Select article "${truncate(item.title, 60)}" [Feed: "${sourceTitle}", id=${id}]`)
        }
        if (!item.hasBeenRead) await this.setItemRead(item, true)
      } finally {
        this.itemLoading = false
      }
    },
    async setItemRead(item: Item, read: boolean) {
      zlog.ui(`${read ? 'Mark as read' : 'Mark as unread'}: "${truncate(item.title, 60)}"`)
      const prev = item.hasBeenRead
      item.hasBeenRead = read
      if (this.selectedItem?.id === item.id) this.selectedItem.hasBeenRead = read
      const idx = this.items.findIndex((i) => i.id === item.id)
      if (idx >= 0) this.items[idx].hasBeenRead = read
      try {
        await api.markRead([item.id], read)
      } catch (e) {
        item.hasBeenRead = prev
        if (this.selectedItem?.id === item.id) this.selectedItem.hasBeenRead = prev
        if (idx >= 0) this.items[idx].hasBeenRead = prev
        throw e
      }
      await this.loadSources()
    },
    async toggleStar(item: Item) {
      const starred = !item.starred
      zlog.ui(`${starred ? 'Star article' : 'Unstar article'}: "${truncate(item.title, 60)}"`)
      item.starred = starred
      if (this.selectedItem?.id === item.id) this.selectedItem.starred = starred
      const idx = this.items.findIndex((i) => i.id === item.id)
      if (idx >= 0) this.items[idx].starred = starred
      try {
        await api.star(item.id, starred)
      } catch (e) {
        const prev = !starred
        item.starred = prev
        if (this.selectedItem?.id === item.id) this.selectedItem.starred = prev
        if (idx >= 0) this.items[idx].starred = prev
        throw e
      }
    },
    async markAllReadInScope() {
      let scopeDesc = 'all articles'
      if (this.scope.type === 'source' && this.scope.id) {
        scopeDesc = `feed "${this.sourceById(this.scope.id)?.title ?? this.scope.id}"`
      } else if (this.scope.type === 'group' && this.scope.id) {
        scopeDesc = `folder "${this.groups.find((g) => g.id === this.scope.id)?.name ?? this.scope.id}"`
      }
      zlog.ui(`Mark all articles as read in ${scopeDesc}`)
      await api.markAllRead(this.scope.type, this.scope.id)
      await Promise.all([this.loadSources(), this.loadItems()])
    },
    async fetchAll() {
      zlog.ui(`Manual refresh triggered (${this.sources.length} feeds)`)
      this.fetching = true
      try {
        await api.fetchSources()
      } finally {
        this.fetching = false
      }
    },
    async addSource(url: string, groupId: number | null) {
      zlog.ui(`Subscribe to feed: ${url}`)
      const source = await api.addSource(url, groupId)
      await Promise.all([this.loadSources(), this.loadItems()])
      return source
    },
    async removeSource(id: number) {
      const name = this.sourceById(id)?.title ?? `#${id}`
      zlog.ui(`Unsubscribe from feed "${name}" [id=${id}]`)
      await api.removeSource(id)
      if (this.scope.type === 'source' && this.scope.id === id) {
        await this.selectScope('all')
      }
      if (this.selectedItem?.sourceId === id) {
        this.selectedItem = null
        this.selectedId = null
      }
      await Promise.all([this.loadSources(), this.loadItems()])
    },
    async fetchFullContent(id: number) {
      const title = this.selectedItem?.id === id ? this.selectedItem.title : `item #${id}`
      zlog.ui(`Request full content for "${truncate(title, 60)}"`)
      await api.fetchFullContent(id)
      await this.selectItem(id)
      this.loadItems().catch(() => {})
    },
  },
})
