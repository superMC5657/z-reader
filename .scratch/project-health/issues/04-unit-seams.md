# 04 — 可测性接缝与专项测试

Status: resolved

- `lib.rs` 聚合循环抽出 `merge_outcome` + 单测（成功/失败/通知混合折叠）。
- `import_backup` 文件替换抽出 `backup::replace_live` + 单测（成功替换 /
  缺失 staged 时原库 intact）。
- `test_backfill_pages_through_thousands`：2500 行三页回填，命中一半。
- `test_items_pagination_is_stable`：limit/offset 三页无重无漏且有序。

未覆盖（诚实记录）：`refresh_one_source` 并发编排需 AppHandle，只做了
聚合函数级测试；恢复全流程（对话框 + 连接重开）需真机手动验证。

## Comments

- 2026-09-10：随 P2 余量批次落地。
