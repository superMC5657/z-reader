# 03 — 恢复后清理 secrets.json sidecar，兑现"恢复后需重填"

Status: ready-for-agent

Blocked by: 02

## 问题

project-health 票 05（secrets-sidecar）写明"备份不带密码、恢复后需重填"，但 `restore_from_archive` 只覆盖 settings.json，从不处理 `secrets.json`。已核实两条静默水合路径：

1. 本机残留 sidecar + 恢复他人/旧账户的 settings.json → `settings::load`（`settings.rs:84-89`）以 sidecar 优先，恢复后的用户名与旧 sidecar 密码错配且静默生效；
2. 旧版（pre-sidecar）备份含明文 inline 密码 + 本机无 sidecar → inline 值原样保留并被水合。

跨机恢复等于旧凭据静默粘贴到新账户，直接违背票 05 声明的边界。

## 建议

- 在 settings 覆盖成功之后（且仅当归档内 settings.json 存在时）：
  1. 删除本机 `secrets.json`（`settings_path.with_file_name("secrets.json")`，与 `settings.rs:11-12` 同源；或重写为空 store，二选一，删除更简单且 `read_secrets` 已容忍缺失）；
  2. 对刚恢复的 settings.json 调用 `scrub_settings_json` 兜底清洗 inline 密码字段（防旧版备份带明文）。
- 与 02 的回滚联动：secrets 清理放在 settings 拷贝成功之后、favicon 阶段之前，此阶段失败沿用 02 建立的回滚路径。
- 前端 `restoreDesc` 文案已声明"需重填"，无需改动；确认 `sync_logout` 清 sidecar 语义不被破坏。

## 验收

- 单测 1：预置本机 secrets.json + 恢复另一账户 settings.json → 恢复后 secrets.json 不存在（或为空 store），`settings::load` 不再水合出密码。
- 单测 2：构造含明文密码的 legacy settings.json fixture（绕过打包期 scrub）→ 恢复后 inline 密码字段为空。
- 票 05 的既有测试（`save_redacts_json_and_load_hydrates` 等）全绿。
