# ZReader 项目健康度与风险治理 Spec

Status: ready-for-agent

> 单 Agent 全量扫描输出（2026-09-10）。范围：`src-tauri/src/*.rs`（commands / lib / db / feed / sync / greader / rules / backup / net / tray / extractor / opml_io / models / settings）、`src/stores/*.ts`、`src/components/feed/ArticleList.vue`、配置与工作区状态。不含浏览器验证与真机云同步联调。

## Problem Statement

ZReader v0.3.0（第二期）功能已全部交付：代理、托盘、通知、规则引擎、FTS5、备份、存储清理、GReader 同步。但作为长期运行的桌面 RSS 终端，项目存在三类用户可感知的问题：刷新慢且失败原因不可见、云同步与本地规则之间行为不一致、敏感凭据与备份以明文存放；以及三类工程风险：单连接串行 DB 与顺序刷新构成的性能瓶颈、大归档下规则回填与备份的内存/锁风险、12 个文件未提交的工作区漂移与缺失的本地 issue 跟踪（`.scratch/` 不存在）。

本 Spec 把这些问题收敛为可执行、可验证的治理清单，而不是新功能规划。

## Solution

按“先止血、再还债、最后加固”的顺序治理：P0 修复数据安全与同步语义断裂（明文凭据、同步旁路规则、恢复流程原子性）；P1 解决性能与可观测性（并发刷新、DB 锁分离、失败原因落盘展示、分页）；P2 补齐隐私、i18n、OPML 保真度与工程 hygiene。每个治理项都有明确的用户故事、模块归属、验证标准与范围边界。

## User Stories

1. As a 订阅了 50+ 源的用户，I want 所有源并发刷新而不是逐个等待，so that 一次手动刷新在秒级完成而不是被单个慢源拖死。
2. As a 遇到抓取失败的用户，I want 在界面上看到是哪个源失败、HTTP 状态或超时原因，so that 我能自行修复 URL 或调整代理而不是对着“failures: N”发呆。
3. As a 使用 FreshRSS 同步的用户，I want 新文章同样经过我的正则规则（已读/收藏/隐藏/通知），so that 云同步模式与本地模式行为一致。
4. As a 配置了“通知规则”的用户，I want 同步模式下命中规则的文章也能弹通知，so that 重要文章不会因为走了 sync 分支而静默丢失（当前 `notified` 在 sync 分支恒为空）。
5. As a 注重安全的用户，I want 同步密码与代理密码不在 settings.json 明文存放，so that 备份文件与磁盘明文不会泄露我的服务端凭据。
6. As a 换机迁移的用户，I want 备份文件加密或至少对凭据脱敏，so that `.zreader.bak` 丢失不等于所有密码丢失。
7. As a 恢复过备份的用户，I want 恢复是原子的（失败则回滚到原库），so that 中途失败不会让我既丢了原库又没拿到新库（当前先占位内存库再复制，复制失败即丢数据）。
8. As a 有上万篇存档的用户，I want 一键回填规则不卡死界面、不 OOM，so that 我敢用规则治理历史文章（当前全量 load + 逐条 UPDATE + `contains` 判重是 O(n²)）。
9. As a 大数据库用户，I want 备份时不把整个 DB 读进内存，so that 几百 MB 的库也能正常导出（当前 `read(db_file)` 全量进内存）。
10. As a 阅读大量未读的用户，I want 列表支持分页/无限滚动而不是截断到 300 条，so that 我不会漏掉旧未读（当前 `limit: 300` 固定）。
11. As a 快速切换文章的用户，I want 已读标记有失败回滚，so that 离线或 DB 繁忙时界面已读态不会与真实状态 permanently 不一致（当前乐观更新无 rollback）。
12. As a 注重隐私的用户，I want 关闭向 Google/DuckDuckGo 发送订阅域名的 favicon 兜底，so that 我的订阅清单不被第三方收集（当前默认依次请求 s2 与 ddg，无开关）。
13. As a 手动配代理的用户，I want 非法代理 URL 明确报错而不是静默直连，so that 我不会误以为走了代理（当前 `unwrap_or_default` + 解析失败回退直连）。
14. As a 用代理的用户，I want 连接测试测的是我的真实订阅源而不是 example.com，so that 测试通过不代表“假阳性”。
15. As a 中英双语用户，I want 切换语言后托盘菜单也同步更新，so that 托盘不会停留在旧语言（当前菜单文案只在创建时计算）。
16. As a 整理订阅的用户，I want OPML 导入保留多级分组、URL 归一化去重，so that 从 Fluent Reader/Inoreader 迁入时结构不丢失（当前嵌套分组坍缩、仅精确 URL 去重）。
17. As a 搜索的用户，I want 搜索在标题/摘要/正文/作者间表现一致，so that FTS 失败回退到 LIKE 时不会突然搜不到作者（当前回退只查 title/content）。
18. As a 自定义了图标的用户，I want 上传的 favicon 被校验为真实图片而不是任意字节，so that 坏文件或 SVG 载荷不会污染界面或带来 XSS 面（当前仅按 dataURL 字符串猜扩展名）。
19. As a 协作者，I want 工作区 12 个文件的修改有明确归属（提交或拆分），so that 健康度治理不建立在漂移的代码上。
20. As a 后续维护者，I want 每个治理项都有回归测试锚点，so that 修一次、保长期。

