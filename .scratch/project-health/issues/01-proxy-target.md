# 01 — 连接测试支持真实目标

Status: resolved

`test_proxy(settings)` 只测 `example.com`，假阳性。改为
`test_proxy(settings, target: Option<String>)`，前端优先传入首个
`errorCount > 0` 订阅源 URL，无失败源才回退默认探测地址；结果文案注明
实际测的目标（`proxyOk` 新增 `{target}`，`proxyTargetDefault` 中英）。

验证：`vue-tsc --noEmit` 通过；后端行为沿用既有网络路径，无新增单测
（需真实网络，不适合单元测试）。

## Comments

- 2026-09-10：随 P2 余量批次落地；与“代理诚实化”决议的最后一项对齐。
