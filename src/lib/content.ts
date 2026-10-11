import type { Item } from '../types'

/**
 * 全文缓存阈值的纯函数核心（字节级判定，无 store、无 IO）。
 * 阈值语义锁死：任何调参都必须同步更新 tests/content.spec.ts。
 *
 * 本模块是自动抓取守卫深度（articleAutoFetch.ts）的阈值下半：
 * 组合语义（远端行/失败集合/在途抓取/选择纪元）由守卫拥有，
 * 这里只回答“这段正文像不像已缓存的全文”。守卫经由唯一的
 * `isFullContentCached` 入口取阈值，不允许各调用方自造副本。
 *
 * 检查文章的全文内容是否已缓存在本地 SQLite 数据库中。
 *
 * 返回 true 的条件：
 * 1. 条目具有有效的内容，且
 * 2. 该内容不仅是未抓取全文的 RSS 摘要占位（例如某些 RSS 源在 XML 中未提供全文，直接将摘要复制到了 content 字段）。
 *
 * 如果返回 false 且该条目包含外部 URL，阅读器将自动在后台抓取并缓存文章全文。
 */
export function isFullContentCached(item: Item | null | undefined): boolean {
  if (!item || !item.content || !item.content.trim()) return false
  if (!item.url) return true // 没有外部链接可抓取，现有内容即为全部内容

  const contentTrimmed = item.content.trim()
  const summaryTrimmed = item.summary?.trim()

  // 如果正文与摘要完全一致，且长度较短并带有 URL，
  // 说明这是订阅同步时存入的未抽取 RSS 回退摘要占位。
  if (summaryTrimmed && contentTrimmed === summaryTrimmed) {
    if (contentTrimmed.length < 600 || isStubText(contentTrimmed)) {
      return false
    }
  }

  // 较短的正文若以典型的截断摘要标志结尾或包含截断标识，则视为未抓取全文
  if (contentTrimmed.length < 350 && isStubText(contentTrimmed)) {
    return false
  }

  return true
}

/**
 * 检查文本是否匹配常见的 RSS 截断摘要结尾提示语。
 */
function isStubText(text: string): boolean {
  return /查看全文|阅读全文|继续阅读|原文链接|阅读更多|Read more|Continue reading/i.test(text)
}
