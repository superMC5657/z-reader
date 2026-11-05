import { ref } from 'vue'
import type { Item } from '../../types'
import { isFullContentCached } from '../../lib/content'

/**
 * 自动抓取守卫深度的唯一组合面：阈值（content.isFullContentCached，字节级，
 * 语义锁死）+ 远端行 + 失败集合 + 在途 + 选择纪元，五者与缺一不可。
 * 阈值本身仍由 lib/content 实现并保持原导入兼容，这里只做组合与重导出，
 * 让守卫消费者只有一个 import 面；调用签名一律不变。
 */
export { isFullContentCached }

export interface ArticleAutoFetchDeps {
  fetchFullContent: (id: number) => Promise<unknown>
  getSelectedId: () => number | null
  /**
   * 选择纪元（data store 的 selectionEpoch）：与 selectedId 双保险，
   * 同 id 重选（纪元推进但 id 不变）也能丢弃陈旧在途结果。
   */
  getSelectionEpoch: () => number
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
 *
 * 陈旧语义与 data store 的 paint+rollback+reload 共用一个深度：
 * 抓取是“读后写”（fetchFullContent 落库再重读），写真相仍归 data，
 * 这里只保证陈旧在途结果永不盖回当前视图（selectedId + 选择纪元双判定）。
 */
export function createArticleAutoFetch(deps: ArticleAutoFetchDeps) {
  const fetching = ref(false)
  const error = ref('')
  const failedIds = ref(new Set<number>())

  function isStaleResult(id: number, epoch: number): boolean {
    return deps.getSelectedId() !== id || deps.getSelectionEpoch() !== epoch
  }

  function onItemChange(next: Item | null | undefined): Promise<void> | undefined {
    error.value = ''
    if (!next) return undefined
    if (!shouldAutoFetchArticle(next, failedIds.value, fetching.value)) return undefined
    return execute(next.id, false)
  }

  async function execute(id: number, manual: boolean): Promise<void> {
    // 单飞兜底：控制器同时只允许一个在途抓取。自动路径由 shouldAutoFetchArticle
    // 的 fetching 条件把关，手动路径由模板 :disabled 把关，这里兜住未来新调用方，
    // 避免双在途交叠（先结算方的 finally 会提前复位后者的 fetching）。
    if (fetching.value) return
    if (manual) {
      failedIds.value.delete(id)
    }
    const epoch = deps.getSelectionEpoch()
    fetching.value = true
    error.value = ''
    try {
      await deps.fetchFullContent(id)
      // 若用户在抓取期间已切换到其它文章（或同文重选推进了纪元），丢弃陈旧结果。
      if (isStaleResult(id, epoch)) return
      await deps.reloadItem(id)
      deps.reloadList()
    } catch (e) {
      if (!isStaleResult(id, epoch)) {
        error.value = String(e)
        failedIds.value.add(id)
      }
    } finally {
      // fetching 是视图局部的在途状态，不是抓取结果：无论结果是否陈旧都必须复位。
      // 控制器自身的 reloadItem（selectItem）会推进纪元，若复位也挂在陈旧判定下，
      // 每次成功抓取都会被误判为"用户切走"，fetching 永久悬挂 → 刷新按钮转圈不停。
      fetching.value = false
    }
  }

  return { fetching, error, failedIds, onItemChange, execute }
}