## Implementation Decisions

- **DB 并发模型**：保持 rusqlite 单文件，但把“async Mutex<Connection> 全局串行”改为“DB 专属阻塞线程 + 消息队列”或连接池 + `spawn_blocking`。命令层不再全程持有跨 await 的 DB 锁；刷新、回填、VACUUM 等长任务拆分为“短事务 + 让出”。具体选型由实现票决定，但禁止在持有 DB 锁期间做网络 IO（当前 `refresh_all_sources` 以锁划界尚可，但 `fetch_sources` 语义下仍易被长 VACUUM 阻塞）。
- **刷新并发化**：本地刷新从顺序 for 改为带并发上限的任务集（建议 4~8），单源超时保持 30s 且失败只记该源；`mark_source_fetched` 扩展为记录 `last_error` 文本；前端按源展示成功/失败态。云同步模式保持“刷新 = 同步”，但失败计数与本地分支口径对齐。
- **同步语义对齐**：`sync::pull_items` 入库路径复用规则引擎（至少应用 mark_read/star/hide，notify 计入聚合通知）；`lib::refresh_all_sources` 的 sync 分支返回真实 `notified` 而非空向量。`enqueue_item_actions` 对无 remote_id 的条目保持跳过但需打点计数，避免静默丢动作；`mark_all_read` 的 group 回退不再凭空构造 `user/-/label/{name}`，无 remote 映射时不上报流动作。
- **凭据安全**：同步密码、代理密码从 settings.json 明文迁移到系统钥匙串（或至少 OS 凭据存储），settings 仅存用户名与引用；备份默认对 settings 中的凭据段脱敏或整包加密，二选一并在恢复时可逆。过渡期必须先做到“备份不带明文密码”。
- **恢复原子性**：导入备份改为“校验 → 复制到临时库 → 二次打开验证 → 原子替换（rename）→ 重开连接”，任何一步失败都保留原库与原连接；favicon 与 settings 的覆盖放在 DB 替换成功之后，失败可整体回滚。
- **回填与清理的规模化**：回填改为分批游标（按 id 分页），批量 UPDATE（`WHERE id IN`）替代逐条，消除 `to_read.contains` 的 O(n²)；保留清理的 per-source 上限删除改为窗口函数单语句或分批，避免 N+1 DELETE；VACUUM 只在用户显式触发或删除量阈值之上执行，并放在后台任务。
- **备份流式化**：`write_archive` 改为文件流式写入 zip，不再 `read` 整个 DB；增加归档大小与条目数上限提示；`extract_archive` 保持路径穿越防护并追加 symlink 与绝对路径拒绝。
- **前端数据层**：`loadItems` 从固定 300 改为分页（limit + offset + 无限滚动或“加载更多”），搜索保持 250ms 防抖（已在 ArticleList 侧存在，保留）；`setItemRead`/`toggleStar` 增加失败回滚；快速切文章时以最新 `selectedId` 为准丢弃过期 `getItem` 回包。
- **代理诚实化**：非法手动代理 URL 在保存与测试阶段即报错；`build_http_client` 不再静默回退直连；连接测试允许选择目标（默认测用户第一个失败源，回退 example.com 并注明）。
- **隐私开关**：favicon 第三方兜底（Google s2 / DuckDuckGo）默认可关，设置项显式说明外发域名行为；关闭后仅尝试源站 icon。
- **托盘与通知**：托盘菜单文案在语言切换后重建；未读徽标逻辑保留；后台通知沿用聚合文案，但区分“新文章数”与“规则命中”两类。
- **OPML 保真度**：导入做 URL 归一化（trim、默认 https、去尾斜杠/大小写 host）后再去重；保留多级分组（递归建组或路径映射），至少做到二级不丢失；导出保持现有结构不变。
- **搜索一致性**：FTS 回退 LIKE 时覆盖 title/summary/content/author 四字段；FTS 分词保持 unicode61；搜索高亮沿用现有 `highlightText`。
- **favicon 校验**：按魔数校验 png/jpg/ico，拒绝超大与空文件（已有限 5MB，保持），SVG 默认拒绝或经消毒后才接受，具体策略由实现票定。
- **错误可观测**：sources 表新增 `last_error TEXT`（迁移 v5），随 `mark_source_fetched(ok, err)` 更新；`get_stats`/`sync_status` 口径不变，新增 `get_source_errors` 或复用 `get_sources` 携带。
- **Schema 纪律**：`CURRENT_VERSION` 随迁移递增并配套迁移测试；禁止手写与 `migrate()` 不一致的建表语句。
- **工作区止血**：先处理 12 个未提交文件的归属（提交 / 拆分 / 丢弃），再开工 P0；`.scratch/project-health/issues/` 按一题一文件建票（`NN-<slug>.md`，`Status:` 行标记）。

