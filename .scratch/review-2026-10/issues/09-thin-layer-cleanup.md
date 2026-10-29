# 09 — 薄委托壳与 dead code 清理

Status: resolved

Blocked by: 01

## 问题（Middle Man + Speculative Generality，judgement call，快速清理）

Middle Man：
- `lib.rs:254-287` `build_targets` / `should_notify` / `notify_body` / `merge_outcome` 四个纯转发壳（自注"薄委托"）；
- `services/storage.rs:48` `run_retention_policy_days` 自述"同语义别名"。

Speculative Generality：
- `net.rs:178` `RequestContext::proxy()` 带 `#[allow(dead_code)]`，自述无调用方；
- `commands.rs:53` `let _ = scope_id;` 占位参数；
- `sync.rs:196` `run_with_cycle(_background)` 未用参数；
- `rules.rs:123` `len()` dead_code；
- `sync.rs:539` 单变体枚举 `UnknownStreamDecision::Skip` 仪式化。

## 建议

- 删壳直调真实实现（调用点改指向被委托方）。
- dead_code 项确认无近期规划用途后删除；`UnknownStreamDecision` 若是为未来 GReader 流扩展占位，在类型注释写明触发条件（哪个 API 路径会引入第二变体），否则内联删除。
- 每处独立小提交，便于单独回滚。

## 验收

- 删除 `#[allow(dead_code)]` 后 `cargo clippy -D warnings` 全绿（无新告警，证明确无调用方）。
- 行为零变化：`cargo test` 全绿。
