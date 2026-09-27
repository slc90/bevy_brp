# Bevy BRP Agent Guide

本文件是 Agent 进入仓库时的统一入口。详细工程约束位于 `rules/`，当前事实以源码、Cargo metadata、rustdoc、MCP help text 和 `crates/runtime/UPSTREAM.md` 为准。

## 默认读取

任何代码、测试、构建配置或其他工程行为修改任务，开始实施前必须读取：

- `rules/task-scope.md`
- `rules/development.md`
- `rules/code.md`

根据任务内容继续读取：

- 依赖、Cargo 配置、feature 或 package/crate 调整：`rules/dependencies.md`
- 注释、rustdoc、help text 或其他文档：`rules/documentation.md`
- 新增行为、行为修改、bug 修复、fixture 或测试：`rules/testing.md`
- 日志、trace、诊断输出或应用日志：`rules/logging.md`
- Git 提交或 commit message：`rules/git.md`

一个任务可以同时命中多个规则文件，必须读取全部命中规则。

当前没有 architecture 专属规则或 MCP 公共协议专属规则。遇到相关任务时，以现有源码、rustdoc、MCP help text、`crates/runtime/UPSTREAM.md` 及实际 Cargo metadata 为事实依据，不得预先假设未来 crate、module 或 tool surface。

## 上下文边界

`plans/` 默认不属于开发上下文，不得主动扫描或把历史计划当成当前事实。用户当次明确提供的方案及该方案点名的交接材料可以作为任务输入；它们仍不能覆盖当前源码、已实施文档或工程硬约束。

如果任务与规则或必要前提真实冲突，不得自行放宽规则，应停在受影响处并说明冲突。

## 事实文档同步

事实与规则分开维护：

- `rules/` 规定开发约束；
- rustdoc、MCP help text 和 `crates/runtime/UPSTREAM.md` 描述已实施事实；
- `plans/` 保存方案和历史设计输入。

修改 workspace 成员、crate 角色、生产依赖关系、公共消费方式、BRP/MCP 公共行为或上游派生事实时，必须在同一任务中更新实际承载该事实的现有文档。没有已实施事实时，不创建空文档或把目标状态写成当前状态。

## 命令环境

Windows 环境统一使用 PowerShell 7（`pwsh`），禁止使用 Windows PowerShell 5（`powershell.exe`）。这项约束不改变项目对其他平台的支持范围。

## 验证入口

按变更风险和命中规则选择必要验证，不机械执行无关命令。Windows MSVC 开发使用 Rust 1.98.1 或更新版本，活动工具链需安装 `rustfmt` 和 `clippy`。定向测试可用 `cargo test -p <package-name> --locked`。完整验证入口：

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
cargo build --workspace --locked
```

独立 feature 检查入口：

```powershell
cargo check -p bevy_brp_extras --locked --no-default-features
cargo check -p bevy_brp_extras --locked --no-default-features --features diagnostics
cargo check -p bevy_brp_extras --locked --no-default-features --features ui
cargo check -p bevy_brp_mcp --locked --no-default-features
```

涉及 MCP/BRP、进程 lifecycle、watch、日志、输入或截图等跨进程行为时，还应按 `rules/testing.md` 运行与修改直接相关的真实测试宿主或协议链。已知失败、新增失败和未运行项必须分开报告。

## 独立 Code Review

任何产生代码或工程行为改动的任务，在实施和必要验证完成后都必须执行独立 Code Review。工程行为改动包括源码、测试、构建脚本、项目配置和规则文件。纯 Markdown 且不改变工程行为的修改不强制 Review。

Review 流程：

1. 施工 Agent 完成修改和必要验证。
2. 启动一个全新的 reviewer subagent。
3. reviewer 必须调用 `$code-review` Skill，静态审查完整 working-tree change，只返回 findings，不修改文件。
4. 若存在 findings，由施工 Agent 修复并完成必要验证，再启动另一个全新 reviewer 重审。
5. 持续到 reviewer 返回 `No review findings.`。

每轮必须使用新的 reviewer，不得复用旧 reviewer 上下文，也不得由施工 Agent 自审代替。缺少 `$code-review` Skill、subagent 能力或 target collection 条件时，必须明确报告阻塞，不能把 Review 标为完成。
