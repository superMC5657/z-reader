import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  createArticleAutoFetch,
  shouldAutoFetchArticle,
} from '../src/components/article/articleAutoFetch'
import type { Item } from '../src/types'

function makeItem(overrides: Partial<Item> = {}): Item {
  return {
    id: 5,
    sourceId: 7,
    guid: 'g5',
    title: 't',
    url: 'https://example.com/a',
    author: null,
    publishedAt: 100,
    content: 'x'.repeat(40),
    summary: 'x'.repeat(40),
    snippet: null,
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

function makeDeps(selectedId: number | (() => number | null)) {
  const getSelectedId = typeof selectedId === 'function' ? selectedId : () => selectedId
  return {
    fetchFullContent: vi.fn(async (_id: number) => undefined),
    getSelectedId: vi.fn(getSelectedId),
    reloadItem: vi.fn(async (_id: number) => undefined),
    reloadList: vi.fn(),
  }
}

beforeEach(() => {
  vi.restoreAllMocks()
})

describe('shouldAutoFetchArticle 触发谓词', () => {
  it('空条目、无链接、已缓存、失败过、抓取中都不自动抓', () => {
    const uncached = makeItem()
    expect(shouldAutoFetchArticle(uncached, new Set(), false)).toBe(true)
    expect(shouldAutoFetchArticle(null, new Set(), false)).toBe(false)
    expect(shouldAutoFetchArticle(makeItem({ url: null }), new Set(), false)).toBe(false)
    expect(
      shouldAutoFetchArticle(makeItem({ content: '完整长正文'.repeat(100) }), new Set(), false),
    ).toBe(false)
    expect(shouldAutoFetchArticle(uncached, new Set([5]), false)).toBe(false)
    expect(shouldAutoFetchArticle(uncached, new Set(), true)).toBe(false)
  })

  it('云端行（remoteId 非空）绝不自动抓，只走手动', () => {
    expect(shouldAutoFetchArticle(makeItem({ remoteId: 'abc123' }), new Set(), false)).toBe(false)
  })
})

describe('自动抓取并发与失败语义', () => {
  it('同一 id 并发触发只发一次抓取', async () => {
    const gate = deferred<void>()
    const deps = makeDeps(5)
    deps.fetchFullContent.mockImplementation(() => gate.promise)
    const ctl = createArticleAutoFetch(deps)
    const item = makeItem()

    const p1 = ctl.onItemChange(item)
    const p2 = ctl.onItemChange(item)
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)

    gate.resolve()
    await Promise.all([p1, p2])
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)
    expect(deps.reloadItem).toHaveBeenCalledTimes(1)
    expect(ctl.fetching.value).toBe(false)
  })

  it('失败 id 不再自动发，手动可再发一次', async () => {
    const deps = makeDeps(5)
    deps.fetchFullContent.mockRejectedValueOnce(new Error('net down'))
    const ctl = createArticleAutoFetch(deps)
    const item = makeItem()

    await ctl.execute(5, false)
    expect(ctl.failedIds.value.has(5)).toBe(true)
    expect(ctl.error.value).toContain('net down')

    // 自动触发被失败集合挡住，不再打后端
    ctl.onItemChange(item)
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)

    // 手动重试：先清失败态，再发一次
    deps.fetchFullContent.mockResolvedValueOnce(undefined)
    await ctl.execute(5, true)
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(2)
    expect(ctl.failedIds.value.has(5)).toBe(false)
  })

  it('抓取期间切走文章：陈旧结果丢弃且不记失败', async () => {
    const gate = deferred<void>()
    const deps = makeDeps(() => 6)
    deps.fetchFullContent.mockImplementation(() => gate.promise)
    const ctl = createArticleAutoFetch(deps)

    const p = ctl.execute(5, false)
    gate.resolve()
    await p

    expect(deps.reloadItem).not.toHaveBeenCalled()
    expect(ctl.failedIds.value.size).toBe(0)
    expect(ctl.error.value).toBe('')
  })
})

describe('storm 回归：云端短 body 不自动抓，本地同形抓一次', () => {
  it('heuristic 同判 false 时云端零 invoke、本地一次 invoke', async () => {
    const deps = makeDeps(5)
    const ctl = createArticleAutoFetch(deps)
    const shortBody = 'y'.repeat(40)
    const cloud = makeItem({ id: 5, content: shortBody, summary: shortBody, remoteId: 'abc123' })
    const local = makeItem({ id: 6, content: shortBody, summary: shortBody, remoteId: null })
    const selected = vi.fn(() => 6)
    deps.getSelectedId.mockImplementation(selected)

    ctl.onItemChange(cloud)
    expect(deps.fetchFullContent).not.toHaveBeenCalled()

    await ctl.onItemChange(local)
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)
    expect(deps.fetchFullContent).toHaveBeenCalledWith(6)
  })
})
