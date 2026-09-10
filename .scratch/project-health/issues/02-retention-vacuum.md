# 02 — 保留清理单语句化与 VACUUM 策略

Status: resolved

`cleanup_retention` 的每源上限删除是 N+1（逐源 `DELETE … LIMIT`），改为单条
窗口函数语句（`ROW_NUMBER() OVER (PARTITION BY source_id …)`，语义与原来
“每源保留最新 N 篇（含收藏占位）、只删未收藏”一致，既有
`test_retention_cleanup` 原样通过）。`cleanup_now` 零删除时跳过 VACUUM；
刷新路径保留“删除 >200 才 VACUUM”阈值。

验证：`cargo test test_retention_cleanup` 通过。

## Comments

- 2026-09-10：随 P2 余量批次落地。
