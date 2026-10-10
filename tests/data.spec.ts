import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { commandHandlers, invokeCalls, resetInvoke } from './helpers/invokeHub'
import type { Item } from '../src/types'
import { useDataStore } from '../src/stores/data'

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
    id: 1,
    sourceId: 7,
    guid: 'g1',
    title: 'hello',
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

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

beforeEach(() => {
  setActivePinia(createPinia())
  resetInvoke()
  commandHandlers.set('get_sources', async () => [])
  commandHandlers.set('mark_read', async () => undefined)
  commandHandlers.set('star', async () => undefined)
})

describe('selectItem 竞态守卫', () => {
  it('先返回的陈旧响应被丢弃，只保留最后一次选中的文章', async () => {
    const first = deferred<Item>()
    const second = deferred<Item>()
    commandHandlers.set('get_item', async (args: any) =>
      args?.id === 1 ? first.promise : second.promise,
    )

    const store = useDataStore()
    const p1 = store.selectItem(1)
    const p2 = store.selectItem(2)
    second.resolve(makeItem({ id: 2, title: 'second' }))
    first.resolve(makeItem({ id: 1, title: 'first-stale' }))
    await Promise.all([p1, p2])

    expect(store.selectedId).toBe(2)
    expect(store.selectedItem?.id).toBe(2)
    expect(store.selectedItem?.title).toBe('second')
    expect(store.itemLoading).toBe(false)
    // 陈旧文章不触发自动标已读，只有命中项走 mark_read
    expect(invokeCalls.filter((c) => c.cmd === 'mark_read').length).toBe(1)
  })
})

describe('setItemRead 乐观回滚', () => {
  it('后端失败时三处引用都恢复原值并向外抛错', async () => {
    const listed = makeItem({ id: 9, hasBeenRead: false })
    const selected = makeItem({ id: 9, hasBeenRead: false })
    commandHandlers.set('mark_read', async () => {
      throw new Error('db locked')
    })

    const store = useDataStore()
    store.items = [listed]
    store.selectedItem = selected

    await expect(store.setItemRead(listed, true)).rejects.toThrow('db locked')
    expect(listed.hasBeenRead).toBe(false)
    expect(selected.hasBeenRead).toBe(false)
    expect(store.items[0].hasBeenRead).toBe(false)
  })

  it('后端成功时三处引用一起落盘并刷新来源', async () => {
    const listed = makeItem({ id: 9, hasBeenRead: false })
    const selected = makeItem({ id: 9, hasBeenRead: false })
    commandHandlers.set('get_sources', async () => [])

    const store = useDataStore()
    store.items = [listed]
    store.selectedItem = selected

    await store.setItemRead(listed, true)
    expect(listed.hasBeenRead).toBe(true)
    expect(selected.hasBeenRead).toBe(true)
    expect(store.items[0].hasBeenRead).toBe(true)
    expect(invokeCalls.some((c) => c.cmd === 'get_sources')).toBe(true)
  })
})
