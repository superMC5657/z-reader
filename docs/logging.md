# 统一日志架构 (logging)

后端 `log::*` 与前端 `zlog` 经 `tauri-plugin-log v2` (Cargo.lock 2.9.0，JS `^2.9.1`) 统一写入以应用名称命名的单一日志文件（`z-reader.log`）。

## 文件位置（OS 路径）

`z_log::log_dir()` = Tauri `app_log_dir()`：

- Windows: `%APPDATA%\com.zreader.app\logs\`
- macOS: `~/Library/Logs/com.zreader.app/`
- Linux: `~/.local/share/com.zreader.app/logs/`（XDG；以运行时为准）

前端通过 `zlog_get_dir` 命令查询该目录。

## 单一应用日志文件 (`z-reader.log`)

所有日志汇流至同一文件，彻底消除多文件时标对齐与因果错位成本：

| 文件 | 来源 | 记录内容 |
| --- | --- | --- |
| `z-reader.log` | 前端 `zlog` + 后端 `log::*` | 用户行为 `[UI]`、命令审计 `[CMD]`、网络请求 `[NET]`、订阅更新 `[FEED]`、云同步 `[SYNC]`、系统 `[APP]` |

轮转与保留：
- 轮转：单文件 5 MiB、上限保留 6 个历史分片、本地时区。
- 保留：启动时 `prune_app_dir` 清理——14 天以上删除，仅处理 `*.log`，之后按 mtime 从旧到新删除直到目录 ≤ 25 MiB（最佳努力，失败吞掉不影响启动）。

---

## 日志格式规范与清晰可读性

所有日志统一经由 `z_log::Builder::format` 定制输出：
`[YYYY-MM-DD HH:MM:SS] [LEVEL] [TAG] <可读语义内容>`

### 模块标签 (TAG)
- `[UI]`: 前端用户真实交互（点击文章、切换订阅源/文件夹、标记已读、搜索、更改设置等）。
  - **因果一致性**：用户操作通过 `immediate: true` 立即刷盘，不再因 3000ms 批量延迟而落后于后端命令。
  - **高频防刷与去重**：重复点击或文章重载时去重，避免重复记录；提供文章标题与订阅源名称，告别无语义的裸 ID (`id=24 sourceId=2`)。
- `[CMD]`: 后端 Tauri 命令入口与审计，展示清晰业务意图与执行结果（如文章标题、链接、解析字符数）。
- `[NET]`: 网络请求与响应生命周期，采用标准 HTTP 格式：
  - `GET https://sspai.com/post/114638 -> 200 OK (1265ms, 44.5 KB) [extractor]`
  - `GET https://example.com/feed -> ERR: connection timed out (10000ms) [feed]`
- `[FEED]`: 订阅源刷新生命周期，标明源名称与失败原因：
  - `Refreshing 2 feed(s)...`
  - `Feed "少数派" refreshed: 5 new article(s)`
  - `Feed "某博客" (https://...) failed: 404 Not Found`
  - `Feed refresh completed: 5 new article(s), 1 failed (1054ms)`
- `[SYNC]`: 云端同步引擎步骤（登录、拉取、推送队列、对账）。
- `[APP]`: 应用核心生命周期与系统级事件。

---

## 示例日志流对照

```log
[2026-09-16 22:02:32] [ INFO] [UI] Select article "少数派年度征文：从效率工具到生活方式" [Feed: "少数派", id=24]
[2026-09-16 22:02:35] [ INFO] [CMD] Fetch full content for "少数派年度征文：从效率工具到生活方式" (https://sspai.com/post/114638)
[2026-09-16 22:02:35] [DEBUG] [NET] GET https://sspai.com/post/114638 [extractor]
[2026-09-16 22:02:37] [ INFO] [NET] GET https://sspai.com/post/114638 -> 200 OK (1265ms, 44.5 KB) [extractor]
[2026-09-16 22:02:37] [ INFO] [CMD] Extracted full content for "少数派年度征文：从效率工具到生活方式" (4520 chars)
[2026-09-16 22:02:37] [ INFO] [CMD] Mark article "少数派年度征文：从效率工具到生活方式" as read
[2026-09-16 22:02:59] [ INFO] [UI] Manual refresh triggered (2 feeds)
[2026-09-16 22:02:59] [ INFO] [FEED] Refreshing 2 feed(s)...
[2026-09-16 22:02:59] [DEBUG] [NET] GET https://feeds.appinn.com/appinns/ [feed]
[2026-09-16 22:02:59] [DEBUG] [NET] GET https://sspai.com/feed [feed]
[2026-09-16 22:03:00] [ INFO] [NET] GET https://feeds.appinn.com/appinns/ -> 200 OK (960ms, 110.4 KB) [feed]
[2026-09-16 22:03:00] [ INFO] [NET] GET https://sspai.com/feed -> 200 OK (1018ms, 132.3 KB) [feed]
[2026-09-16 22:03:00] [ INFO] [FEED] Feed refresh completed: 0 new article(s), 0 failed (1054ms)
```

---

## 噪音抑制与三方库过滤

- `html5ever` / `selectors` / `markup5ever`：过滤级别设为 `Error`，彻底消除 HTML 解析时对 XML 命名空间警告的刷屏（如 `node with weird namespace Atom('')`）。
- `hyper` / `reqwest` / `wry` / `tao` / `tungstenite` / `rustls`：过滤级别设为 `Warn`，消除底层传输噪音。

---

## 隐私脱敏底线（红线）

- 绝不记录密码、凭据与 Token：Auth token、GoogleLogin 密码、代理密码等只留存内存，日志中绝对脱敏（由 `z_log::redact` 与 `net::sanitize_url` 双重保护）。
- 绝不记录长文章全文正文：文章内容仅在提取时记录字数，HTML 正文不进入日志。
- 完整 URL 敏感 Query 剔除：网络请求日志中的 `token`, `auth`, `key`, `secret`, `password` 等均替换为 `***`。
- 异常信息首行截断：最多 160 字符，不跨行，不泄漏本地敏感文件路径。
- 禁止 `println!/eprint!/dbg!`（`z_log` panic hook 的 `eprintln!` 是 stderr 兜底唯一例外）。
