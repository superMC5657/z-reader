# 10 — 网络错误类型化，替代字符串前缀往返（可选）

Status: resolved

Blocked by: 07

## 问题（Primitive Obsession，judgement call）

错误全程以 String 传递，靠前缀字符串 `"auth failed:"` 反向再分类：`net.rs:399` 生成前缀、greader 侧靠 `starts_with` 识别（07 落地后收敛到 `map_net_err` 一处）；`net.rs:59` `classify_error` 也靠子串嗅探 error_kind。前缀是隐式协议，改名/改文案即静默破坏分类。

## 建议

- 在 net 边界定义错误枚举（如 `NetError { Auth, Timeout, Dns, Status(u16), Other(String) }`），实现 `Display` 保留现有用户可见文案，内部传递用枚举。
- 跨 FFI 边界（tauri command 返回）保持序列化为字符串，前端无感。
- 07 的 `map_net_err` 是天然落点：枚举在其内部构造，外层只 match 枚举。

## 排期建议

影响面广（feed / sync / greader 全链路签名调整），收益是消除隐式前缀协议。建议：先落地 07 观察分类是否再出错，若稳定则本票维持 needs-triage 择期立项；若 07 后仍出现前缀耦合扩散（新调用点再次 starts_with），升级为 ready-for-agent。

## 验收（若立项）

- `grep -rn 'starts_with("auth failed:")' src-tauri/src` 仅剩枚举构造/转换一处。
- 错误文案面向用户不变（i18n 无 diff）。

## Comments

- 2026-10-30：维护者拍板立项（原 needs-triage），排期在 07 之后；Blocked by: 07 关系不变。
