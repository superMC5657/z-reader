# ZReader

现代桌面 RSS 阅读器,参考 [Fluent Reader](https://github.com/yang991178/fluent-reader) 用 **Rust + Tauri 2 + Vue 3** 重构。

## 功能(第一期 MVP)

- 订阅 RSS / Atom / JSON Feed(feed-rs 解析,自动去重、favicon 抓取)
- 分组管理、右键菜单、未读计数
- 四种文章视图:卡片 / 列表 / 杂志 / 紧凑
- 已读 / 未读 / 收藏筛选,后台定时刷新
- 内置阅读视图(sandboxed iframe 渲染,ammonia 消毒)+ Readability 全文抓取(dom_smoothie)
- 正则规则之外的原版核心交互:j / k 导航、m 标记已读、s 收藏、r 刷新
- OPML 导入导出
- Fluent Design 风格 UI,深色模式(跟随系统 / 手动),中英双语
- SQLite 本地存储(rusqlite,WAL),设置存 JSON

## 开发

```bash
npm install
npm run tauri dev     # 开发模式
cargo tauri build     # 出包 (src-tauri/target/release/bundle)
```

## 架构

```
src/          Vue 3 前端(Pinia stores、Fluent 风格组件、vue-i18n)
src-tauri/
  src/db.rs         SQLite schema + 迁移 + 查询层
  src/feed.rs       抓取 + 解析 + 入库(网络与 DB 分离以保持 async Send)
  src/extractor.rs  dom_smoothie 全文提取
  src/opml_io.rs    OPML 导入导出
  src/commands.rs   Tauri command 层(前端唯一入口)
  src/lib.rs        应用入口 + 后台定时刷新任务
```

## Roadmap

- [第二期] 同步服务:Google Reader API、Fever(兼容 FreshRSS / Tiny Tiny RSS 等)
- 正则规则(自动已读 / 收藏 / 隐藏)
- 通知、备份恢复、PAC 代理
