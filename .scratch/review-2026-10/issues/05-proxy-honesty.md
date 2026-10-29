# 05 — build_http_client 消除静默直连回退

Status: resolved

Blocked by: 01

## 问题

project-health spec Implementation Decisions 明文："`build_http_client` 不再静默回退直连"。

`net.rs:459-484`：manual 模式下 URL 为空或 `reqwest::Proxy::all` 解析失败时仍 `no_proxy()` 直连，仅靠文档注释声明"设置保存与连通性测试已前置校验，该分支只覆盖手动修改配置文件的情况"——这是有记录的偏离，不是实现。用户手改配置文件写错代理后，所有请求静默直连，误以为走了代理（spec 故事 13 的原始痛点）。

## 建议

- `build_http_client(settings)` 改为 `build_http_client(settings) -> Result<reqwest::Client, String>`：
  - manual 模式 URL 为空或解析失败 → `Err`（复用 `validate_proxy`（`net.rs:445-453`）的错误文案，"invalid proxy URL: …"）；
  - `"none"` / `"system"` 分支行为不变，恒 Ok。
- 三个调用点各自承接错误，不再静默直连：
  - `commands.rs:557` 连通性测试：Err 直接返回给前端展示；
  - `commands.rs:477` 保存后重建：Err 写日志并经现有失败可见通道（source `last_error` 或刷新失败聚合）暴露；
  - `lib.rs:62` 启动：Err 写 `[APP]` 日志 + 通知聚合文案提示代理配置非法，不构建客户端（后续刷新按源报错）。
- 更新 `net.rs:502-504` 三个单测为 Err 断言。

## 验收

- 单测：manual + 非法 URL → Err；manual + 合法 URL / none / system → Ok 且代理参数正确。
- 手测：手改 settings.json 写入非法 proxy_url 后触发刷新，失败原因在 UI 可见（不再是静默直连）。
- `cargo clippy -D warnings` 全绿。
