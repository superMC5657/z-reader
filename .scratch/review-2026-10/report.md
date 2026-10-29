# ZReader 全量代码评审报告（2026-10-30）

> 两轴评审（Standards × Spec），由 `/code-review` 流程执行：两个并行子代理分别评审，本报告做核实与汇总，不跨轴重排。

## 评审设置

- **范围**：整个项目。固定点取初始提交 `b9e77ab`，覆盖 69 个提交 + 当前 26 个未提交文件（+2173/-466，均在 src-tauri 与 ci.yml），即整个工作区现状。
- **规模**：Rust 后端 ~11.6k 行（src-tauri/src），前端 ~9k 行（src/），vitest 测试 ~1k 行。
- **Spec 来源**（本仓库无单一整项目规格，取三份最佳可得）：
  1. `docs/PHASE_2_ROADMAP.md`（二期功能规格）
  2. `.scratch/project-health/spec.md` + 6 张票（健康度治理规格，对应近期未提交改动）
  3. `README.md` 特性清单
- **Standards 来源**：`docs/adr/0001..0003`、`docs/logging.md`、`GLOSSARY.md`、`AGENTS.md`。无 CODING_STANDARDS.md / CONTRIBUTING.md。CI 已强制 `cargo fmt` / `cargo clippy -D warnings` / `cargo test` / `vitest run`，工具链覆盖项一律跳过。
- **核实**：报告人已亲自核实全部关键发现（backup.rs 恢复路径、settings.rs sidecar 机制、net.rs 代理回退、scrub 调用点、build_http_client 调用面），引用行号均有效。

---

## 轴一：Standards（规范轴）

### 硬性违规

**S1. ADR-0002 已被代码超越但未更新。**
ADR-0002 记录"凭证存 settings.json（明文）……keyring 加固留作后续演进"，但 `src-tauri/src/settings.rs:93-109` 的 `save()` 现已把 `sync_password/proxy_password` 剥离至独立 `secrets.json`（unix 0600），settings.json 不再含凭据。决策本身是改进，但 ADR-0002 仍标 Accepted 且正文未改，文档与实现失同步。连带效应：`backup.rs:79` `scrub_settings_json` 清洗 settings.json 密码字段已从"主防线"降级为"防御纵深"，其定位变化无文档记录。→ 票 04。

### 坏味道判断（均为 judgement call，仓库文档优先于基线）

**S2. Middle Man**：`lib.rs:254-287` `build_targets`/`should_notify`/`notify_body`/`merge_outcome` 四个纯转发壳（自注"薄委托"）；`services/storage.rs:48` `run_retention_policy_days` 自述"同语义别名"。→ 票 09。

**S3. Duplicated Code**：`greader.rs` 中 `if e.starts_with("auth failed:")` 错误分类闭包在 `read_authed`(256)/`login_with`(281)/`contents_with`(389)/`post_authed`(470) 重复 4 次；`rules.rs:152-208` `evaluate` 五个 target_field 匹配块近同构。→ 票 07。

**S4. Primitive Obsession**：错误全程以 String 传递，靠前缀字符串 `"auth failed:"` 反向再分类（`net.rs:399` 生成、greader 靠前缀识别），`classify_error`(`net.rs:59`) 也靠子串嗅探。→ 票 10（可选）。

**S5. Data Clumps**：`sync.rs:148-159` `with_auth_retry` 8 个伴随参数（已 `too_many_arguments` 放行）；`sync.rs:529` `SourceIdentity=(i64, Option<i64>, String)` 元组别名宜成结构体。→ 票 08。

**S6. Speculative Generality**：`net.rs:178` `RequestContext::proxy()` `#[allow(dead_code)]`；`commands.rs:53` `let _ = scope_id;` 占位参数；`sync.rs:196` `run_with_cycle(_background)`；`rules.rs:123` `len()` dead_code；`sync.rs:539` 单变体 `UnknownStreamDecision::Skip` 枚举仪式化。→ 票 09。

### 合规确认（无需整改）

ADR-0001 modal 固定 px/15px 基准、ADR-0002 协议/引擎分离与推送优先、ADR-0003 日志格式与脱敏红线（唯一 `eprintln!` 为 z_log.rs:310 文档豁免）、GLOSSARY 三契约（Verdict 唯一入口、Monotonic Hidden、归一化文本）均落实良好。

---

## 轴二：Spec（规格轴）

六张 project-health 票（均 Status: resolved）的交付物在代码中基本可验证（票 01-04、06 的验证点均能在代码中对应）。遗留问题：

### 缺失 / 不完整

**P1. 备份恢复不处理 secrets.json，凭据跨账户/跨机静默水合（最重）。**
票 05（project-health）写明"备份不带密码、恢复后需重填"，但 `backup.rs:307-311` 只覆盖 settings.json，从不清理/更新 `secrets.json` sidecar。已核实的两条水合路径：
- 本机残留 sidecar + 恢复他人账户的 settings.json → `settings::load`（settings.rs:84-89）以 sidecar 优先，用户名与旧 sidecar 密码错配且静默生效；
- 旧版（pre-sidecar）备份含明文 inline 密码 + 本机无 sidecar → inline 原样保留并被水合。
跨机恢复等于旧凭据静默粘贴到新账户。→ 票 03。

