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

/**
 * 标记位写深度（data 模块拥有 paint+rollback+reload 的唯一真相）：
 *
 * - paint：一次写同时落到“三处引用”（入参对象、selectedItem、items 行），
 *   登记 pending，供后续 reload 对账；
 * - rollback：后端失败时按 seq 回滚；若已有更新的同 key 写入覆盖则让路，
 *   回滚时按当前引用重解（reload 可能已替换数组对象）；若 paint 之后有
 *   reload 落地并盖过服务端值，则回到 reload 快照的基线而非 paint 时的 prev；
 * - reload：所有重载（loadItems/loadMore 的列表、selectItem 的 getItem 单行）
 *   落地前快照 opSeq，落地后经 reconcile 把未确认的乐观值与“比重载开始更晚”
 *   确认的已确认值重新盖回去，同时为在途 pending 快照服务端基线；晚于写入
 *   确认时刻才开始的重载一律信任服务端，并剪掉过期的 settled 登记，避免与
 *   托盘/同步等外部真相打架。
 */
type FlagKey = 'hasBeenRead' | 'starred'

interface FlagLedgerEntry {
  seq: number
  value: boolean
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
    /** selectItem 进入次数：autofetch 陈旧守卫的共享令牌源（只增不减）。 */
    selectionEpoch: 0,
    /** flag 写与 reload 的统一单调序列：写 paint 时取值，reload 开始时快照。 */
    opSeq: 0,
    /** 未确认的乐观写：reload 对账时一律盖回当前引用。 */
    flagPending: {} as Record<number, Partial<Record<FlagKey, FlagLedgerEntry>>>,
    /** 已确认的写：只盖回“比重载开始更晚”的条目，随后由信任服务端的重载剪枝。 */
    flagSettled: {} as Record<number, Partial<Record<FlagKey, FlagLedgerEntry>>>,
    /**
     * reload 落地时被在途 pending 覆盖掉的服务端值（按 seq 配对）：
     * 写随后失败则回滚到该基线，而非 paint 时可能已陈旧的 prev。
     */
    flagReloadBaseline: {} as Record<number, Partial<Record<FlagKey, FlagLedgerEntry>>>,
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
      // 托盘菜单“全部标记为已读”等 Rust 后端状态变更。
      await listen<unknown>('unread-changed', () => {
        this.loadSources().catch(() => {})
        this.loadItems().catch(() => {})
      })
      // 备份恢复操作在底层替换了数据库文件。
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
      const reloadMark = this.opSeq
      try {
        this.items = await api.getItems(
          listParams(this.scope, useAppStore().s.filterType, this.search, 0),
        )
        this.reconcileReloadedFlags(reloadMark)
        this.hasMore = this.items.length >= PAGE_SIZE
      } finally {
        this.loading = false
      }
    },
    async loadMore() {
      if (this.loading || this.loadingMore || !this.hasMore) return
      this.loadingMore = true
      const reloadMark = this.opSeq
      try {
        const page = await api.getItems(
          listParams(this.scope, useAppStore().s.filterType, this.search, this.items.length),
        )
        this.items.push(...page)
        this.reconcileReloadedFlags(reloadMark)
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
      this.selectionEpoch += 1
      this.selectedId = id
      this.itemLoading = true
      // getItem 单行即一次重载：先快照 opSeq，落地后走与列表重载同一对账，
      // 否则在途确认的已确认写会被陈旧单行盖掉并触发重复 mark。
      const reloadMark = this.opSeq
      try {
        const item = await api.getItem(id)
        // 如果用户在网络请求未完成时已切换到其它文章，丢弃陈旧的响应。
        if (this.selectedId !== id) return
        this.selectedItem = item
        // 单行重载对账：pending 一律盖回，settled 只盖回比重载开始更晚的；
        // 已是已读则下面的自动标已读直接跳过，不重复落 mark。
        this.reconcileReloadedFlags(reloadMark, id)
        if (!isAlreadySelected) {
          const sourceTitle = this.sourceById(item.sourceId)?.title ?? 'Unknown'
          zlog.ui(`Select article "${truncate(item.title, 60)}" [Feed: "${sourceTitle}", id=${id}]`)
        }
        if (!this.selectedItem.hasBeenRead) await this.setItemRead(this.selectedItem, true)
      } finally {
        this.itemLoading = false
      }
    },
    async setItemRead(item: Item, read: boolean) {
      zlog.ui(`${read ? 'Mark as read' : 'Mark as unread'}: "${truncate(item.title, 60)}"`)
      const { seq, rollback } = this.paintItemFlag(item, 'hasBeenRead', read)
      try {
        await api.markRead([item.id], read)
      } catch (e) {
        rollback()
        throw e
      }
      this.confirmFlagWrite(item.id, 'hasBeenRead', seq)
      await this.loadSources()
    },
    async toggleStar(item: Item) {
      const starred = !item.starred
      zlog.ui(`${starred ? 'Star article' : 'Unstar article'}: "${truncate(item.title, 60)}"`)
      const { seq, rollback } = this.paintItemFlag(item, 'starred', starred)
      try {
        await api.star(item.id, starred)
      } catch (e) {
        rollback()
        throw e
      }
      this.confirmFlagWrite(item.id, 'starred', seq)
    },
    /** 当前选择纪元：autofetch 控制器可选的陈旧令牌源（与 selectedId 双保险）。 */
    nextSelectionToken(): number {
      return this.selectionEpoch
    },
    /**
     * 三处引用一次 paint 并登记 pending，返回本次 seq 与回滚函数。
     * 唯一 paint 出口：setItemRead/toggleStar 只允许经此写入标记位。
     */
    paintItemFlag<K extends FlagKey>(item: Item, key: K, value: boolean) {
      const prev = item[key]
      const seq = ++this.opSeq
      const per = (this.flagPending[item.id] ??= {})
      per[key] = { seq, value }
      this.applyFlagToCurrentRefs(item.id, key, value, item)
      const rollback = () => {
        const cur = this.flagPending[item.id]?.[key]
        // 已有更新的同 key 写入覆盖：让路，不回滚。
        if (cur && cur.seq !== seq) return
        const settled = this.flagSettled[item.id]?.[key]
        // 已有更新的同 key 确认写入：同样让路。
        if (settled && settled.seq > seq) {
          const bucket = this.flagPending[item.id]
          if (bucket) {
            delete bucket[key]
            if (Object.keys(bucket).length === 0) delete this.flagPending[item.id]
          }
          return
        }
        const bucket = this.flagPending[item.id]
        if (bucket) {
          delete bucket[key]
          if (Object.keys(bucket).length === 0) delete this.flagPending[item.id]
        }
        // paint 之后若有 reload 落地并盖过服务端值：回到该基线而非陈旧 prev。
        const bBucket = this.flagReloadBaseline[item.id]
        const bEntry = bBucket?.[key]
        let target = prev
        if (bBucket && bEntry) {
          if (bEntry.seq === seq) target = bEntry.value
          // 无论命中与否，该基线都已结算，随手清理防泄漏。
          delete bBucket[key]
          if (Object.keys(bBucket).length === 0) delete this.flagReloadBaseline[item.id]
        }
        this.applyFlagToCurrentRefs(item.id, key, target, item)
      }
      return { seq, rollback }
    },
    /** 后端确认后把 pending 转 settled：reload 对账与剪枝的唯一入口。 */
    confirmFlagWrite(id: number, key: FlagKey, seq: number) {
      const cur = this.flagPending[id]?.[key]
      if (cur && cur.seq === seq) {
        const bucket = this.flagPending[id]
        if (bucket) {
          delete bucket[key]
          if (Object.keys(bucket).length === 0) delete this.flagPending[id]
        }
        const slot = (this.flagSettled[id] ??= {})
        slot[key] = { seq, value: cur.value }
        // 确认成功：同代 reload 基线作废，随手清理防泄漏。
        const bBucket = this.flagReloadBaseline[id]
        const bEntry = bBucket?.[key]
        if (bBucket && bEntry && bEntry.seq === seq) {
          delete bBucket[key]
          if (Object.keys(bBucket).length === 0) delete this.flagReloadBaseline[id]
        }
      }
    },
    /** 把三处当前引用（selectedItem、items 行，另加 detached 的入参对象）写成同一值。 */
    applyFlagToCurrentRefs(id: number, key: FlagKey, value: boolean, extra?: Item) {
      if (extra) extra[key] = value
      if (this.selectedItem && this.selectedItem.id === id) {
        this.selectedItem[key] = value
      }
      const row = this.items.find((i) => i.id === id)
      if (row) row[key] = value
    },
    /** 仅盖回未确认的乐观写（reconcile 的前半步； settled 对账与剪枝由调用方继续）。 */
    reapplyPendingFlags() {
      for (const idStr of Object.keys(this.flagPending)) {
        const id = Number(idStr)
        const per = this.flagPending[id]
        if (!per) continue
        for (const key of ['hasBeenRead', 'starred'] as const) {
          const w = per[key]
          if (w) this.applyFlagToCurrentRefs(id, key, w.value)
        }
      }
    },
    /**
     * 重载对账：pending 一律盖回；settled 只盖回比重载开始更晚的
     * （重载发得早、落得晚，陈旧服务端值不能吃掉已确认的写）；
     * 随后剪掉不晚于本次重载开始的 settled——这类重载晚于写，
     * 服务端已含真相，后续一律信任服务端。
     * 为在途 pending 快照本次重载带来的服务端基线：写随后失败则
     * 回滚到该基线而非 paint 时的陈旧 prev。
     * freshId 有值时为单行重载（selectItem/getItem），服务端真相只取
     * 新 selectedItem；缺省为列表重载，真相取 items 新行。
     */
    reconcileReloadedFlags(reloadMark: number, freshId?: number) {
      if (freshId !== undefined) {
        const fresh =
          this.selectedItem && this.selectedItem.id === freshId ? this.selectedItem : undefined
        if (fresh) {
          const per = this.flagPending[freshId]
          if (per) {
            for (const key of ['hasBeenRead', 'starred'] as const) {
              const w = per[key]
              if (w) {
                const slot = (this.flagReloadBaseline[freshId] ??= {})
                slot[key] = { seq: w.seq, value: fresh[key] }
              }
            }
          }
        }
      } else {
        for (const row of this.items) {
          const per = this.flagPending[row.id]
          if (!per) continue
          for (const key of ['hasBeenRead', 'starred'] as const) {
            const w = per[key]
            if (w) {
              const slot = (this.flagReloadBaseline[row.id] ??= {})
              slot[key] = { seq: w.seq, value: row[key] }
            }
          }
        }
      }
      this.reapplyPendingFlags()
      for (const idStr of Object.keys(this.flagSettled)) {
        const id = Number(idStr)
        const per = this.flagSettled[id]
        if (!per) continue
        for (const key of ['hasBeenRead', 'starred'] as const) {
          const w = per[key]
          if (!w) continue
          if (w.seq > reloadMark) {
            this.applyFlagToCurrentRefs(id, key, w.value)
          } else {
            delete per[key]
          }
        }
        if (Object.keys(per).length === 0) delete this.flagSettled[id]
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
