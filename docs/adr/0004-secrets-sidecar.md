# ADR-0004: 凭据存储迁移到 secrets.json sidecar（钥匙串降级方案）

- 状态：Accepted（2026-10）
- 关联：ADR-0002（决策 4 凭据存储部分被本 ADR 取代）、`.scratch/project-health/issues/05-secrets-sidecar.md`、`.scratch/review-2026-10/issues/03-restore-secrets.md`

## 背景

ADR-0002 决策 4 曾定为"凭证存 settings.json（明文），keyring 加固留作后续演进"。项目健康度治理（project-health 票 05）把凭据明文列为 P0 风险：settings.json 会以明文驻留磁盘并进入备份链路，泄漏面大。原决议的系统钥匙串需要引入 `keyring` crate，而实施时本机无 crates.io 网络，无法拉取新依赖，按 spec 降级条款执行。

## 决策

1. **凭据物理隔离。** 同步密码与代理密码存放在 settings.json 同目录的独立 `secrets.json`（Unix 0o600）；settings.json 不再含任何凭据。Windows 无 unix mode，at-rest 保护弱于钥匙串——接受，作为过渡。
2. **写入擦除、读取水合。** `settings::save` 落盘 sidecar 成功后才擦除 JSON 明文（失败则保留原文，永不丢密码）；`settings::load` 以 sidecar 优先，无 sidecar 时兼容旧 inline 值，下次保存完成迁移。
3. **备份链路硬承诺"恢复后需重填"。** 打包期 `backup::scrub_settings_json` 清洗 settings.json 的 inline 密码字段（防御纵深，主防线是 sidecar 隔离）；恢复期删除本机 sidecar 并对恢复的 settings 再次 scrub（review-2026-10 票 03）——任何恢复路径都不得把旧凭据静默水合到恢复后的账户上。`sync_logout` 清除 sidecar 中的同步密码。
4. **演进路径。** 网络恢复后可替换为 `keyring` crate，`load`/`save` 签名不变，调用面无感。

## 后果

- 正向：settings.json 与备份归档均无凭据明文，备份丢失不再等于密码丢失；恢复语义与 project-health 票 05 的声明一致。
- 代价：settings + secrets 成为必须配套迁移/清理的状态对，恢复流程为此增加清 sidecar 步骤与现场还原；Windows 上 at-rest 保护弱于钥匙串。
- 拒绝的替代方案：整包加密备份（密钥仍需存放，复杂度不成比例）、系统钥匙串（当时无法拉取依赖，留作演进）。
