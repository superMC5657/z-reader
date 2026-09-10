# 06 — clippy -D warnings 门槛

Status: resolved

`cargo clippy --all-targets -- -D warnings` 通过。顺手清掉 4 个预存
（opml struct-update ×3、greader 字面量后缀 ×1）及新代码引入的
（`unused_mut`、`ptr_arg`、测试内 `field_reassign`）。

性能数字（诚实记录）：全单测套件 0.56s 内（含 2500 行回填用例）；
真机刷新/回填/备份前后对比未量，需后续在有数据的机器上补。

## Comments

- 2026-09-10：随 P2 余量批次落地。
