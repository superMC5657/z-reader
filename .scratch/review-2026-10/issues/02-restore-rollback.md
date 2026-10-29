# 02 — 备份恢复 post-swap 阶段回滚完整性

Status: ready-for-agent

Blocked by: 01

## 问题

project-health spec Implementation Decisions："favicon 与 settings 的覆盖放在 DB 替换成功之后，失败可整体回滚"。

`src-tauri/src/backup.rs:307-325`：DB swap 成功后，settings 拷贝（`backup.rs:310`）与 favicon `create_dir_all`（`backup.rs:316`）失败时直接 `?` 返回 Err——此时 DB 已替换且不回滚，原库快照残留，恢复处于"半完成"状态且用户看到失败却拿到新库。

## 建议

- 把 post-swap 阶段（settings + favicon）包进内部闭包/Result：
  - settings 拷贝失败 → 复用已有的 `rollback_live(&rollback, db_path, &staged, &tmp_dir, &mut guard, e)`（快照在 `backup.rs:289-293` 已创建）回滚到恢复前状态并返回原 Err，保证 spec 的"整体回滚"承诺。
  - favicon `create_dir_all` 失败降级为 best-effort + 日志（单文件拷贝已是 `let _ =`，保持）——favicon 是普通资源，不值得触发全量回滚；在返回值/日志中注明 favicon 未完全恢复。
- 成功/失败路径的 staged/tmp 清理逻辑保持不变。

## 验收

- 新增单测：settings 目标路径不可写（如指向目录）时 restore 返回 Err，且 DB 内容与恢复前一致（快照回滚生效）。
- 新增单测：favicon 目录创建失败不影响整体成功，日志有记录。
- 既有 `restore_full_order_tmp_fixture` 全绿。

## Comments

- 2026-10-30：维护者拍板采用本票建议语义：settings 拷贝失败 → 整体回滚到快照；favicon 失败 → 尽力而为 + 日志/结果提示，不触发回滚。
