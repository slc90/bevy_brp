# 文档规则

## 事实、规则与方案分离

- rustdoc、MCP help text 和 `runtime/UPSTREAM.md` 描述已实施事实与使用 contract。
- `rules/` 只规定开发约束。
- `plans/` 保存方案和历史设计输入，默认不作为当前事实来源。

不得把未实现目标写成当前能力，也不得为了路由创建空的 architecture 文档。修改已记录事实时，同一任务更新实际承载它的文档。

## 语言

遵循被修改文件的既有受众和语言：

- 面向公开消费者的公共 rustdoc 和 help text 默认沿用英文；
- 项目工程规则和已有中文内部说明使用中文；
- 同一段落不要无理由切换语言；
- API、type、method、field、protocol、runtime、transport、lifecycle 等技术术语和代码标识保留英文原名。

不得为了统一语言而在当前任务中批量改写无关现有文档。

## Rustdoc 与声明注释

公共 API 必须满足 workspace `missing_docs` contract。Rustdoc 应说明正确使用所需的：

- 职责和适用边界；
- 参数、默认值、单位和特殊值；
- error、timeout、cancellation 与 cleanup 语义；
- 必需 Plugin、Component、Resource、feature 或平台前提；
- 与 BRP/MCP wire contract 相关的稳定行为。

私有声明只在职责或约束不能从代码直接看出时添加注释。禁止机械复述名称。

Doctest 可以用于简短、稳定且能真实编译的公共 API 示例。不要用 doctest 承载需要窗口、网络、进程或易受环境影响的集成流程；这类行为使用测试宿主和 integration test。

## MCP help text 与 schema

`mcp/help_text/` 是 tool 使用 contract 的组成部分。修改 tool 名称、参数、默认值、互斥条件、结果、错误或推荐调用顺序时，必须同步：

- 参数/结果 type 与 schema；
- help text；
- 相关 rustdoc 示例；
- contract/regression test。

help text 应给出可直接使用的 JSON 形状，并明确 port、entity ID、name matching、path、timeout 等关键约束。不要描述当前 tool 无法保证的结果。

## 注释位置与内容

注释独立成行，不使用行尾旁白。function 内只解释关键步骤、非显然逻辑、协议原因或平台差异。

测试注释只在场景或方法无法从 test 名和 fixture 清楚表达时添加；不要为每个简单测试强制写重复注释。

## 上游来源与变更记录

修改派生自上游的 transport 或协议实现时，保持 `runtime/UPSTREAM.md` 的来源、版本、license 和本地语义差异准确。

crate 发生面向用户的行为或 contract 变化时，应同步现有 rustdoc 和 MCP help text，不为形式完整新建发布文档。纯内部重构、测试或规则修改不机械添加 release note。
