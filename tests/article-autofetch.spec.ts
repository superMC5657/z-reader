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

/** 真实接线：reloadItem=selectItem 每次同步推进纪元并改选中，与 data store 一致。 */
function makeDeps() {
  let epoch = 0
  let selectedId: number | null = null
  return {
    fetchFullContent: vi.fn(async (_id: number) => undefined),
    getSelectedId: vi.fn(() => selectedId),
    getSelectionEpoch: vi.fn(() => epoch),
    reloadItem: vi.fn(async (id: number) => {
      epoch += 1
      selectedId = id
    }),
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
    const deps = makeDeps()
    deps.reloadItem(5)
    deps.fetchFullContent.mockImplementation(() => gate.promise)
    const ctl = createArticleAutoFetch(deps)
    const item = makeItem()

    const p1 = ctl.onItemChange(item)
    const p2 = ctl.onItemChange(item)
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)

    gate.resolve()
    await Promise.all([p1, p2])
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)
    expect(deps.reloadItem).toHaveBeenCalledTimes(2) // 初始选中 + 自身回读
    expect(ctl.fetching.value).toBe(false)
  })

  it('失败 id 不再自动发，手动可再发一次', async () => {
    const deps = makeDeps()
    deps.reloadItem(5)
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
    const deps = makeDeps()
    deps.reloadItem(6) // 用户已选中 6，抓的却是 5
    deps.fetchFullContent.mockImplementation(() => gate.promise)
    const ctl = createArticleAutoFetch(deps)

    const p = ctl.execute(5, false)
    gate.resolve()
    await p

    expect(deps.reloadItem).toHaveBeenCalledTimes(1) // 只有初始选中，无回读
    expect(ctl.failedIds.value.size).toBe(0)
    expect(ctl.error.value).toBe('')
  })
})

describe('真实接线（getSelectionEpoch 纪元随 reloadItem 推进）下的复位语义', () => {
  it('自动抓取成功后 fetching 必须复位（刷新按钮不再转圈）', async () => {
    const deps = makeDeps()
    const ctl = createArticleAutoFetch(deps)
    const item = makeItem()

    // 模拟用户点击文章：store 先推进纪元再选中
    deps.reloadItem(5)
    await ctl.onItemChange(item)

    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)
    expect(deps.reloadItem).toHaveBeenCalledTimes(2) // 自身回读一次
    expect(ctl.fetching.value).toBe(false)
  })

  it('手动抓取成功后 fetching 也必须复位', async () => {
    const deps = makeDeps()
    const ctl = createArticleAutoFetch(deps)
    deps.reloadItem(5)

    await ctl.execute(5, true)
    expect(ctl.fetching.value).toBe(false)
  })

  it('抓取失败时 fetching 复位且记失败（纪元未推进）', async () => {
    const deps = makeDeps()
    deps.fetchFullContent.mockRejectedValueOnce(new Error('net down'))
    const ctl = createArticleAutoFetch(deps)
    deps.reloadItem(5)

    await ctl.execute(5, false)
    expect(ctl.fetching.value).toBe(false)
    expect(ctl.failedIds.value.has(5)).toBe(true)
  })

  it('真陈旧（抓取期间切走）：结果丢弃，但 fetching 仍要复位', async () => {
    const deps = makeDeps()
    const ctl = createArticleAutoFetch(deps)
    let release!: () => void
    deps.fetchFullContent.mockImplementation(
      () => new Promise<void>((r) => (release = r)),
    )

    deps.reloadItem(5)
    const p = ctl.onItemChange(makeItem())
    // 用户切走：selectedId 与纪元同时推进
    deps.reloadItem(6)
    release()
    await p

    expect(deps.reloadItem).toHaveBeenCalledTimes(2) // 没有第三次回读
    expect(ctl.error.value).toBe('')
    expect(ctl.fetching.value).toBe(false)
  })

  it('单飞兜底：在途时再调 execute 直接返回，不叠加抓取', async () => {
    const deps = makeDeps()
    const ctl = createArticleAutoFetch(deps)
    let release!: () => void
    deps.fetchFullContent.mockImplementation(
      () => new Promise<void>((r) => (release = r)),
    )
    deps.reloadItem(5)

    const p1 = ctl.execute(5, false)
    const p2 = ctl.execute(5, true) // 在途期间的手动调用：兜底吞掉
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)
    release()
    await Promise.all([p1, p2])
    expect(deps.fetchFullContent).toHaveBeenCalledTimes(1)
    expect(ctl.fetching.value).toBe(false)
  })
})

describe('storm 回归：云端短 body 不自动抓，本地同形抓一次', () => {
  it('heuristic 同判 false 时云端零 invoke、本地一次 invoke', async () => {
    const deps = makeDeps()
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
