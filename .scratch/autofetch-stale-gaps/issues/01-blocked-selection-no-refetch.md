# 01：在途抓取期间选中的新文章不会被自动抓，且结算后不补触发

Status: needs-triage

## 症状

文章 A 全文抓取在途时，用户点开未缓存的文章 B：`onItemChange(B)` 被 `shouldAutoFetchArticle` 的 `fetching` 条件拒绝；A 结算后 `fetching` 复位，但 B 的 `watch(item)` 不会重发——B 停留在无正文/手动抓取状态，直到用户重选一次。

## 机理

- `ArticleView.vue` 的 `watch(item)` 只在 `selectedItem` 变化时触发一次；被拒的那次 `onItemChange` 返回值被丢弃，没有任何重试记忆。
- `execute()` 的 `finally` 只复位 `fetching`，不知道"刚才有一个因在途而被拒的选中"。
- 该缺口早于 2026-11-06 的转圈修复就存在；那次修复（finally 无条件复位）反而把旧的"fetching 永久悬挂"窗口收窄了，但补触发仍缺失。

## 证据

Spec review（code-review 双轴，2026-11-06）finding 2："when the stale fetch settles and `fetching` resets, the watch never re-fires, so article B's auto-fetch is silently skipped"。现有测试未覆盖"在途期间 onItemChange 被拒 → 结算后恢复"链路。

## 修复方向（未拍板）

- 方向一：`onItemChange` 被在途拒绝时记住 pending 选中（如 `pendingId`），`finally` 复位后对当前选中补跑一次触发谓词。
- 方向二：`finally` 复位后直接对 `deps.getSelectedId()` 当前值补判一次（无 pending 记忆，语义更简单，但会多一次谓词评估）。

## 验收

- `tests/article-autofetch.spec.ts` 真实接线（`makeDeps()`）下新增用例：在途期间 `onItemChange(B)` 被拒 → A 结算 → B 的抓取自动补发一次且 `fetching` 最终复位。
- 不回归：同一 id 并发只发一次（`storm` 与并发用例仍绿）。
