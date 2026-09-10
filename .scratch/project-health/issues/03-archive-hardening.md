# 03 — 备份解压加固与上限

Status: resolved

`extract_archive` 追加：`enclosed_name()` 权威穿越校验（原手动 `..`/绝对路径
检查保留为第一道）、`is_symlink()` 条目跳过、条目数 ≤20000 / 总量 ≤1GB /
单文件 ≤512MB 上限。新增 `test_extract_skips_traversal_entries`。

已知边界：公共 writer API 会抹掉 symlink 文件类型位（`unix_permissions`
只保留 `0o777`），所以 symlink 跳过以代码审查 + `is_symlink` 存在性保证，
单测覆盖的是可达的 traversal 形状。

## Comments

- 2026-09-10：随 P2 余量批次落地。
