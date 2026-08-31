import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import * as api from '../lib/tauri'
import type { Group, Item, Source } from '../types'
import { useAppStore } from './app'

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
        this.items = await api.getItems({
          scope: this.scope.type,
          scopeId: this.scope.id,
          filter: useAppStore().s.filterType,
          search: this.search || undefined,
          limit: 300,
        })
      } finally {
        this.loading = false
      }
    },
    async selectScope(type: Scope['type'], id: number | null = null) {
      this.scope = { type, id }
      this.selectedId = null
      this.selectedItem = null
      await this.loadItems()
    },
    async setFilter(filter: number) {
      await useAppStore().patch({ filterType: filter })
      await this.loadItems()
    },
    async search_(q: string) {
      this.search = q
      await this.loadItems()
    },
    async selectItem(id: number) {
      this.selectedId = id
      this.itemLoading = true
      try {
        const item = await api.getItem(id)
        this.selectedItem = item
        if (!item.hasBeenRead) await this.setItemRead(item, true)
      } finally {
        this.itemLoading = false
      }
    },
    async setItemRead(item: Item, read: boolean) {
      await api.markRead([item.id], read)
      item.hasBeenRead = read
      if (this.selectedItem?.id === item.id) this.selectedItem.hasBeenRead = read
      const idx = this.items.findIndex((i) => i.id === item.id)
      if (idx >= 0) this.items[idx].hasBeenRead = read
      await this.loadSources()
    },
    async toggleStar(item: Item) {
      const starred = !item.starred
      await api.star(item.id, starred)
      item.starred = starred
      if (this.selectedItem?.id === item.id) this.selectedItem.starred = starred
      const idx = this.items.findIndex((i) => i.id === item.id)
      if (idx >= 0) this.items[idx].starred = starred
    },
    async markAllReadInScope() {
      await api.markAllRead(this.scope.type, this.scope.id)
      await Promise.all([this.loadSources(), this.loadItems()])
    },
    async fetchAll() {
      this.fetching = true
      try {
        await api.fetchSources()
      } finally {
        this.fetching = false
      }
    },
    async addSource(url: string, groupId: number | null) {
      const source = await api.addSource(url, groupId)
      await Promise.all([this.loadSources(), this.loadItems()])
      return source
    },
    async removeSource(id: number) {
      await api.removeSource(id)
      if (this.scope.type === 'source' && this.scope.id === id) {
        await this.selectScope('all')
      }
      await Promise.all([this.loadSources(), this.loadItems()])
    },
    async fetchFullContent(id: number) {
      await api.fetchFullContent(id)
      await this.selectItem(id)
      this.loadItems().catch(() => {})
    },
  },
})
