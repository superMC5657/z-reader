# Code Review 报告（2026-09-01）

> **当前状态**：✅ **全部问题已修复并通过验证**（修复完成时间：2026-09-01）

---

## 总体结论

本次 Review 发现的 13 项缺陷（4 项 P1、5 项 P2、4 项 P3）已于 2026-09-01 全部完成代码修复并通过自动化测试（`cargo test` 与 `pnpm build`）。

关键修复成果包括：
- 后台定时刷新已正确按配置周期触发；
- “全部文章”全局标记已读修复了参数绑定错误并增加了测试验证；
- Feed 解析器与全文提取器已全面支持最终响应 URL（包含重定向）作为基准 URL；
- 订阅源重命名、已读联动刷新、删除文章选中状态清理等前端交互缺陷已彻底修正；
- 多语言 i18n 缺失文案与发布工作流 SemVer 正则守卫均已补齐完善。

---

## 问题清单与修复记录

### [P1] 后台刷新永远不会启动
- **状态**：✅ **已修复**
- **位置**：`src-tauri/src/lib.rs:73, 81-87`
- **现象**：`last_fetch` 初始化为 `None`，而在 `None` 时直接判定未到期导致死锁。
- **修复方案**：将 `last_fetch` 初始化为 `std::time::Instant::now()`，使得启动后每隔 `interval` 分钟即可正常触发后台刷新。

### [P1] 全部范围标记已读存在参数绑定错误
- **状态**：✅ **已修复**
- **位置**：`src-tauri/src/db.rs:358-376`
- **现象**：`db::mark_all_read` 的 all 分支生成的 SQL 无占位符但仍传入参数导致 `InvalidParameterCount`。
- **修复方案**：重构 `mark_all_read`，在全局 `_` 分支中直接执行无参数 SQL `UPDATE items SET has_been_read=1`，并新增单元测试 `test_mark_all_read` 覆盖全部 scope 分支。

### [P1] Feed 解析缺少基准 URL
- **状态**：✅ **已修复**
- **位置**：`src-tauri/src/feed.rs:27-45`
- **现象**：调用无 base 的 `feed_rs::parser::parse`，相对的 entry link、media image、icon/site URL 会原样入库。
- **修复方案**：使用 `feed_rs::parser::Builder::new().base_uri(Some(&final_url))` 注入最终响应 URL，并新增单元测试 `test_feed_parse_with_base_uri` 验证相对路径转绝对路径逻辑。

### [P2] 全文提取应使用重定向后的最终 URL 作为基准
- **状态**：✅ **已修复**
- **位置**：`src-tauri/src/extractor.rs:14-23`
- **现象**：使用原始 `link` 作为 Readability 的 base，301/302 重定向后相对资源失效。
- **修复方案**：在 `resp.text().await` 消耗响应前提取 `resp.url().to_string()` 并传给 Readability 作为 base_url。

### [P2] 阅读器 iframe 缺少基准 URL
- **状态**：✅ **已修复**
- **位置**：`src/components/article/ArticleView.vue:27-40`
- **现象**：`srcdoc` 中只有 `<base target="_blank">` 而无 `href`。
- **修复方案**：在 iframe `head` 中根据文章 URL 动态注入转义后的 `<base href="${escapeHtmlAttr(item.url)}" target="_blank">`。

### [P1] 订阅源重命名调用了分组重命名命令
- **状态**：✅ **已修复**
- **位置**：`src/components/nav/SideNav.vue:83-133`
- **现象**：源右键菜单调用了 `openGroupModal`，提交时执行了 `rename_group`。
- **修复方案**：增加 `openRenameSourceModal` 与 `modalMode` 状态，源重命名时调用 `api.renameSource(id, name)` 并刷新源列表。

### [P3] Mark all as read 缺少翻译键
- **状态**：✅ **已修复**
- **位置**：`src/i18n/en-US.json`、`src/i18n/zh-CN.json`
- **现象**：使用了 `item.markAllRead` 但字典中只有 `toolbar.markAllRead`。
- **修复方案**：在两个 locale JSON 中补齐 `item.markAllRead` 以及 `feed.rename` 键。

### [P2] 标记源已读后未刷新文章列表
- **状态**：✅ **已修复**
- **位置**：`src/components/nav/SideNav.vue:82`
- **现象**：标记单个源全部已读后只调用 `loadSources()`，未刷新 `data.items`。
- **修复方案**：执行 `api.markAllRead('source', s.id).then(() => Promise.all([data.loadSources(), data.loadItems()]))`。

### [P2] 删除订阅源时未清除已选文章
- **状态**：✅ **已修复**
- **位置**：`src/stores/data.ts:159-166`
- **现象**：在 All 或分组视图中删除当前文章所属源后，`selectedItem` 仍指向已删除记录。
- **修复方案**：在 `removeSource` 中增加判断，若 `selectedItem?.sourceId === id` 则同步重置 `selectedItem = null` 和 `selectedId = null`。

### [P3] 删除当前分组后未重置列表范围
- **状态**：✅ **已修复**
- **位置**：`src/components/nav/SideNav.vue:112-120`
- **现象**：删除分组后未检查当前 `data.scope` 是否指向被删除的分组。
- **修复方案**：删除分组前若 `data.scope.type === 'group' && data.scope.id === gid`，先调用 `await data.selectScope('all')`。

### [P3] 保存设置时未同步应用版本号
- **状态**：✅ **已修复**
- **位置**：`src-tauri/src/settings.rs:13-27`
- **现象**：保存和读取时未将旧版本号同步为当前应用版本。
- **修复方案**：在 `settings::load()` 与 `settings::save()` 时统一覆盖 `settings.version = env!("CARGO_PKG_VERSION").to_string()`。

### [P3] OPML 设置说明未纳入 i18n
- **状态**：✅ **已修复**
- **位置**：`src/components/SettingsPanel.vue:400-420`、`src/i18n/*.json`
- **现象**：OPML 导入与导出描述文本为硬编码中文。
- **修复方案**：在 `settings.data` 中添加 `importOpmlDesc` 与 `exportOpmlDesc`，并在 Vue 模板中通过 `t()` 动态渲染。

### [P2] 发布守卫未强制 SemVer tag
- **状态**：✅ **已修复**
- **位置**：`.github/workflows/release-tauri.yml:43-47`
- **现象**：仅检查 tag 是否以 `v` 开头，非法 tag 会导致产物生成错误。
- **修复方案**：守卫使用严格正则 `^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$` 校验 SemVer tag。

---

## 自动化验证结论

1. **Rust 单元测试**：`cargo test` 全部通过（包含 `db::tests::test_mark_all_read` 与 `feed::tests::test_feed_parse_with_base_uri`）。
2. **前端类型与打包**：`pnpm build`（`vue-tsc --noEmit && vite build`）通过，无任何类型错误或打包异常。


