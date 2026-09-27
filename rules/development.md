# 开发流程规则

## 基本流程

生产代码开发默认遵循：

```text
理解任务与现有 contract
    ↓
Type-Driven Development
    ↓
Test-Driven Development
    ↓
最小必要实现
    ↓
必要重构与验证
```

按小的可观察行为循环推进，不一次设计未知的完整未来。

## Type-Driven Development

实现前先明确当前功能涉及的：

- request、response、error 和 wire data；
- Component、Resource、Event、Message 及其他 ECS type；
- lifecycle state、ownership、timeout、cancellation 和 shutdown 状态；
- 输入、输出、不变量与边界条件；
- crate 内部或公共接口。

优先用 type 表达真实状态和协议约束，避免特殊值、相互依赖的 bool 或隐式约定。不要为形式上的 type safety 引入没有当前价值的 wrapper、generic 或抽象层。

Type 设计可以在测试和实现提供新证据后调整，不把早期设计视为不可变。

## Test-Driven Development

新增可观察行为或修复 bug 时采用 Red → Green → Refactor：

1. 先编写表达当前行为、边界或 regression 的测试。
2. 确认测试因尚未实现或仍存在的问题而失败。
3. 编写使该测试通过的最小实现。
4. 在测试保护下做当前需要的重构。
5. 重跑直接相关测试。

不要先完成实现，再补一个只能覆盖现状、无法证明需求的测试。若 type 无法合理表达行为，应先调整 type，而不是增加临时绕过。

## Bug 修复

优先留下能够稳定复现问题的 regression test。只有当问题无法通过合理的自动化测试覆盖时，才可以不新增测试，并在交接中说明原因和替代验证。

## 纯重构

纯重构不要求人为制造 Red。开始前应确认已有测试能保护需要保持的行为；覆盖不足时先补行为测试。重构不得改变可观察 contract。

## 跨进程与运行时行为

以下变化通常不能只靠局部 unit test 判断：

- MCP stdio handshake、tool listing 或 tool call；
- BRP HTTP/SSE、watch、timeout、wake 或 cleanup；
- 应用发现、build、launch、status、日志和 shutdown；
- screenshot、keyboard、mouse 或其他运行时输入；
- Windows path、process 或 socket 行为。

这类任务在自动化测试之外，应使用 `test-app`、现有 examples/fixtures 或本地 MCP→BRP 链进行与改动直接相关的运行时验证。只验证当前行为，不机械调用所有工具。

真实运行会修改外部状态时，应使用测试端口和仓库提供的宿主，结束后停止 watch、正常关闭应用并确认进程/端口清理。无法完成必要运行时验证时必须报告缺失条件，不得把静态检查当成等价证明。

## 必要重构边界

测试通过后只做已经由当前实现证明有价值的重构，例如消除实际重复、澄清职责或简化真实 control flow。不得为猜测中的未来 transport、client、tool 或平台提前设计扩展点。
