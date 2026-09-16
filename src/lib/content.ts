import type { Item } from '../types'

/**
 * Checks if the article's full content is already cached in local SQLite.
 *
 * Returns true if:
 * 1. The item has valid content, AND
 * 2. It is not just an un-extracted RSS summary stub (e.g. from an RSS feed where
 *    full text was never provided in the feed XML and was duplicated into content).
 *
 * If this returns false and the item has an external URL, the reader will
 * automatically fetch and cache the full article text.
 */
export function isFullContentCached(item: Item | null | undefined): boolean {
  if (!item || !item.content || !item.content.trim()) return false
  if (!item.url) return true // No external link to fetch from, so existing content is all there is

  const contentTrimmed = item.content.trim()
  const summaryTrimmed = item.summary?.trim()

  // If content is literally identical to summary, and is short with a URL,
  // it is an un-extracted RSS fallback summary stub stored during feed sync.
  if (summaryTrimmed && contentTrimmed === summaryTrimmed) {
    if (contentTrimmed.length < 600 || isStubText(contentTrimmed)) {
      return false
    }
  }

  // Short content ending with or containing typical truncated excerpt stub markers
  if (contentTrimmed.length < 350 && isStubText(contentTrimmed)) {
    return false
  }

  return true
}

/**
 * Check if the text matches common RSS excerpt stub endings.
 */
export function isStubText(text: string): boolean {
  return /查看全文|阅读全文|继续阅读|原文链接|阅读更多|Read more|Continue reading/i.test(text)
}
