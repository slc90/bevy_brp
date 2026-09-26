# bevy_brp 测试目标对齐 Agent 实际链路

**Repository:** `slc90/bevy_brp`

## 目标

让 bevy_brp 自带的主测试 App 覆盖 Agent 真正使用的 Runtime 链路，同时保留 Extras 专项 fixture、无 Extras fallback 以及同名 target 消歧等原有专项验证职责。

## 范围

仅限 `slc90/bevy_brp` 中 `test-app/`、`test-duplicate-a/` 和 `test-duplicate-b/` 的测试角色与相关依赖。

## 预期产出

主 test targets 使用 `BrpRuntimePlugin`；`extras_plugin.rs` 继续直测 Extras；`no_extras_plugin.rs` 继续验证无 Extras fallback；duplicate fixtures 继续验证 `brp_launch` 目标发现与消歧。

## 与前后方案的关系

依赖方案 03 已提供 Runtime crate。它把 bevy_brp 内部的功能链接到可验证状态，为方案 05 的仓库整体验证和 `v0.1.0` 可消费基线做准备。

## 方案内容

## 7. test-app 调整

让主测试目标覆盖 Agent 实际使用的完整链路。

以下目标改用 `BrpRuntimePlugin`：

```text
test-app/src/bin/test_app.rs
test-app/examples/event_test.rs
test-app/examples/mouse_test.rs
test-app/examples/test_app.rs
```

保留专项 fixture 的原始职责：

- `extras_plugin.rs` 继续直接使用 `BrpExtrasPlugin`，专门覆盖 Extras 能力和其配置 API。
- `no_extras_plugin.rs` 继续只使用 `RemotePlugin + RemoteHttpPlugin`，验证目标 App 没有 Extras 时 MCP 的 fallback 行为。
- `test-duplicate-a/b` 保留，继续验证同名 example 的 path 消歧和 `brp_launch` search order。
