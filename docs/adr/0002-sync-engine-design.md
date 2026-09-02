# ADR-0002: 云同步引擎设计（推送优先队列 + 服务端权威拉取 + LWW）

- 状态：Accepted（2026-09）
- 关联：docs/PHASE_2_ROADMAP.md Phase 2.3

## 背景

ZReader 要接入 Google Reader 兼容云服务（FreshRSS、Bazqux、Inoreader、TT-RSS 插件等），解决多端阅读进度一致问题。需要决定：本地数据与服务端的关系、冲突如何解决、离线操作如何处理。

## 决策

1. **本地 SQLite 仍是 UI 的唯一数据源。** 服务端订阅映射到现有 `sources/groups` 表（新增 `remote_id` 列），文章进 `items`（新增 `remote_id`）。前端完全不知道"同步"的存在，复用全部界面与交互；刷新按钮、后台定时器在账号连接时自动路由到同步引擎（`refresh_all_sources` 顶部单一入口分流）。
2. **推送优先。** 每轮同步先清空本地操作队列（`sync_queue` 表：mark_read / mark_unread / star / unstar 逐条目，mark_all_read 按流），再拉取远端变化。这样后续拉到的文章状态已包含本地刚做的操作，避免"自己覆盖自己"。
3. **服务端权威（LWW）。** 拉取用 `stream/items/ids?s=reading-list&ot={游标}` 增量分页，`contents` 中的 read/starred 状态标签直接覆盖本地对应行。首个同步窗口回看 180 天，之后以 `sync_state` 中的上次同步时间为游标。远端不存在的本地源不删除。
4. **凭证存 settings.json（明文），会话只存内存。** Auth token 缓存在 `AppState.sync_token`（Rust std RwLock，从不落盘），启动重新登录，401 触发一次重登重试。与 Fluent Reader 行为一致；明文风险在设置页展示提示，keyring 加固留作后续演进。
5. **协议层与引擎分离。** `greader.rs` 只做请求构造与响应解析（纯函数可测），`sync.rs` 编排"推队列 → 订阅对账 → 增量拉取"。`url::form_urlencoded::Serializer` 持有非 Send 闭包且带 Drop 守卫，必须用作用域块约束在其 `finish()` 之后、await 之前销毁（编译器强制，见 greader.rs 注释）。

## 后果

- 正向：UI 零改动接入同步；离线操作不丢失；协议解析可对 fixture 做单元测试；支持任意 GReader 兼容服务端（自填 API 基址）。
- 代价：远端把文章标记为已读而本地此前未变动时，依赖下一轮 ot 窗口收敛，极端情况下（超过 180 天回看窗、条目被服务端清除）会漂移——接受。
- 拒绝的替代方案：双写合并（复杂度不成比例）、完整双向逐字段 diff（服务端不提供变更日志）、Fever 协议（无订阅管理 API，受众萎缩，留待后续）。
