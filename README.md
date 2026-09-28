# ZReader

现代化桌面 RSS 阅读器，基于 **Rust + Tauri 2 + Vue 3** 构建。以本地 SQLite 数据库为唯一核心状态源，支持云端增量同步、全文检索与正则自动化过滤。

## 特性与功能

### 订阅与内容聚合
- **多协议解析**：支持 RSS 0.9/2.0、Atom 与 JSON Feed（基于 `feed-rs`），智能提取元数据、相对路径补全与去重。
- **并发抓取池**：后台与手动刷新采用并发控制任务池（并发度 6），抓取网络 IO 与数据库短事务严格解耦。
- **错误状态可观测**：持久化记录单源拉取失败原因（`last_error`），在订阅列表中直观展示故障状态。
- **OPML 互通**：支持 OPML 格式完整导入与导出，保留多级分组层级结构与 URL 规范化去重。

### 阅读与排版
- **多视图模式**：卡片视图、杂志视图、列表视图快速切换。
- **安全阅读沙箱**：内置沙箱 `iframe` 结合 `ammonia` 净化渲染正文，保障跨站点脚本安全。
- **正文抓取（Readability）**：针对仅输出摘要的订阅源，支持基于 `dom_smoothie` 的自动与手动网页全文提取。
- **交互与快捷键**：支持全局键盘导航（上一篇/下一篇、标已读/收藏/重抓全文等自定义快捷键）。
- **主题与排版定制**：浅色/深色主题动态跟随系统或手动指定，支持界面缩放比例与阅读基准字号自由调节。

### 规则自动化引擎
- **多维模式匹配**：支持针对文章标题、正文、作者、来源 URL 等字段设定正则表达式匹配。
- **自动化动作**：命中后自动执行标为已读、加入收藏、静默隐藏、桌面优先提醒。
- **灵活作用域与回填**：规则可限定全量、单源或特定分组，支持对存量历史文章一键游标分批回填。

### 检索与数据治理
- **FTS5 全文搜索**：内置 SQLite FTS5 倒排索引（`unicode61` 分词器），毫秒级响应，关键词高亮显示。
- **存储生命周期**：支持按保留天数与单源文章上限自动清理未收藏历史文章，大批量删除后自动 `VACUUM` 压缩。
- **流式备份与原子恢复**：打包导出 `.zreader.bak` 完整数据归档（自动剥离敏感密码）；导入前完整性校验，原子替换并在失败时保留原库回滚。

### 云端同步与网络
- **Google Reader API 兼容**：无缝接入 FreshRSS、Miniflux、Bazqux、Inoreader 等兼容服务端。
- **双向同步机制**：推送优先（Push-Queue-First）+ 服务端权威拉取（LWW），离线操作断网入库暂存，联网自动补报。
- **网络代理配置**：支持跟随系统代理、直连或手动配置 HTTP/HTTPS/SOCKS5 代理（含凭据认证），支持针对订阅源的实时连接测试。
- **凭据物理隔离**：敏感凭据保存在独立的 `secrets.json` 中，公开配置保存在 `settings.json`，规避明文备份泄漏风险。

### 桌面原生集成
- **系统托盘**：常驻托盘图标与未读红点徽标，右键菜单支持快速刷新、全读标记与窗口控制。
- **后台常驻与单实例**：支持关闭窗口最小化到托盘，单实例运行保护。
- **桌面原生通知**：后台定时刷新捕获新文章或规则命中优先通知时弹出原生聚合提示。

## 架构

```
src/                      Vue 3 前端工程
  components/feed/        卡片 / 杂志 / 列表视图组件
  components/article/     阅读视图与正文渲染
  components/nav/         侧边栏导航与分组树
  stores/                 Pinia 状态管理 (app / data / ui)
  lib/                    Tauri IPC 封装与辅助工具
src-tauri/                Tauri 2 后端 (Rust)
  src/commands.rs         IPC 命令分发门面
  src/db.rs               SQLite 存储引擎 (WAL、FTS5、索引、查询)
  src/feed.rs             网络抓取、Feed 格式解析与入库
  src/greader.rs          Google Reader 协议客户端
  src/sync.rs             云端双向同步编排引擎
  src/rules.rs            正则自动化规则匹配与回填引擎
  src/net.rs              网络客户端构建与代理管理
  src/settings.rs         配置读写与独立凭据管理 (secrets.json)
  src/backup.rs           数据归档流式备份与原子恢复
  src/extractor.rs        网页全文 Readability 提取
  src/opml_io.rs          OPML 规范化导入与导出
  src/tray.rs             系统托盘与全局事件联动
  src/lib.rs              应用生命周期、并发刷新池与后台任务
```

## 开发与构建

### 依赖环境
- Node.js >= 18, pnpm >= 9
- Rust (Cargo) 稳定版

### 常用命令

```bash
pnpm install       # 安装前端依赖
pnpm tauri dev     # 启动开发模式 (热重载)
pnpm tauri build   # 生产打包出包 (产物位于 src-tauri/target/release/bundle)
```

### 质量检查

```bash
cargo test --manifest-path src-tauri/Cargo.toml          # 后端单元测试
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings # 后端代码检查
pnpm build                                               # 前端类型检查与构建
```
