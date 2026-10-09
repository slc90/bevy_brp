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

普通 Custom Pointer 的 Windows 桌面回归使用 `pointer_test` 宿主和共享 stdio client `tests/mcp_stdio.py`，独立于历史输入接受回放：

```powershell
cargo build --workspace --locked
cargo build --workspace --example pointer_test --locked
cargo test --workspace --example pointer_test --locked
python tests/pointer-regression.py --port 15828 --output target/pointer-evidence
```

输出目录必须尚不存在、测试 port 必须空闲。测试期间保持系统鼠标静止；脚本只读 Win32 cursor/foreground PID，将 inactive 宿主窗口放在 cursor 基线之外，未激活窗口或注入 OS 输入。宿主保留 `WinitSettings::desktop_app()`，使用默认 UI Picking backend、stock Button、普通拖动目标与 scroll overflow。fixture observer 按 Pointer 输入更新真实 Node/ScrollPosition，脚本经 `fixture/state` 读出实际行为，不直接写业务 state。场景覆盖三按钮、首次 move+click/后续无 Move 点击、零时长按键、引擎多击阈值内外、最小步数跨目标拖放、Line/Pixel 滚动、stock Button 离开目标后取消、唯一 Cancel/无成功激活、多轮 UUID 复用、严格 control 参数、已销毁窗口、idle active Pointer 有界续帧与 activity 归零。

证据分开保存：`tools-list.json`、`interaction.jsonl`、两窗口截图、`desktop-baseline.json` / `desktop-after.json`、`summary.json` 和 fixture 日志。截图须实际查看。raw 计数包含 individual mouse/cursor channels 与 WindowEvent mouse variants；keyboard/gesture 不混入零注入断言。只有 cursor 前后相等、OS foreground 未变、窗口实际未获焦点、raw 未增长且 UI 断言通过，才能声称桌面隔离通过；失败保留记录，不能用 headless 测试替代。脚本 finally 释放 Pointer、正常关闭本轮 PID、结束 MCP 并核查端口；正常关闭失败会报告失败并只终止本轮已核实的 PID。Native Window move/resize 区域不支持并排除在场景之外。

`mouse_test` 的 raw/native 字段只展示物理输入，不能当作新普通 BRP 鼠标方法的成功计数；cuboid Picking 部分仍接收 Pointer。`extras_plugin` 通用组件/截图 fixture 与 `event_test` 事件用途保持独立。Extras 私有 Picking/RayMap 回归覆盖物理交接、不同 DPI 的窗口逻辑坐标、无关 raw 保留、暂停虚拟时间、generation 及失效；它们不证明 OS cursor 隔离。

2026-10-08 本地实测：受控 Pointer 桌面链通过，cursor 前后均为 `(944, 615)`，foreground PID 未变，两个宿主 Window 均未获焦点且 native cursor 为 null；五类 raw 计数均未增长。真实按钮、双击、拖放和 Line/Pixel 滚动断言通过，两张截图已查看；idle 的两次查询间隔 0.5 秒，updates 从 482 增至 488，activity 为零；窗口销毁后归为 inactive 并保留来源 method/window 错误，App/MCP/端口清理通过。记录位于本地忽略目录 `target/pointer-stage05-controlled/`。此前一轮 cursor 发生变化，不能作为隔离通过证据；受控重跑后才得到上述结论。

同轮 workspace fmt/check/Clippy/test/build、Extras 三种独立 feature 与 MCP no-default check 通过；stock Button 取消回归作为 `pointer_test` example test 自动随 workspace test 执行。共享 stdio client 下的 keyboard 桌面链、普通公共 MCP、普通/诊断目录回归（47/49 工具）以及诊断历史图形回放通过，历史 `NatesList` 图像已查看，进程/端口和 trace 清理通过。既有同名 fixture build warning 与 9 个 ignored doctest 保留。Native Window move/resize、Widgetry 具体控件和硬件多 DPI 桌面未验收；DPI 逻辑坐标已有自动化覆盖。

06 固定版本验证（2026-10-08）：四个生产 package、内部 version/path 和 Cargo.lock 一致为 `0.3.0`；workspace fmt/check/Clippy/test/build 与四项独立 feature 检查重新通过，保留同名 fixture warning 和 9 个既有 ignored doctest。隔离消费者同时引用 runtime 与 Extras，公共 Plugin/activity 入口通过独立 workspace 的 `cargo check --locked --offline`。本地 MCP 以 `cargo install --path crates/mcp --locked --debug --root target/pointer-stage06/mcp-install` 安装成功；本轮验证的是 debug 安装，未验证 release 安装或远程 Git 下载。

