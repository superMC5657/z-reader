# GLOSSARY 术语表（中英对照）

本文档收录双路摄取 verdict 归一引入的新概念名，仅三词条。

- **Verdict 裁决**：规则匹配结果（`Outcome`）经 hide→已读折叠并与服务端已读/加星做 OR 后的最终行状态（`has_been_read / starred / hidden / notify`）。唯一派生点在 `services::article`，调用方只收裁决不再手写折叠。
- **Normalized Text 归一化文本**：Adapter 做 wire→原文翻译后交模块的单路原文。feed 为 content+summary 双源拼接，sync 为 `GItem.content` 合并体，backfill 为 DB 行双源拼接；模块内统一清洗（`html_to_text`）匹配，不再区分 content-only 与 concat。
- **Monotonic Hidden 单调隐藏**：已存在行的 `hidden` 只置 1 不清 0（`hidden = hidden OR verdict.hide`）。`upsert_remote_item` 更新路径生效；`read/star` 保持服务端权威 LWW 覆盖；不提供 hidden-clear 路径。
