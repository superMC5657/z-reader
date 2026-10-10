import { describe, expect, it } from 'vitest'
import { isFullContentCached } from '../src/lib/content'
import type { Item } from '../src/types'

function makeItem(overrides: Partial<Item> = {}): Item {
  return {
    id: 1,
    sourceId: 7,
    guid: 'g1',
    title: 't',
    url: 'https://example.com/a',
    author: null,
    publishedAt: 100,
    content: null,
    summary: null,
    snippet: null,
    image: null,
    hasBeenRead: false,
    starred: false,
    hidden: false,
    remoteId: null,
    ...overrides,
  }
}

describe('isFullContentCached 阈值锁定（阈值本身不动）', () => {
  it('空与空白正文一律视为未缓存', () => {
    expect(isFullContentCached(null)).toBe(false)
    expect(isFullContentCached(undefined)).toBe(false)
    expect(isFullContentCached(makeItem({ content: '' }))).toBe(false)
    expect(isFullContentCached(makeItem({ content: '   \n  ' }))).toBe(false)
  })

  it('没有外部链接时现有内容即全部，直接视为已缓存', () => {
    expect(isFullContentCached(makeItem({ url: null, content: '短正文' }))).toBe(true)
  })

  it('正文与摘要完全一致且短（<600）视为占位，未缓存', () => {
    const stub = 'x'.repeat(100)
    expect(isFullContentCached(makeItem({ content: stub, summary: stub }))).toBe(false)
  })

  it('正文与摘要一致但长且无截断标识视为已缓存', () => {
    const body = '正文段落。'.repeat(140)
    expect(body.length).toBeGreaterThan(600)
    expect(isFullContentCached(makeItem({ content: body, summary: body }))).toBe(true)
  })

  it('短正文（<350）带中文截断标识视为未缓存', () => {
    expect(
      isFullContentCached(makeItem({ content: '前面只给了两段……阅读全文', summary: null })),
    ).toBe(false)
  })

  it('短正文（<350）带英文截断标识视为未缓存', () => {
    expect(
      isFullContentCached(makeItem({ content: 'Teaser text… Read more', summary: null })),
    ).toBe(false)
  })

  it('短正文无截断标识视为已缓存', () => {
    expect(isFullContentCached(makeItem({ content: '短，但完整的一句话。' }))).toBe(true)
  })
})
