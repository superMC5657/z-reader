# ADR-0003: 统一用户行为与网络生命周期日志架构

- 状态：Accepted（2026-09）
- 影响范围：`src-tauri/src/net.rs`、`src-tauri/src/feed.rs`、`src-tauri/src/extractor.rs`、`src-tauri/src/greader.rs`、`src-tauri/src/commands.rs`、`src/lib/z-log.ts`、`src/stores/`

## 背景

原有日志设计过于克制且存在两大约束盲区：
1. **用户操作黑盒**：前端完全没有用户交互与操作打点，排查异常时无法获知前置交互路径；
2. **网络资源黑盒**：RSS 抓取、正文提取、Favicon 探测、云同步 API 缺乏统一的网络请求生命周期日志（状态码、延迟、字节量与脱敏 URL），难以定位慢源与网络超时；
3. **分级配置与第三方噪音**：开发模式下 HTML 解析器底层（`html5ever`）语法分析存在海量刷屏日志。

## 决策

1. **统一汇流与日志格式标准化**：
   - 所有前后端记录统一汇流至以应用名称命名的单一文件（`z-reader.log`），消弭多文件对齐成本。
   - 通过 `tauri_plugin_log::Builder::format` 自定义统一输出格式：
     `[YYYY-MM-DD HH:MM:SS] [LEVEL] [TAG] <内容>`
   - 自动收敛消除冗长目标路径：WebView 堆栈帧自动映射为 `[UI]`，后端模块精简为 `[CMD]`、`[NET]`、`[FEED]`、`[SYNC]`、`[APP]`。
2. **高语义可读性与防刷降噪**：
   - 杜绝无语义的裸 ID（如 `id=24 sourceId=2`），全链路补充文章标题、订阅源名称、清晰 URL、文章字符数等可读上下文。
   - 对前端高频切换及文章重载做幂等去重，避免重复记录。
   - 三方库噪声压制：`html5ever`、`selectors`、`markup5ever` 调整至 `Error`，彻底消除 XML 命名空间告警刷屏；底层网络传输库保持 `Warn`。
3. **消除因果时序倒置（Immediate Flush）**：
   - 前端用户交互打点（`zlog.ui`）与异常（`warn`/`error`）采用即时刷盘（`immediate: true`），确保用户行为在日志时间线上严格领先于其触发的后端命令与网络 I/O。
   - 非即时日志刷新间隔由 3000ms 缩短至 200ms。
4. **标准 HTTP 网络生命周期（`RequestLog`）**：
   - 统一输出格式：`GET https://sspai.com/post/114638 -> 200 OK (1265ms, 44.5 KB) [extractor]`。
   - `net::sanitize_url` 自动脱敏 basic auth 与敏感 Query（`token`、`auth`、`key`、`password` 等替换为 `***`）。
5. **隐私与安全底线**：
   - 绝不记录密码、凭据与长文章正文 HTML；
   - 异常信息首行截断，剥离完整敏感路径。

## 后果

- **正向**：日志不仅结构严谨，而且极具自然语言可读性。排查用户问题时，开发者可直接通读完整的用户行为链、后端命令链与网络请求生命周期，定位效率呈数量级提升。
- **性能与存储可控**：前端日志异步非阻塞，后端单行紧凑输出，完全在现有 5MB 轮转与 25MB 总量上限保护下运行。
