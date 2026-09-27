# 测试规则

## 测试义务

- 新增或改变可观察行为时必须有对应测试。
- 修复 bug 时优先增加能复现问题并防止回归的 regression test。
- 纯结构重构若行为不变，不要求为重构本身强行增加测试，但已有测试必须覆盖所保持的关键行为。

测试要验证本仓库的 contract、组合边界和 regression，不重复证明 Rust、Bevy 或第三方 crate 已保证的内部行为。

## 测试层级

### Unit test

局部解析、转换、state transition、error mapping、deadline、cleanup 和私有逻辑放在相应 module 的 `#[cfg(test)] mod tests`，贴近实现。

### Integration test

以下行为优先使用 crate `tests/`、`test-app` 或现有 example/fixture：

- 从 crate 外部验证公共 API；
- 多 module/crate 的真实组合；
- Bevy App、ECS schedule、render world 或 Winit integration；
- MCP stdio、BRP HTTP/SSE、process 和 filesystem 边界。

不得为了测试方便把私有实现改成 `pub`。测试共用基础设施只有在实际重复并具有稳定职责时才提取。integration test 的 `tests/` 目录按需要允许使用 `mod.rs`。

## Type 与 wire contract

涉及 MCP/BRP 的测试应按变化验证必要 contract：

- JSON field 名、类型、required/optional 和默认值；
- tool annotation、help text 与 registry 可见性；
- structured result 和 error data；
- entity ID、port、path、duration 等边界值；
- unknown method/type、malformed response 和 transport failure；
- timeout、cancellation、watch end、shutdown 与 cleanup。

不要只断言 message 文案而遗漏结构化 contract，也不要在 contract 未要求时把内部顺序或 debug 文本固化成测试。

## Fixture 边界

现有 fixture 具有明确用途：

- `test-app` 提供真实 App、runtime、input、event 和 screenshot 宿主；
- `test-duplicate-a` / `test-duplicate-b` 保护 target 名冲突与消歧；
- screenshot fixtures 保护 camera、UI/AABB crop 和错误边界。

删除、合并或改名 fixture 前必须确认其 regression 用途已有等价覆盖。不得因为它不像生产代码或造成重复 binary 名 warning 就直接删除。

## 平台与环境

平台相关测试应把真正的 product failure 与环境限制分开。Windows path、dep-info、process、socket 和 window 行为不能用 Unix 假设替代。

测试失败时先确认是新增 regression、既有失败还是环境缺失。不得把失败测试静默忽略、放宽断言或标成 flaky 来通过当前任务。

## 验证范围

优先运行直接相关的最小测试以缩短 Red/Green 循环，完成后按风险扩大到 crate 或 workspace：

```bash
cargo test -p <package-name> <test-filter> --locked
cargo test -p <package-name> --locked
```

完整 workspace 和 feature matrix 命令统一见 [AGENTS.md 的验证入口](../AGENTS.md#验证入口)。修改公共 feature、workspace dependency、proc macro、跨 crate contract 或 build/launch 路径时，补充相关 workspace/all-targets 验证。

## 运行时验证

tool listing、launch、watch、日志、input、screenshot、shutdown 或 wake-aware runtime 变化需要真实 MCP→BRP 验证时：

1. 使用独立测试 port；
2. 固定 target、参数和预期；
3. 验证结构化响应和实际副作用；
4. screenshot 变化要检查实际图像，不只检查文件存在；
5. 停止 watch 并 clean shutdown；
6. 确认进程和 port 清理。

运行时验证不替代自动化测试。无法运行时应报告未测项和缺失条件。
