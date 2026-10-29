# 04 — 新增 ADR-0004 记录 secrets sidecar 决策，ADR-0002 标注部分被取代

Status: resolved

Blocked by: 03

## 问题

Standards 轴唯一硬性违规：ADR-0002 仍声明"凭证存 settings.json（明文）……keyring 加固留作后续演进"，但 `settings.rs:93-109` 的 `save()` 已把密码剥离至独立 `secrets.json`（unix 0600）。决策是改进，文档失同步。连带：`backup.rs:79` `scrub_settings_json` 从"主防线"降级为"防御纵深"（打包期清洗 + 票 03 的恢复期清洗），其定位变化无记录。

## 建议

- 新增 `docs/adr/0004-secrets-sidecar.md`，记录最终决策：
  - 凭据存储：settings.json 同目录 `secrets.json`，unix 0o600（Windows 无 mode 的降级边界照抄票 05 原文）；
  - `save()` 落盘成功才擦除 inline 明文（失败保留原文，永不丢密码）；`load()` sidecar 优先、无则兼容 inline 并在下次保存迁移；
  - 备份链路：打包期 scrub + 恢复期清 sidecar/洗 inline（引用票 03），恢复后需重填；
  - 演进：网络恢复后可替换 `keyring` crate，`load`/`save` 签名不变。
- ADR-0002 头部加 "Partially superseded by ADR-0004（凭据存储部分）"，正文凭据小节标注 superseded；协议/引擎分离等其余内容保持 Accepted。
- 检查 GLOSSARY.md 与 README.md 无残留"明文凭据"表述。

## 验收

- ADR 编号连续、交叉引用一致（0002 ↔ 0004）。
- `grep -rn "settings.json" docs/` 中关于凭据的表述与新 ADR 一致。
