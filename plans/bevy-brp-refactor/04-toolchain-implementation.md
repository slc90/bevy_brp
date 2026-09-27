# 04 · 统一 Rust 工具链与 workspace 配置

**类型：实施方案。** 本文件属于待审改造方案；批准后按唯一执行顺序使用。

## 本项的共同边界

目标仓库是 `slc90/bevy_brp`，以方案 01 固定的当前 `main` 为起点，承接前序已完成的改造；方案 01 自身负责固定这个起点。不同步上游，不修改 widgetry，不安排用户学习仓库。Codex 只做完成本项目标需要的定位与实施，不顺手处理旁支目标。

先读本仓库现有 `AGENTS.md` 及命中规则。只定向读取当前方案点名的交接材料，不主动扫描历史 `plans/`；未实施的计划不能代替当前源码和事实文档。若任务与硬规则或必要前提冲突，停在受影响处并说明，不能自行放宽。

## 目标

让任意干净检出都能按同一套工程配置开发和验证。目标状态是有一个权威 Rust 版本、一套格式与 lint 口径，以及清楚的 workspace 配置继承。

## 范围与输入

输入为用户已确认的 `plans/refactor/03-toolchain-decision.md` 和当前 `AGENTS.md`、相关规则。只调整工程配置及为该配置兼容所必需的最小改动，不改变 crate 职责、公共 API、MCP tool 集合或业务行为。

## 实施后的样子

根 `rust-toolchain.toml` 固定已确认版本和必要组件。根 `Cargo.toml` 集中保存选定的 workspace metadata、共享依赖与 lint；子 crate 继承适用字段，保留本身的名称、用途和 feature 选择。

```toml
# 示意继承关系，不是未确认即可复制的完整配置。
[package]
edition.workspace = true
version.workspace = true
repository.workspace = true

[lints]
workspace = true
```

`rustfmt.toml`、必要的 `clippy.toml`、`.cargo/` 和辅助工具配置与该基准一致。已决定删除的配置，其命令、脚本引用和文档同步清理。`Cargo.lock` 的变化能够解释；不顺手执行无关依赖更新。

开发命令集中写在 README 或确有需要的开发文档中，其他入口引用它，不长期维护几份不同命令。若当前配置变化不影响 architecture 文档记载的事实，不为此重写架构说明。

## 验证与完成标准

使用确认的工具链运行必要检查。基本命令形态为：

```text
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo build --workspace --locked
```

具体 feature、target 和环境限制使用方案 03 的结论，不自行扩大平台承诺。确认 lockfile 与配置相符后，用 `--locked` 验证没有隐式漂移。比较方案 01，明确哪些失败原来已有、哪些是此次新增。因选择的新基准造成的失败必须处理或明确阻塞，不能只提交配置即宣告完成。

产出实际工程配置和一致的开发入口；完成必要验证与独立 Code Review。若解决兼容问题必须先做另一项架构或业务决定，停在该边界，不把大规模重构混进本项。

## 与前后方案的关系

只在方案 03 已获确认后执行。完成后进入明确冗余清理；后面的代码重整必须沿用这里建立的工程基准。

## 实施交接要求

未获批准的公共行为和消费方式保持不变。本仓库内直接受影响的位置一起处理；widgetry 等外部消费者只说明影响，不自动修改。不要覆盖用户已有工作，不自动提交、推送或发布。

按照当前规则执行必要验证。产生代码或工程行为改动时，由新的独立 reviewer 调用 `$code-review` 审查完整 working-tree change；施工 agent 修复 findings 后使用全新 reviewer 复审。没有独立审查能力时报告未完成，不能自审代替。纯 Markdown 且不影响工程行为的改动不强制执行代码审查。架构事实或公共 MCP 契约实际变化时同步对应 `docs/`。

完成后交接实际变化、验证结果、未测项和能否继续；只完成本项，不自动执行下一方案。已有问题、新增失败和环境限制分别报告。

## 方案导航

[返回总览](00-overview.md) · [详细总方案](bevy-brp-refactor-plan.md)

前一份：[03 · 确定可复现的 Rust 工程基准](03-toolchain-design.md)

后一份：[05 · 清理已经证实冗余的仓库资产](05-repository-cleanup.md)
