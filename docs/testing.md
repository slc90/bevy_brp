# 回归验证入口

从仓库根目录运行。普通检查不需要窗口；`cargo test` 会执行各 package 的 unit/integration test，但不会启动 MCP stdio 与 Bevy App 的完整链路。

```powershell
cargo fmt --all -- --check
cargo test --workspace --locked --no-fail-fast
```

需要缩短反馈时，可先运行直接相关的 package，例如 `cargo test -p bevy_brp_mcp --locked`、`cargo test -p bevy_brp_extras --locked` 或 `cargo test -p bevy_brp_runtime --locked`。`crates/extras/tests/agent_tool_registration.rs` 由 `bevy_brp_extras` 的 Cargo test harness 执行。完整编译、Clippy、build 和独立 feature 检查入口见 [AGENTS.md](../AGENTS.md#验证入口)。

## Windows 图形与协议链

在有可见桌面、可用图形设备、PowerShell 7、空闲测试端口和已构建宿主的 Windows 会话运行：

```powershell
cargo build -p bevy_brp_mcp -p bevy_brp_test_apps --locked --features bevy_brp_mcp/mcp-debug
& .\tests\regression.ps1 -Port 15712
```

`tests/regression.ps1` 调用[方案 01 的固定 MCP 重放器](../plans/refactor/assets/01-baseline/replay.ps1)，其中包含两次仅诊断构建提供的 server trace 工具调用，因此必须先构建 `mcp-debug`。脚本再检查握手与工具目录、BRP 发现、Sprite Transform 从 `0` 到 `42` 的修改、watch 建立与结束、输入排队、两条 App 日志中的固定 marker、两次 clean shutdown，以及 `NatesList` 截图的尺寸和实际像素。当前基线中 `test_app` 无 primary window 的截图调用预期返回 `brp_extras/screenshot` 的 `-32603` 错误及对应原因；该断言只固定本场景已记录的行为。重放器会启动 `test_app` 和 `extras_plugin`，恢复后者窗口。脚本在运行前拒绝占用的端口、同名 App 或 MCP 进程；成功后确认端口与进程退出，并删除本轮临时记录和唯一命名的 App/watch 日志，同时把共享 trace 文件恢复到运行前长度。失败时保留记录目录及尽可能完整的交互记录，并清理、等待和复查本轮 App、MCP 进程及端口。用 `-OutputDirectory <尚不存在的目录路径>` 或 `-KeepArtifacts` 可保留成功记录。脚本不修改方案 01 的 before 样本。

普通构建的 MCP 公共协议链使用 `cargo build -p bevy_brp_mcp -p bevy_brp_test_apps --locked` 后运行 `& .\tests\public-mcp-regression.ps1`。普通和诊断两种 tool registry 及 trace 隔离可在相应构建后运行 `& .\tests\diagnostics-regression.ps1`；诊断构建传入 `-DebugBuild`。每次切换 feature 后先重建 MCP binary，避免复用上一种构建。

`BrpRuntimePlugin::with_port` 的真实监听验证使用独立示例：

```powershell
cargo build -p bevy_brp_runtime --example runtime_custom_port --locked
& .\tests\runtime-port-regression.ps1
```

脚本要求 15752（代码配置的 Main）、15702（默认 Main）和 15703（Render）均空闲。它移除子进程继承的 `BRP_EXTRAS_PORT`，检查自定义端口能响应 `rpc.discover`、默认端口未监听，并经 `brp_extras/shutdown` 确认进程和自定义端口清理。环境变量覆盖优先级另由 runtime unit test 验证。

如无交互式桌面、GPU 或图形窗口，跳过该入口并记录为**未测及缺失条件**，不要把 Cargo 测试通过等同于端到端通过。成功运行的 MCP 交互记录在 `interaction.jsonl`，完整摘要在 `summary.json`。失败时先查同目录的 `failure.json` 和已写出的部分 `interaction.jsonl`，再按实际生成情况检查 `tools-list.json` 与截图。进程、端口或 trace 文件未恢复属于测试失败。

## 覆盖边界

`tests/test-duplicate-a` 与 `tests/test-duplicate-b` 保留跨 package 同名 target 的发现/消歧场景；`tests/test-app` 的同名 bin/example 保护 target kind 选择，`extras_plugin/screenshot_fixtures.rs` 提供截图边界和确定性图像。它们是供 MCP 启动的宿主，不因 `cargo test --workspace` 运行而自动完成协议验证。runtime 的 mailbox/deadline 与持续动作推进、extras 的 agent tool 注册、输入和截图边界仍由各 package 的测试覆盖。

本入口重放方案 01 中已证实的场景。输入只检查排队结果，未断言最终 UI 文本；其他键盘、鼠标、手势、资源 CRUD、entity 创建/销毁/重设父级、事件与 type guide 的完整真实调用仍未覆盖。MCP 自诊断在该重放中检查调用成功，未将 trace 文件内容或 token 用量设为 contract。原始 before 数据与当时的两个 Windows build-freshness 测试失败记录在[基线记录](../plans/refactor/01-baseline.md)；当前测试结果须重新运行判定，不能沿用旧结论。