## Testing Decisions

- **什么是好测试**：只测外部行为（命令返回、DB 状态、文件产物、同步语义），不测实现细节（锁类型、并发数、内部函数名）。优先复用现有 `#[cfg(test)]` 锚点：`db::tests`（retention/FTS/upsert/queue）、`rules::tests`、`greader::tests`（fixture 解析）、`backup::tests`（roundtrip/拒绝垃圾）。
- **必须覆盖的模块**：DB 迁移与保留清理、规则引擎（含回填分批）、同步引擎（offline 队列、auth 重试、远端 wins语义）、备份恢复（损坏包、版本过新、缺表）、OPML（嵌套分组、URL 归一化）、代理配置（非法 URL 报错、鉴权透传）。
- **新增测试**：恢复原子性（中途失败原库 intact）、回填 10k 量级性能冒烟（时间/内存上限）、并发刷新（慢源不阻塞快源、失败隔离）、凭据脱敏备份（归档内无明文密码）、分页加载（offset 连续无重无漏）。
- **前端**：现有无单测，保持手动验证 + 类型检查（`vue-tsc --noEmit`）；若引入分页组件，至少加 store 级行为说明与越界用例描述。
- **回归门槛**：`cargo test` 全绿、`cargo clippy -- -D warnings`（若引入）、前端 `build` 通过；性能项给出前后对比数字（刷新耗时、回填耗时、备份内存峰值）。

## Out of Scope

- 真机云同步联调（FreshRSS/Bazqux/Inoreader 实测）仍按路线图待后续，本 Spec 只修语义与可测行为。
- 全文提取质量调优（dom_smoothie 策略）、阅读视图渲染、Fluent UI 重设计。
- 自动更新签名与发布流水线、上报崩溃遥测。
- 多账户同步、多设备冲突可视化合并（超出 LWW 范围的需求）。
- 搜索引擎分词器更换（如 jieba）与跨语言 relevance 调优。

## Further Notes

- 工作区现状：`git status` 显示 12 个已修改未提交文件（commands/db/feed/greader/lib/models/rules/sync/tray + RuleEditDialog/SettingsPanel/types），且 `.scratch/` 尚不存在——本 Spec 落盘即建好 `project-health` 目录，后续实现票拆到 `issues/`。
- 同步分支的 `report.failures` 已在本次工作区改动中被接回 `refresh_all_sources`，属止血向改动，治理时确认其归属后提交。
- 密码明文问题在 `models::SyncAccount`/`Settings` 与 `settings.json`、`export_backup` 打包链路中是同一根因，建议同一票根治，不要拆成三票。
- 若钥匙串方案在某平台不可用，允许降级为“受 OS 文件权限保护的独立凭据文件 + 备份脱敏”，但必须在票内写明降级边界。
- 建议票序：凭据与恢复原子性 → 同步×规则对齐 → 并发刷新与错误可见 → 回填/备份规模化 → 分页与回滚 → 隐私/代理诚实化/托盘/OPML/搜索/favicons。
