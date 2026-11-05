# 02：控制器自身 reloadItem reject 时，失败被陈旧判定静默吞掉

Status: needs-triage

## 症状

`execute()` 成功路径里控制器会调自己的 `reloadItem`（= `data.selectItem`，其内部 `getItem` / `setItemRead` 链可能 reject）。此时纪元已被自身推进，`catch` 里的 `isStaleResult(id, epoch)` 判为陈旧 → `error` / `failedIds` 都不记录，抓取实际失败但用户看不到任何提示（`fetching` 至少已正确复位，不会转圈）。

## 机理

- `selectItem` 先推进 `selectionEpoch` 再 await `getItem`（`data.ts:214`），所以"自身回读失败"与"用户切走"在纪元上不可区分。
- `catch` 的陈旧门控本意是"不属于当前视图的失败不记账"，但把控制器自己发起的写路径失败也一并吞了。
- 早于 2026-11-06 的转圈修复就存在；那次修复只保证了 `fetching` 复位，未触碰 catch 门控。

## 证据

Standards review（code-review 双轴，2026-11-06）adjacent edge："if the controller's own `reloadItem` rejects … the catch's isStaleResult sees the already-advanced epoch and silently swallows error/failedIds recording"。

## 修复方向（未拍板）

- 方向一：`reloadItem` 的失败单独分类——它不参与陈旧判定（陈旧语义只管 `fetchFullContent` 的结果），失败照常记 `error`。
- 方向二：`execute` 里把 `fetchFullContent` 与 `reloadItem` 分成两段 try，各配各的陈旧门控。

注意与工单 01 的耦合：若 01 采取"结算后补判"方案，本票的失败记录会影响补判是否触发，宜先拍板 01 再动本票。

## 验收

- 真实接线（`makeDeps()`）下新增用例：`reloadItem` reject → `error` 非空且 `failedIds` 记账（或按拍板后的语义断言），`fetching` 复位。
- 不回归：真陈旧（抓取期间切走）路径仍不记账。
