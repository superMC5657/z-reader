# spec：autofetch 控制器单飞设计的两个 pre-existing 缺口

2026-11-06 code review（转圈修复的 uncommitted diff，双轴 Standards/Spec）在确认修复本身正确之外，各自点名一个**早于本次修复就存在**的行为缺口。两者都在 `createArticleAutoFetch` 的单飞（同时只允许一个在途抓取）设计之内，修复它们超出当次修复契约，单独立票。

共同背景：

- 控制器：`src/components/article/articleAutoFetch.ts`；真实接线在 `ArticleView.vue`（`reloadItem = data.selectItem`，`selectItem` 每次调用无条件推进 `selectionEpoch`）。
- 单飞守卫现已收敛到 `execute()` 顶部 `if (fetching.value) return`；自动路径由 `shouldAutoFetchArticle` 的 `fetching` 条件把关，手动路径由模板 `:disabled` 把关。
- 测试缝：`tests/article-autofetch.spec.ts` 的 `makeDeps()`（真实接线：纪元随 reloadItem 推进）。

## Tickets

- `issues/01-blocked-selection-no-refetch.md` — 在途抓取期间选中的新文章不会被自动抓，且在途结算后不补触发（Spec review 发现）。
- `issues/02-own-reload-reject-swallowed.md` — 控制器自身 reloadItem reject 时失败被陈旧判定吞掉（Standards review 发现）。
