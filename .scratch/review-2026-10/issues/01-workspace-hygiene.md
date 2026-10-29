# 01 — 工作区 26 个未提交文件的归属拆分提交

Status: ready-for-agent

Blocked by: —

## 问题

project-health spec 故事 19 要求"工作区修改有明确归属（提交或拆分）"，治理后漂移反而从 12 个文件扩大到 26 个（+2173/-466，src-tauri 全模块 + ci.yml）。所有后续修改都建立在漂移的工作区上，改动无法归因、回滚粒度失控。本票是本 effort 全部代码票的前置。

## 建议

按主题拆分提交（对照 project-health 票 01-06 与近期 logging/重构提交脉络），建议分组：

1. secrets sidecar：`settings.rs`、`backup.rs`（scrub 相关）、相关 i18n
2. RequestContext / RequestLog / z_log：`net.rs`、`z_log.rs`、`utils.rs`
3. unit seams 与测试：`db/*`、`services/*` 中纯测试缝改动
4. clippy/CI 修复：`ci.yml` 与纯 lint 修复 hunk
5. 其余按模块收拢

同一 hunk 跨主题时以 `git add -p` 拆分；无法归因的改动显式丢弃而不是混入。

## 验收

- `git status` 干净（除本 effort 新增文件）。
- 每个提交主题单一，提交信息可回溯到来源票。

## Comments

- 2026-10-30：维护者拍板：放弃按主题拆分，26 个文件合并为单个批量提交，归因以本票为准；本 effort 全部票（02-10）已批准开工。