用该安装目录的 executable 运行同一 Pointer 桌面回归（临时启动 wrapper 只替换 MCP executable 路径，未修改永久脚本），普通目录为 47 工具；cursor 前后 `(1032, -813)`，foreground 未变、两窗口未获焦点、raw 五通道零增长，点击/多击/拖放/滚动/取消/UUID/窗口销毁通过。截图已查看，idle 0.5 秒 updates 从 472 增至 477、activity 为零，App/MCP/15830 端口清理通过。证据位于本地忽略目录 `target/pointer-stage06/installed-pointer/`；安装 binary 与摘要记录的 debug binary SHA256 相同。此证据采集于 06 提交前，摘要中的 revision 是当时 HEAD，版本修改属于当时 working tree；不可把该字段解释成最终固定提交。消费方使用最终交接的完整 commit，且 runtime、Extras 和 MCP 必须来自同一 revision。Native Window、Widgetry、硬件多 DPI 的未测边界不变。

多窗口 keyboard 回归使用 `keyboard_windows` 宿主和本地 MCP stdio，验证可选 `window` schema、secondary 的文本与 press/release、默认 PrimaryWindow、无效和已销毁 target 错误，以及关闭窗口时中止长 typing/hold 并清理 Ctrl/Shift。截图和结构化状态分别记录在输出目录，fixture 只展示事件路由，不代表具体 Widget 的 focus 或编辑行为验收。需要 Python 3 和上述 Windows 桌面条件：

```powershell
cargo build --workspace --locked
cargo build --workspace --example keyboard_windows --locked
python tests/keyboard-window-regression.py --port 15816 --output target/keyboard-window-evidence
```

输出目录必须尚不存在，端口必须空闲。脚本只关闭本轮启动的应用，并检查应用、MCP 进程与端口退出。失败时保留交互日志用于诊断。

`tests/test-duplicate-a` 与 `tests/test-duplicate-b` 保留跨 package 同名 target 的发现/消歧场景；`tests/test-app` 的同名 bin/example 保护 target kind 选择，`extras_plugin/screenshot_fixtures.rs` 提供截图边界和确定性图像。它们是供 MCP 启动的宿主，不因 `cargo test --workspace` 运行而自动完成协议验证。runtime 的 mailbox/deadline 与持续动作推进、extras 的 agent tool 注册、输入和截图边界仍由各 package 的测试覆盖。

本入口重放方案 01 中已证实的场景。历史回放的输入只检查排队结果；最终 UI 文本与普通 Pointer 行为分别由上述 keyboard / pointer 专门入口验收。其他手势、资源 CRUD、entity 创建/销毁/重设父级、事件与 type guide 的完整真实调用仍未覆盖。MCP 自诊断在该重放中检查调用成功，未将 trace 文件内容或 token 用量设为 contract。原始 before 数据与当时的两个 Windows build-freshness 测试失败记录在[基线记录](../plans/refactor/01-baseline.md)；当前测试结果须重新运行判定，不能沿用旧结论。

## Bevy 0.20 升级：阶段 01 的旧版基线

2026-10-09 在干净的 `349cc44838ebe820749f6a046b69db9cb101e82a` 上采集。与方案指定的
`ae9fdaec25cb1f02b501f6e07199a27df144501e` 相比仅新增升级方案文档，工程源码一致。
生产 package 为 `0.3.1`，Bevy 为 `0.19.1`；本阶段没有修改依赖或 lockfile。
Cargo.lock 的 SHA256 为 `09d06215847df76d77e09f844b1a274e9d3ac482e981432ced62e1e5bc108c83`。
环境为 Windows `x86_64-pc-windows-msvc`、Rust/Cargo 1.99.0、PowerShell 7.6.6、Python 3.14.3；
活动工具链已安装 rustfmt 和 Clippy。

以下命令退出码均为 0，完整输出与依赖树保存在本地忽略目录 `target/bevy020-stage01/`：

```powershell
cargo check --workspace --all-targets --locked
cargo test --workspace --locked --no-run
cargo build --workspace --examples --locked
cargo test --workspace --locked --no-fail-fast
cargo check -p bevy_brp_extras --locked --no-default-features
cargo check -p bevy_brp_extras --locked --no-default-features --features diagnostics
cargo check -p bevy_brp_extras --locked --no-default-features --features ui
cargo check -p bevy_brp_runtime --locked
cargo check -p bevy_brp_mcp --locked --no-default-features
cargo build --workspace --locked
```

自动化测试为 266 passed、0 failed，保留 9 个既有 ignored doctest 和同名 fixture 产物 warning。
本轮未发现旧版测试失败；早期记录中的失败不替代本轮结果。

