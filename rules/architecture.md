# 架构边界规则

## 事实与决策依据

先从根 `Cargo.toml`、各成员 manifest、源码入口及 [`docs/architecture.md`](../docs/architecture.md) 确认已实施的 package、target、依赖和公共入口。架构文档记录当前事实；改变 workspace 成员、crate 职责、生产依赖方向、测试宿主归属或公共入口时，同项更新该文档。普通模块内部调整或不改变这些事实的依赖版本更新，不机械改架构文档。

`plans/` 是当次指定时才读取的设计输入。需要与现有规则或公共 contract 冲突的架构变更，应先明确决策及影响，不能以目标目录图替代已实施事实。

## Crate 与依赖边界

新增、合并、拆分或删除生产 crate 必须有实际的编译目标、依赖/feature、运行位置、ownership 或外部消费边界作依据，并列出受影响的直接消费者和迁移方式。一个模块有多个职责、只有一个消费者或目录需要整齐，都不能单独证明要改变 crate 数量。proc macro、可独立使用的插件与不同 transport 需求按各自真实边界判断，不预先规定未来 crate 数量。

依赖沿职责方向建立，不为调用方便使协议服务依赖 Bevy App 内部实现，或让基础扩展库依赖组合它的 runtime。生产代码不依赖测试宿主。测试 package 可以组合所需生产 crate，但其较宽 Bevy feature 不得成为生产 crate 的默认依赖要求。Cargo 版本、workspace/path dependency 和 feature 的具体规则见 [`dependencies.md`](dependencies.md)。

## 模块与可见性

保留能说明职责和 ownership 的模块边界。跨模块共享并不自动要求独立 crate 或多层 facade；改动应从当前调用点出发，避免为未知复用增加转发层。入口模块负责装配和必要的对外 re-export，具体实现留在负责该行为的模块。

默认使用 private；确需在同一 crate 的模块间共享时选择最小足够的可见性，只有外部消费入口才使用 `pub`。binary 内的 `pub`/re-export 不等于存在外部 Rust library API；library 的 `pub` 则须按公共 API contract 处理。不得为测试绕过可见性，或在目录移动时顺带扩大/收窄公共入口。具体源码组织与 lint 约束见 [`code.md`](code.md)。

## 验证与文档

纯结构重构先确认已有测试覆盖需要保持的行为；跨 package、feature 或路径变化按 [`testing.md`](testing.md) 验证独立 package、测试宿主和真实 MCP→BRP 链。新目录中的 `tests/` 只有在明确的 Cargo package 或 harness 下才是可执行测试入口。公共消费路径变化须针对受影响的 Git/package 或本地 path 消费方式验证，并同步已实施事实；不得自动修改外部仓库。
