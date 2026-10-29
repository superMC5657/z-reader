# 07 — greader 错误分类去重 + rules evaluate 收敛

Status: ready-for-agent

Blocked by: 01

## 问题（Duplicated Code，judgement call）

1. `greader.rs` 中 `if e.starts_with("auth failed:")` 错误分类闭包重复 4 次：`read_authed`(256)、`login_with`(281)、`contents_with`(389)、`post_authed`(470)。
2. `rules.rs:152-208` `evaluate` 五个 target_field 匹配块近同构（title/summary/content/author/feed 各自展开同样的 verdict 判定）。

## 建议

- 抽取 `fn map_net_err(e: String) -> <现有 GReader 错误类型>`，一处定义"auth failed:" 前缀分类，四处调用。此函数同时是票 10（错误类型化）的收敛锚点。
- `rules.rs evaluate` 收敛为"按 target_field 取文本 → 统一走单一 match/判定路径"的循环，删掉五份同构块。保持 GLOSSARY 三契约不变：Verdict 唯一入口、Monotonic Hidden、归一化文本比较。

纯重构，行为零变化。

## 验收

- `cargo test` 全绿，greader/rules 既有单测断言无变化。
- `grep -c 'starts_with("auth failed:")' src-tauri/src/greader.rs` ≤ 1（只剩 map_net_err 内部）。
- `rules.rs evaluate` 中不再有按 target_field 展开的重复判定块（行数显著下降）。