**P2. 恢复"整体回滚"不完整。**
spec 原文："favicon 与 settings 的覆盖放在 DB 替换成功之后，失败可整体回滚"。`backup.rs:307-325`：DB swap 成功后 settings 拷贝失败直接 `?` 返回 Err——此时 DB 已替换且不回滚，原库快照残留；favicon 阶段 `create_dir_all`(316) 同样可 `?`。→ 票 02。

**P3. `build_http_client` 仍静默回退直连。**
project-health spec 明文"`build_http_client` 不再静默回退直连"。`net.rs:459-484`：manual 模式 URL 为空/解析失败仍 `no_proxy()` 直连，仅靠注释声明"只覆盖手改配置文件"——是有记录的偏离而非实现。3 个生产调用点：`commands.rs:477`（保存后重建）、`commands.rs:557`（连通性测试）、`lib.rs:62`（启动）。→ 票 05。

**P4. 通知点击唤起未实现。**
roadmap 4.1："点击通知可激活并唤起应用主窗口，自动定位到对应文章或未读视图"。全仓仅 `lib.rs:53` 初始化插件，无任何 click/on_action 处理，通知弹出后不可交互。→ 票 06。

**P5.（小项，可接受）** `last_error` 前端展示仅 hover title（`SideNav.vue:270`）；连接测试目标源选择已按 spec 实现（`GeneralTab.vue:45`）。不建票。

### 范围蔓延（已接受，不建票）

- Linux 打包 CI（4cfd4fb/9036778/57ded37：ubuntu deb+AppImage、clippy 修复、Windows 分隔符）——两份 spec 均未要求发行打包，属合理工程演进。
- RequestContext/RequestLog/z_log 与 5+ 个 "deepen" 重构提交——超出 spec 可观测范围，但与 ADR-0003 一致，视为 logging 治理的延续。

### 流程问题

**P6. 工作区漂移扩大。** spec 故事 19 要求"12 个文件有明确归属"，现状是 26 个未提交文件 +2173/-466，漂移比治理前更大。所有后续修改都建立在漂移的工作区上。→ 票 01（先行阻塞票）。

---

## 票映射与状态

票目录：`.scratch/review-2026-10/issues/`。`Blocked by:` 为依赖声明；Status 使用 `docs/agents/triage-labels.md` 五词汇。

| 票 | 主题 | 来源 | Status | Blocked by |
| --- | --- | --- | --- | --- |
| 01 | 工作区 26 文件归属拆分提交 | P6 / spec 故事19 | ready-for-human | — |
| 02 | 恢复 post-swap 回滚完整性 | P2 | ready-for-agent | 01 |
| 03 | 恢复后 secrets.json 清理 | P1 | ready-for-agent | 02 |
| 04 | ADR-0004 secrets sidecar + 0002 标注 | S1 | ready-for-agent | 03 |
| 05 | build_http_client 静默回退诚实化 | P3 | ready-for-agent | 01 |
| 06 | 通知点击唤起主窗口 | P4 | ready-for-agent | 01 |
| 07 | greader 错误分类去重 + rules evaluate 收敛 | S3 | ready-for-agent | 01 |
| 08 | sync 上下文参数结构体化 | S5 | ready-for-agent | 01 |
| 09 | 薄委托壳与 dead code 清理 | S2+S6 | ready-for-agent | 01 |
| 10 | 网络错误类型化（可选） | S4 | needs-triage | 07 |

---

## 执行计划（按依赖排序）

```
阶段 0  止血（阻塞一切）
  01 工作区归属 ──────────────┬──────────────────────────────┐
阶段 1  数据安全（串行链，最高优先）                          │
  02 恢复回滚完整性 ──> 03 恢复凭据清理 ──> 04 ADR 补账        │
阶段 2  行为对齐（与阶段 1 并行，互不依赖）                   │
  05 代理诚实化          06 通知点击唤起                       │（仅依赖 01）
阶段 3  代码还债（与阶段 1/2 并行，内部自由顺序）              │
  07 greader 去重       08 sync 参数结构体    09 薄层清理      │
      │                                                       │
      └──> 10 错误类型化（可选，needs-triage，观察 07 后再议）  │
```

**依赖规则**

- **01 → 全部**：所有代码票都修改当前未提交的文件，必须先落定归属再动工，否则改动无法归因、回滚粒度失控。
- **02 → 03 → 04 严格串行**：02 重构 `restore_from_archive` 的 post-swap 阶段（回滚骨架），03 在该骨架内加入 secrets 清理步骤（同一函数，顺序避免冲突）；04 的 ADR 必须记录 03 落定后的最终行为（含恢复语义），文档跟着代码走。
- **05、06、07、08、09 相互独立**：只依赖 01，可任意并行/穿插。
- **07 → 10**：10 的前提是前缀判断先收敛到一处（07 的 `map_net_err`），故标记 needs-triage，观察 07 后再决定是否立项。

**优先级依据**：阶段 1 是真实的数据安全缺陷（恢复可致凭据错配水合 + 部分失败窗口），收益最高；阶段 2 是 spec 明文承诺的行为（代理诚实化）与 roadmap 承诺（通知点击）；阶段 3 为 judgement call 还债，其中 09 是无风险快速清理，10 影响面最广、收益最低，放最后。

**建议节奏**：01 由维护者拆分提交（半天）→ 02+03 一个会话内完成（同函数）→ 04 顺手补文档 → 05/06/07/08/09 按需并行。
