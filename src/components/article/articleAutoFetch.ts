import { ref } from 'vue'
import type { Item } from '../../types'
import { isFullContentCached } from '../../lib/content'

export interface ArticleAutoFetchDeps {
  fetchFullContent: (id: number) => Promise<unknown>
  getSelectedId: () => number | null
  reloadItem: (id: number) => Promise<unknown>
  reloadList: () => void
}

/**
 * 自动抓取触发谓词：只有同时满足才允许后台自动抓——
 * 有正文链接、本地行（remoteId 为空，云端行只走手动）、
 * 启发式判定未缓存、本次视图内未失败过、当前没有正在进行的抓取。
 * 阈值语义完全由 isFullContentCached 拥有，这里只做组合。
 */
export function shouldAutoFetchArticle(
  item: Item | null | undefined,
  failedIds: Set<number>,
  fetching: boolean,
): boolean {
  if (!item || !item.url) return false
  if (item.remoteId != null) return false
  if (isFullContentCached(item)) return false
  if (failedIds.has(item.id) || fetching) return false
  return true
}

/**
 * 阅读器自动抓取的视图局部控制器：失败集合只活在当前视图实例里，
 * 不进 store、不持久化，切走即忘。行为与原内联实现逐行等价。
 */
export function createArticleAutoFetch(deps: ArticleAutoFetchDeps) {
  const fetching = ref(false)
  const error = ref('')
  const failedIds = ref(new Set<number>())

  function onItemChange(next: Item | null | undefined): Promise<void> | undefined {
    error.value = ''
    if (!next) return undefined
    if (!shouldAutoFetchArticle(next, failedIds.value, fetching.value)) return undefined
    return execute(next.id, false)
  }

  async function execute(id: number, manual: boolean): Promise<void> {
    if (manual) {
      failedIds.value.delete(id)
    }
    fetching.value = true
    error.value = ''
    try {
      await deps.fetchFullContent(id)
      // 若用户在抓取期间已切换到其它文章，丢弃陈旧结果。
      if (deps.getSelectedId() !== id) return
      await deps.reloadItem(id)
      deps.reloadList()
    } catch (e) {
      if (deps.getSelectedId() === id) {
        error.value = String(e)
        failedIds.value.add(id)
      }
    } finally {
      if (deps.getSelectedId() === id) fetching.value = false
    }
  }

  return { fetching, error, failedIds, onItemChange, execute }
}
