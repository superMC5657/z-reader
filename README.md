# ZReader

现代桌面 RSS 阅读器,参考 [Fluent Reader](https://github.com/yang991178/fluent-reader) 用 **Rust + Tauri 2 + Vue 3** 重构。

## 功能(第一期 MVP)

- 订阅 RSS / Atom / JSON Feed(feed-rs 解析,自动去重、favicon 抓取)
- 分组管理、右键菜单、未读计数
- 三种文章视图:卡片 / 杂志 / 列表
- 已读 / 未读 / 收藏筛选,后台定时刷新
- 内置阅读视图(sandboxed iframe 渲染,ammonia 消毒)+ Readability 全文抓取(dom_smoothie)
- 正则规则之外的原版核心交互:← / → 导航、m 标记已读、s 收藏、r 刷新
- OPML 导入导出
- Fluent Design 风格 UI,深色模式(跟随系统 / 手动),中英双语
- SQLite 本地存储(rusqlite,WAL),设置存 JSON

## 功能(v0.3.0 第二期)

- **网络代理**:HTTP/HTTPS/SOCKS5,跟随系统或手动配置(含鉴权),支持连接测试,保存后热生效
- **系统托盘**:常驻托盘 + 未读红点徽标,右键菜单(立即刷新 / 全部已读 / 显示 / 退出),关闭窗口最小化到托盘,单实例
- **原生通知**:后台刷新到新文章时弹聚合通知;命中"通知"规则的文章强制提醒
- **正则规则引擎**:对新文章按标题/正文/作者/链接自动执行 已读 / 收藏 / 隐藏 / 通知,支持限定订阅源或分组、一键回填存量文章、隐藏文章审查
- **FTS5 全文搜索**:毫秒级倒排检索,搜索输入防抖,标题与摘要关键词高亮
- **完整备份恢复**:.zreader.bak 打包数据库 + 设置 + 图标,恢复前完整性校验
- **存储生命周期**:按保留天数 / 每源上限自动清理未收藏旧文章,大批量删除后自动 VACUUM
- **云端同步**:接入 Google Reader 兼容服务端(FreshRSS / Bazqux / Inoreader 等),订阅导入 + 文章与已读/收藏双向同步,离线操作队列补报,断开后本地数据保留

## 开发

```bash
pnpm install
pnpm tauri dev     # 开发模式
pnpm tauri build   # 出包 (src-tauri/target/release/bundle)
```

## 架构

```
src/          Vue 3 前端(Pinia stores、Fluent 风格组件、vue-i18n)
src-tauri/
  src/db.rs         SQLite schema + 迁移 + 查询层(含 FTS5 触发器、保留清理、同步队列)
  src/feed.rs       抓取 + 解析 + 入库(网络与 DB 分离以保持 async Send)
  src/greader.rs    Google Reader API 客户端(请求/解析分离,可 fixture 测试)
  src/sync.rs       云同步引擎(推队列 → 订阅对账 → 增量拉取)
  src/rules.rs      正则规则引擎(编译、作用域过滤、回填)
  src/net.rs        HTTP 客户端构建(代理配置)
  src/tray.rs       系统托盘(菜单、红点徽标、主窗口唤起)
  src/backup.rs     备份快照 / 压缩包 / 校验
  src/extractor.rs  dom_smoothie 全文提取
  src/opml_io.rs    OPML 导入导出
  src/commands.rs   Tauri command 层(前端唯一入口)
  src/lib.rs        应用入口 + 统一刷新流程 + 后台定时任务
```

## Roadmap

第二期规划详见 [docs/PHASE_2_ROADMAP.md](docs/PHASE_2_ROADMAP.md)。

**v0.3.0 第二期全部交付**:网络代理、系统托盘、原生通知、正则规则引擎、FTS5 搜索、完整备份、存储清理、Google Reader 云同步(设计决策见 docs/adr/0002)。

Fever API 兼容与云同步真机联调待后续。
