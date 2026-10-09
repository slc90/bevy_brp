# Bevy 0.20 升级方案导航（按全仓可编译节点拆分）

基于前一版完整技术方案重新分区，目标仓库为 `slc90/bevy_brp`，固定源码基线 `ae9fdaec25cb1f02b501f6e07199a27df144501e`；从 Bevy `0.19.1` 迁到正式 `v0.20.0`。**本包是待执行方案；目前没有对仓库实施修改，也没有验证这些准出条件已经成立。**

## 整体目标与阶段性门槛

旧拆分以 Pointer、截图等职责为分界，第一个阶段先改依赖，可能让 workspace 在多个阶段持续编译失败。本次调整为：**第 01 阶段保持旧版可编译；第 02 阶段集中完成整个 0.20 workspace 的 API 兼容与编译恢复；第 03—06 阶段在持续可编译的前提下验证并修复功能行为。**

编译准出条件是未来执行时必须满足的工程验收标准，而不是静态文档能预先保证的成功结果。一旦执行时 `cargo check --workspace --all-targets --locked`、测试产物构建或必要独立 feature 检查失败，该阶段不能宣告完成、不能把编译失败挪到下一阶段。图形环境缺失引起的真实运行测试未测要单独记录，不能冒充功能通过。

## 唯一执行顺序

1. **[01. 建立 Bevy 0.19.1 可编译基线](01-bevy019-compilable-baseline.md)**：在旧版本下建立可重复的编译、工具目录与运行证据，不动引擎依赖。旧版已有失败和环境限制单独标注。
2. **[02. 一次性完成 Bevy 0.20 全 workspace 编译兼容](02-bevy020-workspace-compilation.md)**：统一升级根依赖/lockfile，在同一阶段解决所有 crate、examples、tests 的阻塞性 API 变化；准出为全目标可编译与独立 feature 不被测试宿主掩盖。
3. **[03. 验证 Custom Pointer 输入生命周期与桌面隔离](03-pointer-behavior-regression.md)**：在能编译的版本上检验独立 Pointer、Cancel、物理交接、队列和 activity，恢复旧行为并保持全仓编译门槛。
4. **[04. 验证 UI 截图的精确裁剪与图像语义](04-ui-screenshot-behavior-regression.md)**：检验多重旋转裁剪、无界轴、padding、坐标/像素与旧截图边界，保持全仓编译门槛。
5. **[05. 验证 BRP Runtime 与 MCP 的整条协议链](05-runtime-mcp-protocol-regression.md)**：在真实宿主上核验 Main/Render wake-aware transport、超时/watch/清理，以及 MCP registry/type guide/CRUD 错误契约，保持全仓编译门槛。
6. **[06. 完成全链回归、独立消费与版本交接](06-full-regression-consumer-handoff.md)**：在最终不可变源码状态重新跑全量构建/特性、Windows 回归与独立消费者，记录图像/端口/进程证据并更新事实文档。

**顺序固定：01 → 02 → 03 → 04 → 05 → 06。** Pointer 与截图之间没有天然算法依赖，03→04 的线性顺序是为了避免共用 Extras/fixture 的更改同时发生。第 05 阶段把 runtime 与 MCP 收在一起，是因为它们共同承担 App→BRP transport→MCP 的协议链验收，而不是允许两个互不相关的阶段同时执行。

## 关键交接规则

- 阶段 01 在旧 0.19.1 源码上验证；阶段 02 才第一次改动 Bevy 版本，切换后不得用临时 stub、删除 example 或关闭 feature 伪造编译成功。Pointer 和截图在第 02 阶段必须采用保留语义的 0.20 API 适配；第 03、04 阶段完善行为覆盖与真实证据。
- 第 03—06 阶段每次改动后都重跑整个 `cargo check --workspace --all-targets --locked` 和相应测试产物构建；代码编译与行为验收是两个独立门槛。
- 不动外部消费者，不碰旧 `v0.3.1`，不创建未授权新 tag；最终以经过验证的完整 revision 同时匹配 runtime、Extras 与 MCP。所有代码变更按仓库规则进行独立 code review。
- 具体技术算法、背景、示例、例外、失败路径和完整验证命令都写在对应小方案中，不在本总览中简写替代。
