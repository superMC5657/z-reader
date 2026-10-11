import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { callsFor, commandHandlers, gate, resetInvoke } from './helpers/invokeHub'
import type { Item } from '../src/types'
import { useDataStore } from '../src/stores/data'
import { createArticleAutoFetch } from '../src/components/article/articleAutoFetch'

vi.mock('@tauri-apps/api/core', async () => {
  const hub = await import('./helpers/invokeHub')
  return { invoke: hub.mockInvoke }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}))

vi.mock('@tauri-apps/plugin-log', () => ({
  attachConsole: vi.fn(async () => {}),
  trace: vi.fn(async () => {}),
  debug: vi.fn(async () => {}),
  info: vi.fn(async () => {}),
  warn: vi.fn(async () => {}),
  error: vi.fn(async () => {}),
}))

function makeItem(overrides: Partial<Item> = {}): Item {
  return {
    id: 9,
    sourceId: 7,
    guid: 'g9',
    title: 'race',
    url: null,
    author: null,
    publishedAt: 100,
    content: 'body',
    summary: null,
    snippet: 'body',
    image: null,
    hasBeenRead: false,
    starred: false,
    hidden: false,
    remoteId: null,
    ...overrides,
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  resetInvoke()
  commandHandlers.set('get_sources', async () => [])
  commandHandlers.set('get_items', async () => [])
  commandHandlers.set('mark_read', async () => undefined)
  commandHandlers.set('star', async () => undefined)
})

describe('optimistic→queue→reload：陈旧重载不能吃掉在途乐观写', () => {
  it('loadItems 发得早落得晚（旧服务端值），提交成功后列表仍保持乐观值', async () => {
    const listed = makeItem({ hasBeenRead: false })
    const itemsGate = gate<Item[]>()
    const markGate = gate<void>()
    // 重载返回“写之前”的旧快照。
    commandHandlers.set('get_items', () => itemsGate.promise)
    commandHandlers.set('mark_read', () => markGate.promise)

    const store = useDataStore()
    store.items = [listed]

    const reloadP = store.loadItems()
    const writeP = store.setItemRead(listed, true)
    // 乐观 paint 已同步落到三处引用。
    expect(listed.hasBeenRead).toBe(true)

    itemsGate.resolve([makeItem({ hasBeenRead: false })])
    await reloadP
    // 陈旧重载落地瞬间仍被 pending 盖回。
    expect(store.items[0].hasBeenRead).toBe(true)

    markGate.resolve()
    await writeP
    expect(store.items[0].hasBeenRead).toBe(true)
    expect(listed.hasBeenRead).toBe(true)
    expect(callsFor('mark_read').length).toBe(1)
  })

  it('toggleStar 同理：starred 在途时陈旧重载不回闪', async () => {
    const listed = makeItem({ starred: false })
    const itemsGate = gate<Item[]>()
    const starGate = gate<void>()
    commandHandlers.set('get_items', () => itemsGate.promise)
    commandHandlers.set('star', () => starGate.promise)

    const store = useDataStore()
    store.items = [listed]

    const reloadP = store.loadItems()
    const writeP = store.toggleStar(listed)
    expect(listed.starred).toBe(true)

    itemsGate.resolve([makeItem({ starred: false })])
    await reloadP
    expect(store.items[0].starred).toBe(true)

    starGate.resolve()
    await writeP
    expect(store.items[0].starred).toBe(true)
  })
})

describe('optimistic→queue→reload：失败回滚盖到重载后的当前引用', () => {
  it('mark_read 失败时不仅回滚旧对象，也回滚 reload 替换后的新行', async () => {
    const listed = makeItem({ hasBeenRead: false })
    const selected = makeItem({ hasBeenRead: false })
    const itemsGate = gate<Item[]>()
    const markGate = gate<void>()
    commandHandlers.set('get_items', () => itemsGate.promise)
    commandHandlers.set('mark_read', () => markGate.promise)

    const store = useDataStore()
    store.items = [listed]
    store.selectedItem = selected

    const reloadP = store.loadItems()
    const writeP = store.setItemRead(listed, true)

    // 重载先落地：数组对象已被替换，pending 把新行也盖成 true。
    itemsGate.resolve([makeItem({ hasBeenRead: false })])
    await reloadP
    expect(store.items[0].hasBeenRead).toBe(true)

    markGate.reject(new Error('queue full'))
    await expect(writeP).rejects.toThrow('queue full')

    // 回滚必须到达当前三处引用，而不是只修 detached 的旧对象。
    expect(store.items[0].hasBeenRead).toBe(false)
    expect(listed.hasBeenRead).toBe(false)
    expect(selected.hasBeenRead).toBe(false)
  })
})

