# 05 — 凭据迁移到独立 secrets 文件（钥匙串降级方案）

Status: resolved

Spec 原决议是系统钥匙串，但本机无 crates.io 网络，新依赖拉不下来，
按 Spec Further Notes 的降级条款执行：`settings.json` 同目录
`secrets.json`（unix 0o600）存放同步密码与代理密码；`save()` 落盘成功才
擦除 JSON 明文（失败则保留原文，永不丢密码）；`load()` 以 sidecar 优先、
无则兼容旧 inline 值，下次保存完成迁移；`sync_logout` 清 sidecar；
备份链路本来就只打包 `settings.json`，sidecar 永不进包。

降级边界：Windows 无 unix mode，at-rest 保护弱于钥匙串；备份不带密码、
恢复后需重填（`restoreDesc` 文案已同步中英）。网络恢复后可替换为
`keyring` crate 而不改调用面（`load`/`save` 签名不变）。

验证：`save_redacts_json_and_load_hydrates`、
`legacy_inline_passwords_migrate_on_save` 通过。

## Comments

- 2026-09-10：随 P2 余量批次落地；i18n `plaintextWarning` 中英已更新。
