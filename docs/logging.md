# 统一日志 (logging)

后端 `log::*` 与前端 `zlog` 经 `tauri-plugin-log v2` (Cargo.lock 2.9.0，JS `^2.9.1`)
双文件分流落盘。业务代码用 `log::info!/warn!/debug!` 直接打点，
插件在 `run()` 最早注册。

## 文件位置（OS 路径）

`z_log::log_dir()` = Tauri `app_log_dir()`：

- Windows: `%APPDATA%\com.zreader.app\logs\`
- macOS: `~/Library/Logs/com.zreader.app/`
- Linux: `~/.local/share/com.zreader.app/logs/`（XDG；以运行时为准）

前端通过 `zlog_get_dir` 命令查询该目录。

## 双文件分流

`z_log::init()` 注册两个 `LogDir` target，按 `target` 是否以
`webview:`（`WEBVIEW_TARGET`）开头过滤：

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
| `zreader_lib` / `::feed` / `::sync` / `::net` | Debug（含下表第二层） | `Info`：只见下表第一层 |
| `hyper` / `reqwest` / `wry` / `tao` / `tungstenite` / `rustls` | Warn | `Warn`：压住传输层噪声 |

## 第一层：release 可见（info / warn）

一轮刷新只一行 `info`（本地模式仅 R1；同步模式 R1 + R6 共两行）；失败走 `warn`。消息英文小写前缀模块词。

| # | 文件行号 | 级别 | 消息样例 |
| --- | --- | --- | --- |
| R1 | `lib.rs:397` | info | `refresh done new 12 failures 1 sync false`（复用 `total_new`/`failures`/`is_sync`） |
| R2 | `lib.rs:219` | warn | `source 7 failed host example.com reason fetch failed: …`（store 失败分支） |
| R3 | `lib.rs:244` | warn | `source 7 failed host example.com reason HTTP 500`（fetch 失败分支） |
| R4 | `sync.rs:90` | info | `sync login ok provider greader`（只记 provider 名；仅新登录，缓存命中不重复） |
| R4b | `sync.rs:85` | warn | `sync login failed reason …`（登录失败唯一出口，调用方直接透传） |
| R5 | `commands.rs:518` | info | `sync logout ok provider greader`（logout 只记此处，与 R4 无双记） |
| R6 | `sync.rs:188` | info | `sync pull done new 5 failures 0 notified 1`（整轮结束计数） |
| R6b | `sync.rs:126,133,139` | warn | `sync push / sync subscriptions failed reason …`（`retry_auth!` 宏三失败分支，op 名在调用处展开） |
| R7 | `sync.rs:173,179,184` | warn | `sync pull failed reason …`（三处收敛：重试耗尽/重登失败/直失败；内联展开与宏语义等价：三分支+计数各一次） |
| R8 | `commands.rs:459` | info | `proxy test ok latency 320ms`（不记 URL 值） |
| R9 | `commands.rs:430,449,455` | warn | `proxy test failed reason …`（校验/连接/状态三处，不记 URL 值） |
| R10 | `commands.rs:386` | info | `opml import ok groups 2 sources 9 existing 1`（只记数量） |
| R11 | `commands.rs:399` | warn | `opml import failed reason …`（原因首行） |
| R12 | `commands.rs:411` | info | `opml export ok bytes 4821`（只记字节数） |
| R13 | `commands.rs:415` | warn | `opml export failed reason …` |
| R14 | `commands.rs:660` | info | `backup export ok file zreader-backup-20260101-120000.zreader.bak`（basename） |
| R15 | `commands.rs:665` | warn | `backup export failed reason …` |
| R16 | `commands.rs:714` | info | `backup import ok file zreader-backup-20260101-120000.zreader.bak`（basename） |
| R17 | `commands.rs:719` | warn | `backup import failed reason …` |
| R18 | `commands.rs:636,648` | warn | `rules backfill failed reason …`（引擎加载/回填两处） |
| R19 | `commands.rs:332` | warn | `content fetch failed item 42 reason …`（item id + 原因首行，不记 URL/正文） |
| R20 | `commands.rs:846` | info | `store vacuum ok` |
| R21 | `commands.rs:108,116,121` | warn | `add source failed host example.com reason …`（fetch/insert/store 三处，只记 host + 原因首行，不记完整 URL） |
| R22 | `commands.rs:492` | warn | `sync login subscriptions failed reason …`（订阅拉取失败，原因首行，不记正文/token） |
| R23 | `lib.rs:300` | warn | `refresh rules load failed reason …`（规则引擎加载失败，仍 `?` 透传整轮中止） |
| Z1 | `z_log.rs:91` | warn | `zlog: cannot resolve log dir: …`（启动期 prune 拿不到日志目录，吞掉不影响启动） |

## 第二层：debug（release 不可见）

单源成功只许 `debug`，禁止 info 化；轮开始/挂起/清理细节同理。

| # | 文件行号 | 级别 | 消息样例 |
| --- | --- | --- | --- |
| D1 | `lib.rs:290` | debug | `refresh start mode background sync true`（同步轮，无源数量） |
| D2 | `lib.rs:320` | debug | `refresh start mode manual sources 18 sync false`（本地轮，手动/后台+源数量） |
| D3 | `lib.rs:239` | debug | `source 7 ok host example.com new 3`（单源成功唯一出口，含 new 计数） |
| D4 | `lib.rs:233` | debug | `source 7 favicon miss host example.com` |
| D5 | `lib.rs:374` | debug | `store retention done deleted 12` |
| D6 | `lib.rs:359` | debug | `refresh task join failed`（任务挂起/崩溃收敛计数处） |
| D7 | `commands.rs:864` | debug | `store cleanup done deleted 12` |

## 第三层：禁止记（红线）

- 60s tick / 心跳：`background_refresh` 的 sleep/跳过分支零打点。
- 标题/正文/摘要/服务端原文：永不进日志（`notified` 标题只进通知体，不进日志）。
- 完整 URL / query / userinfo：只许 `host`（`url` crate 取 `host_str`，失败记 `unknown`；reqwest 错误回显由 `http_err_reason` 降为 host，见下）。
- token / 密码 / Auth：只活内存（`sync::Session` 不落盘、不记日志）；`GReaderError` 本就不含密码原文。
- 原因一律首行（`.lines().next()`），超长截断 160 字符（`chars` 计数，字符边界安全）；路径只许 `basename`。
- 禁止 `println!/eprint!/dbg!`（`z_log` panic hook 的 `eprintln!` 是 stderr 兜底，
  唯一例外）；禁止改 `z_log.rs` / 分级 / profile / identifier / DB 结构。

## 前端 batch（`src/lib/z-log.ts`）

- 队列合并：`BATCH_SIZE = 200` 条或 `FLUSH_MS = 3000` ms 触发一次转发，
  `initZLog()` 在 `src/main.ts` 调用一次。
- `pagehide` 补刷：监听器只注册一次（持久监听，非 `once`），
  reload/close 不丢刷新突发日志。
- `attachConsole` 仅 DEV（`import.meta.env.DEV`）。
- 发送 fire-and-forget：日志永远不阻塞 UI。

## 脱敏（`z_log::redact` + `net::http_err_reason`）

- `redact`：掩盖 `password/passwd/pwd/token/secret/api_key/apikey/api-key` 的
  `key = value` / `key: value` 对（大小写不敏感），值延至空白/`"'`,/`;`/`&`
  或配对引号结束；`Bearer <token>` 同理。必须有 `=`/`:` 分隔符才算一对，
  纯文本原样通过，`mytoken=x` 这类前后粘连词不误杀。接线位置：panic hook。
- `redact` 不覆盖 URL userinfo，因此业务日志一律只记 `host`。
- `http_err_reason`（`net.rs`）：reqwest `Error` 的 Display 会追加
  `for url (<完整>)`，所有网络错误（feed / extractor / greader / 代理测试）
  先经此函数把回显 URL 降为 host 再返回；调用方仍按惯例包 `short_reason`。

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
  源表 DB 已持久化每次抓取结果（`fetched_at`/`error` 是单源真相源），
  日志不复制 DB 状态。
- 禁止引入 `tracing` / sentry / 任何自动上报；禁止改 release profile、
  应用 identifier 与业务 DB。