describe('已确认写不粘滞：晚于提交才开始的重载信任服务端', () => {
  it('提交完成后的 reload（服务端新值 false）不再被 settled 盖回', async () => {
    const listed = makeItem({ hasBeenRead: false })
    const store = useDataStore()
    store.items = [listed]

    await store.setItemRead(listed, true)
    expect(store.items[0].hasBeenRead).toBe(true)

    // 外部真相变化后发起的重载：开始时刻晚于确认，按服务端为准。
    commandHandlers.set('get_items', async () => [makeItem({ hasBeenRead: false })])
    await store.loadItems()
    expect(store.items[0].hasBeenRead).toBe(false)
  })
})

describe('selectItem 单行重载：在途 getItem 不吃掉已确认写', () => {
  it('getItem 在途时确认落地 → 陈旧单行不回闪，且不重复 mark_read', async () => {
    const itemGate = gate<Item>()
    commandHandlers.set('get_item', () => itemGate.promise)
    commandHandlers.set('mark_read', async () => undefined)

    const store = useDataStore()
    store.items = [makeItem({ hasBeenRead: false })]

    const selP = store.selectItem(9)
    // getItem 在途时，另一路写把同一篇标为已读并确认。
    const listed = store.items[0]
    await store.setItemRead(listed, true)
    expect(callsFor('mark_read').length).toBe(1)

    // 陈旧单行落地（服务端旧值 false）：确认晚于重载开始，必须盖回 true。
    itemGate.resolve(makeItem({ hasBeenRead: false }))
    await selP

    expect(store.selectedItem?.hasBeenRead).toBe(true)
    expect(store.items[0].hasBeenRead).toBe(true)
    // 已是已读：selectItem 的自动标已读直接跳过，不重复落 mark_read。
    expect(callsFor('mark_read').length).toBe(1)
  })
})

describe('失败回滚：reload 送来新真相时回到基线而非 paint 时 prev', () => {
  it('reload 带来 true 后写失败 → 保持 true，不被陈旧 prev 盖回 false', async () => {
    const listed = makeItem({ hasBeenRead: false })
    const itemsGate = gate<Item[]>()
    const markGate = gate<void>()
    commandHandlers.set('get_items', () => itemsGate.promise)
    commandHandlers.set('mark_read', () => markGate.promise)

    const store = useDataStore()
    store.items = [listed]

    const reloadP = store.loadItems()
    const writeP = store.setItemRead(listed, true)
    expect(listed.hasBeenRead).toBe(true)

    // reload 落地带来服务端新真相 true（与 paint 时 prev=false 不同）。
    itemsGate.resolve([makeItem({ hasBeenRead: true })])
    await reloadP
    expect(store.items[0].hasBeenRead).toBe(true)

    markGate.reject(new Error('queue full'))
    await expect(writeP).rejects.toThrow('queue full')

    // 回滚回到 reload 基线 true，而不是 paint 时的陈旧 prev=false。
    expect(store.items[0].hasBeenRead).toBe(true)
    expect(listed.hasBeenRead).toBe(true)
  })
})

describe('选择纪元与 autofetch 守卫共用深度', () => {
  it('selectItem 推进 selectionEpoch，供守卫做 id 之外的陈旧判定', async () => {
    commandHandlers.set('get_item', async (args: any) => makeItem({ id: args?.id, hasBeenRead: true }))
    const store = useDataStore()
    expect(store.nextSelectionToken()).toBe(0)
    await store.selectItem(1)
    expect(store.nextSelectionToken()).toBe(1)
    await store.selectItem(2)
    expect(store.nextSelectionToken()).toBe(2)
    expect(store.selectedId).toBe(2)
  })

  it('同 id 重选推进纪元：在途抓取结果被纪元令牌丢弃且不记失败', async () => {
    const fetchGate = gate<void>()
    let token = 1
    const deps = {
      fetchFullContent: vi.fn(() => fetchGate.promise),
      getSelectedId: vi.fn(() => 5),
      getSelectionToken: vi.fn(() => token),
      reloadItem: vi.fn(async () => undefined),
      reloadList: vi.fn(),
    }
    const ctl = createArticleAutoFetch(deps)

    const p = ctl.execute(5, false)
    // 抓取在途时同文重选：id 不变但纪元推进。
    token = 2
    fetchGate.resolve()
    await p

    expect(deps.reloadItem).not.toHaveBeenCalled()
    expect(ctl.failedIds.value.size).toBe(0)
    expect(ctl.error.value).toBe('')
  })
})
