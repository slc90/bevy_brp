# bevy_widgetry BRP 迁移拆分总览

## 目标

本次只完成一件事：把 `bevy_widgetry` 中为 Agent GUI 调试维护的 BRP 基础设施迁入 `slc90/bevy_brp`，让 Widgetry 只作为该工具链的消费者，不再 vendoring 或维护 BRP transport / activity 实现。

本次不重新设计 MCP、Extras 或 Runtime；迁移完成后再单独学习和演进 `bevy_brp`。

## 执行顺序

1. [bevy_brp 仓库基线、清理与版本线](01-bevy-brp-repository-baseline.md)  
   Repository: `slc90/bevy_brp`. 建立后续迁移所依赖的独立 bevy_brp 仓库基线，清理不再使用的上游开发设施，并固定本地 Cargo 配置与 0.1.0 版本线。
2. [bevy_brp Extras 已验证改动正式迁入](02-bevy-brp-extras-migration.md)  
   Repository: `slc90/bevy_brp`. 把 Widgetry vendored `bevy_brp_extras` 中已经运行验证的 methods-only transport 和跨帧 activity lifecycle 行为正式纳入 fork 的 `extras/`。
3. [bevy_brp 迁入独立 Runtime crate](03-bevy-brp-runtime-migration.md)  
   Repository: `slc90/bevy_brp`. 将 Widgetry Gallery 中已经工作的 wake-aware BRP HTTP transport 与 progress controller 原样迁入新的 `bevy_brp_runtime` crate。
4. [bevy_brp 测试目标对齐 Agent 实际链路](04-bevy-brp-test-chain-adjustment.md)  
   Repository: `slc90/bevy_brp`. 让 bevy_brp 自带的主测试 App 覆盖 Agent 真正使用的 Runtime 链路，同时保留 Extras 专项 fixture、无 Extras fallback 以及同名 target 消歧等原有专项验证职责。
5. [bevy_brp 形成 v0.1.0 可消费基线](05-bevy-brp-v0.1.0-consumable-baseline.md)  
   Repository: `slc90/bevy_brp`. 在 bevy_brp 内部迁移完成后，做必要的文档收口与本地验证，形成可供其他项目通过 Git tag 稳定消费的 `v0.1.0` 基线。
6. [bevy_widgetry 切换外部 Runtime 并移除本地 BRP 基础设施](06-bevy-widgetry-external-runtime-cutover.md)  
   Repository: `slc90/bevy_widgetry`. 让 Widgetry 从 BRP transport/activity 的维护者变为 `bevy_brp_runtime@v0.1.0` 的单纯消费者，删除 vendored Extras、Gallery 内 BRP 实现以及只为这些实现服务的依赖。
7. [bevy_widgetry 同步当前文档并完成外部 Runtime 集成验证](07-bevy-widgetry-docs-and-validation.md)  
   Repository: `slc90/bevy_widgetry`. 使 Widgetry 的当前架构文档、GUI 调试规则和本地验证结果与“只消费外部 `bevy_brp_runtime`”的新事实一致。

## 前后关系

整条执行链是唯一线性顺序：先完成 `bevy_brp` 仓库的 01–05，形成可消费的 `v0.1.0` 基线；再进入 `bevy_widgetry` 仓库执行 06–07。每份小方案都只在一个仓库内收口，不设计跨仓库实施单元。