普通 MCP 完整工具 schema 为 47 项，诊断构建为 49 项，分别保存于
`catalog-ordinary/` 与 `catalog-diagnostic/`；前者还保存 `rpc.discover` 和过滤到输入、UI、
渲染、Transform 的 live `registry.schema`。摘要记录实际 MCP binary SHA256，两个模式均重新构建。
图形/协议基线同样来自本轮运行：

- Pointer：`pointer/`，端口 15922；真实按钮、多击、拖放、Line/Pixel 滚动、取消、UUID 复用和
  窗口销毁通过。OS cursor 前后均为 `(446, 584)`，foreground PID 不变，两窗口未获焦点，
  五类 raw mouse/cursor 计数零增长。0.5 秒 idle 查询间 updates 为 234 → 238，activity 为零；
  两窗口截图已查看。
- Keyboard：`keyboard/`，端口 15924；多窗口路由、默认窗口、非法/销毁窗口以及长操作的修饰键
  清理通过，两窗口截图已查看。
- Runtime：`runtime-port.log`；代码配置 Main=15752 可发现 40 个 method，默认 Main 未监听，
  正常 shutdown 退出码为 0。
- 诊断回放：`replay/`，端口 15928；握手、Transform 修改、watch、输入排队、日志 marker、
  两次 shutdown 和 `NatesList` 的 64×48 尺寸/关键像素检查通过，实际图像已查看。

各入口均完成正常关闭与清理；运行后另行确认本轮 MCP/App 进程和测试端口、Render 15703 均已释放。
所有上述结果仅证明旧 `0.19.1` 对照可用，不证明 `0.20.0` 已验收。Native Window 操作、
Widgetry 具体控件、硬件多 DPI 和其他平台未测，未运行全量 Clippy；这些范围不计作通过。

## Bevy 0.20 升级：阶段 02 的编译适配

workspace 的 Bevy、bevy_mesh、bevy_remote、bevy_winit 已统一为 `0.20.0`，内部 package
版本仍为 `0.3.1`。锁文件 SHA256 为
`da1443fef4c09d666fe4fcd63024fbd247b0bba68009a0dc8165a161974dd930`；
本轮仅重新解析 Bevy 及其传递依赖，image、uuid、rmcp 的既有精确约束保持不变。

源码已适配独立 Pointer 事件、PointerPressState、shape 和 render::view 的路径，以及
CalculatedClip 的多仿射矩形。UI 截图先以节点四边形与全部 clip 半平面求交，再计算有面积
区域的矩形 bounds；padding 后再次限制到同一可见域。空 clip、不受限轴、旋转 clip 和
多个祖先 clip 的定向测试通过。本阶段不替代后续截图像素或 Pointer 桌面验收。

`target/bevy020-stage02/` 保存依赖差异和命令输出。fmt、workspace all-targets check、
test --no-run、examples build、workspace test、Clippy 均通过；Extras 无默认 feature、
diagnostics、ui，runtime 独立消费及 MCP 无默认 feature、mcp-debug 检查也通过。
自动化测试为 270 passed、0 failed、9 ignored doctest，Pointer 定向测试为 34 passed。
保留同名 fixture 产物与旧 Interaction fixture 的弃用 warning。真实 Pointer、截图及
runtime/MCP `0.20.0` 协议回归将在后续阶段采集，本阶段不计为已通过。

## Bevy 0.20 升级：阶段 03 的 Pointer 回归

在 `27eba2692d7b2f13154d931bdce047259e79d9ea` 的生产源码上运行，working tree
仅增加测试覆盖：移除 PointerLocation、PointerPressState 或 PointerInteraction 都清理旧
generation 并保留 UUID；原 press 目标销毁后不向失效 entity 补发 Cancel。对照 `v0.20.0`
Picking 装配确认 First 输入生产、receive 后物理抑制、backend 后 Out 恢复、Hover 后收尾
的顺序仍成立。无默认 feature 的 Pointer 定向测试 35 passed，stock Button example 的
取消/再次激活测试通过；全仓 all-targets check、test --no-run 和 examples build 通过。

`python tests/pointer-regression.py --port 15932 --output target/bevy020-stage03/pointer`
通过，实际两窗口图像已查看。按钮、多击、拖放、Line/Pixel 滚动、双消费面取消、UUID
复用、窗口销毁和错误上下文通过；六个鼠标工具的完整 schema 与阶段 01 普通构建一致。
OS cursor 前后 `(144, 721)`，foreground PID 未变，两窗口未获焦点，raw 五通道零增长。
空闲 0.5 秒 updates 为 232 → 236、activity 为零；App/MCP/15932 正常清理。
本轮逻辑 DPI 覆盖来自自动化窗口尺度测试，未验证真实硬件多 DPI 或 Native Window。
