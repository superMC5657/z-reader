# 06 — 通知点击唤起主窗口（roadmap 4.1）

Status: resolved

Blocked by: 01

## 问题

`docs/PHASE_2_ROADMAP.md` 4.1 承诺："点击通知可激活并唤起应用主窗口，自动定位到对应文章或未读视图"。

当前全仓仅 `lib.rs:53` 初始化 `tauri-plugin-notification`，无任何 click/on_action 处理，通知弹出后不可交互。

## 建议（最小实现优先）

1. 注册通知点击回调 → show + set_focus 主窗口，并把主视图切到未读视图（复用现有 store 的视图切换机制）。
2. "定位到具体文章"需要通知载荷携带 feed/article id，而当前通知是聚合文案（不含 id）——作为可选增强单独迭代，本票只交付"唤起窗口 + 未读视图"。
3. 平台边界：若 `tauri-plugin-notification` v2 在 Windows 的 on_action/click 事件支持受限，用托盘图标点击作为等效兜底路径，并把边界记录到本票 Comments；实在不可行则降级 Status 为 ready-for-human 交维护者决策（改用自绘托盘气泡通知等方案）。

## 验收

- Windows 手动验证：触发通知 → 点击 → 主窗口前置并获得焦点，视图为未读列表；`[APP]` 日志记录点击事件。
- 回归：现有通知聚合逻辑（新文章数 vs 规则命中两类文案）不变。

## Comments

- 2026-10-30：维护者拍板最小实现：点击通知 → 前置主窗口 + 切未读视图；"定位到具体文章"延后为独立增强（需通知载荷携带 feed/article id）。
- 2026-10-30：落地边界：tauri-plugin-notification 桌面端不暴露任何点击回调 API（已核实 2.4.0 源码），Windows 安装版绕行 tauri-winrt-notification 注入 on_activated；开发目录与非 Windows 平台回落插件（无点击能力）。代码级验证（clippy/92 后端测试/40 前端测试）通过；toast 实际点击需安装版手测。
