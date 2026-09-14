# 统一日志 (logging)

后端 `log::*` 与前端 `zlog` 经 `tauri-plugin-log v2` (Cargo.lock 2.9.0，JS `^2.9.1`)
双文件分流落盘。`log 0.4` 保留：业务代码继续用 `log::warn!` 等宏，
不再悬空——插件在 `run()` 最早注册，后续所有 `log::warn!`（托盘、刷新、
retention、sync 重试等）全部写入 `rust.log`。

## 文件位置（OS 路径）

`z_log::log_dir()` = Tauri `app_log_dir()`：

- Windows: `%APPDATA%\com.zreader.app\logs\`
- macOS: `~/Library/Logs/com.zreader.app/`
- Linux: `~/.local/share/com.zreader.app/logs/`（XDG；以运行时为准）

前端通过 `zlog_get_dir` 命令查询该目录。

## 双文件分流

`z_log::init()` 注册两个 `LogDir` target，按 `target` 是否以
`webview:`（`WEBVIEW_TARGET`）开头过滤（plugin ≥ 2.5 的 `Target::filter`，
当前 2.9.0，真生效，无需前缀降级方案）：

| 文件 | 来源 |
| --- | --- |
| `rust.log` | 后端 `log::*`（target 非 `webview:` 开头） |
| `webview.log` | 前端 `@tauri-apps/plugin-log`（target 为 `webview:*`） |

轮转：单文件 5 MiB、上限保留 6 个历史分片、本地时区。
保留：启动时 `prune_app_dir` 清理——14 天以上删除，仅处理 `*.log`，
之后按 mtime 从旧到新删除直到目录 ≤ 25 MiB（最佳努力，失败吞掉不影响启动）。

## dev / release 级别矩阵

| target | dev (`Debug`) | release (`Info`) |
| --- | --- | --- |
| 全局默认 | `Debug` + `Stdout` | `Info`（无 stdout） |
| `zreader_lib` / `::feed` / `::sync` / `::net` | Info（≥全局） | `Info`：刷新失败可诊断 |
| `hyper` / `reqwest` / `wry` / `tao` / `tungstenite` / `rustls` | Warn | `Warn`：压住传输层噪声 |

`feed/sync/net` 在 release 保持 Info 是有意的：订阅刷新失败必须可见；
三方传输库降 Warn，避免淹没业务日志。

## 前端 batch（`src/lib/z-log.ts`）

- 队列合并：`BATCH_SIZE = 200` 条或 `FLUSH_MS = 3000` ms 触发一次转发，
  `initZLog()` 在 `src/main.ts` 调用一次。
- `pagehide` 补刷：监听器只注册一次（持久监听，非 `once`），
  reload/close 不丢刷新突发日志。
- `attachConsole` 仅 DEV（`import.meta.env.DEV`）。
- 发送 fire-and-forget：日志永远不阻塞 UI。

## 脱敏（`z_log::redact`）

- 掩盖 `password/passwd/pwd/token/secret/api_key/apikey/api-key` 的
  `key = value` / `key: value` 对（大小写不敏感），值延至空白/`"'`,/`;`/`&`
  或配对引号结束；`Bearer <token>` 同理。
- 必须有 `=`/`:` 分隔符才算一对，纯文本（如 "nothing secret here"）原样通过，
  `mytoken=x` 这类前后粘连词不误杀。
- 接线位置：panic hook（见下）。注意 `redact` **不**覆盖 URL userinfo
  （`http://user:pass@host`），因此 `net.rs` 的无效代理 URL 警告
  **不记录 URL 值**，只记事件本身。
- `sync.rs` 的 `GReaderError` 仅含状态码/网络错误串，不含密码原文，可直接记。

## panic hook + stderr 兜底

`install_panic_hook()` 在 `run()` 第一行安装：panic 文本先 `redact` 再
`log::error!("[BE] panic: …")`，同时 `eprintln!` 一份（`panic = "abort"` 的
release 下 hook 仍会执行，stderr 是最终兜底），最后调用前一个 hook。

## 导出（用户显式触发，无自动上报）

- `zlog_get_dir`：返回日志目录绝对路径。
- `zlog_export_bundle`：把 `*.log`（排除此前生成的
  `zreader-log-bundle-*`，防止重复导出层层嵌套）拼成
  `zreader-log-bundle-<epoch>.log` 并返回路径。
- 前端封装：`src/lib/tauri.ts` 的 `zlogGetDir` / `zlogExportBundle`。
- 权限：`src-tauri/capabilities/default.json` 含 `log:default`。
- 本应用**永不自动上传日志**，上报只能是用户显式的"导出并发送"（TODO）。

## 禁止事项

- 日志**禁止**写入业务库：业务数据只在 `zreader.db`（sqlite），
  日志只在 OS 日志目录。`prune`/`export` 只碰 `*.log`，不动其他文件。
- 禁止引入 `tracing` / sentry / 任何自动上报；禁止改 release profile、
  应用 identifier 与业务 DB。

## 旧 `log::warn!` 现落盘说明

此前唯一的悬空点是散落在各处的 `log::warn!`（`lib.rs` 托盘/刷新/retention、
`commands.rs` 清理/恢复、`db.rs` 同步队列、`net.rs` 代理、`sync.rs` 重试），
没有初始化任何 logger，release 下直接丢弃。现在 `z_log::init()` 作为首个
plugin 注册 + `prune_app_dir` 在 `setup` 内执行，这些 `warn!` 全部落盘到
`rust.log`（release Info 级别下可见），刷新失败类问题可从日志目录复现。
