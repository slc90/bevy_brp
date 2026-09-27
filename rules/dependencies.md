# 依赖与 Cargo 规则

## Workspace 统一管理

能够共享的外部 crate 版本统一定义在根 `Cargo.toml` 的 `[workspace.dependencies]`，成员 crate 使用 `workspace = true`。不得在多个成员中独立维护同一依赖版本。

Workspace 内部依赖使用 workspace/path dependency 指向当前成员。不得为发布设想提前改成 Git 或 crates.io 依赖。

## 新增依赖

新增依赖必须服务于当前明确目标。引入前确认：

- 标准库、Bevy、当前依赖或小段本地实现不能合理满足需求；
- feature 和 default-features 是最小必要集合；
- license、目标平台和 Rust toolchain 与 workspace 兼容；
- 没有把仅测试需要的 crate 放进生产依赖；
- `Cargo.lock` 与直接受影响文档同步。

不得为了一个小 helper、可能的未来复用或统一风格增加依赖。

## Bevy feature

生产 crate 保持定向 feature；不要把 `test-app` 为类型和渲染 fixture 使用的 Bevy 全 feature 传播到 `extras`、`runtime` 或 `mcp`。

修改 Bevy feature 时应检查 feature unification 对 workspace build、独立 package build、Windows target 和启动 freshness 判断的影响。不得仅凭 workspace 已能编译就假设单 crate 消费方式也成立。

## Crate 与 package

现有 crate/package 名称和角色属于公共消费事实。普通任务不得改名、新增 crate、合并 crate 或改变发布方式。若当前目标明确要求结构变化，应同步 workspace members、内部依赖、lockfile、直接测试宿主及实际存在的消费说明。

不要在通用规则中预先冻结未来 crate 数量或 module 层次；architecture 决策必须依据对应任务和当前事实单独完成。

## Git 消费

当前仓库以 Git tag 提供消费者依赖。涉及 tag、Git URL、版本号或安装命令的变化必须同时检查 runtime 消费和 MCP 安装两条路径，但未经发布任务授权不得创建 tag、发布或修改外部消费者。
