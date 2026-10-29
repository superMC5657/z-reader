# 08 — sync 上下文参数结构体化

Status: ready-for-agent

Blocked by: 01

## 问题（Data Clumps，judgement call）

1. `sync.rs:148-159` `with_auth_retry` 携带 8 个伴随参数（state/http/acct/auth/cycle/host/report/op），靠 `#[allow(clippy::too_many_arguments)]` 放行。
2. `sync.rs:529` `SourceIdentity = (i64, Option<i64>, String)` 元组别名，调用点靠位置语义读字段。

## 建议

- 把 8 个伴随参数收进一个上下文结构体（建议名 `SyncCtx`，字段沿用现名），`with_auth_retry(ctx, f)`；构造点集中在现有调用处，逻辑不变。
- `SourceIdentity` 升格为具名字段结构体（如 `source_id: i64, remote_id: Option<i64>, title: String`），元组别名删除。该类型已具备领域含义（"本地源 ↔ 远端流"映射），符合 GLOSSARY 术语化方向。

机械重构，行为零变化。

## 验收

- `cargo test` + `cargo clippy -D warnings` 全绿；`too_many_arguments` 的 allow 可移除。
- `SourceIdentity` 不再是元组：`grep -n "type SourceIdentity" src-tauri/src/sync.rs` 无结果。
