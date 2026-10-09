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

## Bevy 0.20 升级：阶段 04 的 UI 截图回归

UI 几何定向测试 20 passed，新增多重旋转、AABB 假交集、交叉条带、亚像素面积、线/点
相切、镜像/非均匀缩放、奇异/非有限变换和旋转无界轴。OverrideClip、FixedNode 和
Display::None 用例运行引擎 update_clipping_system 后消费实际 CalculatedClip。
旧 `URect::new(35, 38, 46, 45)`、viewport 偏移、物理尺寸和 live target 边界保持。
Extras 测试、workspace all-targets check、test --no-run、examples build、Clippy 及
Extras 无默认 feature/ui 独立检查通过，完整输出在 `target/bevy020-stage04/`。

新增可重复图像入口（需要 Pillow；输出目录须不存在）：

```powershell
cargo build --workspace --example extras_plugin --locked
python tests/screenshot-regression.py --port 15934 --output target/screenshot-evidence
```

本轮 `screenshots-final/` 通过，五张实际图像已查看。NatesList 保持 64×48 蓝底和黄/品红
marker；旋转 UI 为 32×56 青色；两层引擎 clip 的嵌套 UI 为 40×40，绿色中心、黑色角落；
2D/3D AABB 分别为 12×60 红色和 12×48 绿色，黄色 marker 正确。live get 确认嵌套
CalculatedClip 含两项。hidden/partial UI、无 bounds、重名和错误 camera 被拒绝，不发布 PNG。
截图是合成 target 的矩形 crop，不是节点独占像素或 alpha mask。

首次 shader 尚未完成时图像可能是清屏色，入口在有界时间内重捕获并核对同一精确像素，
不放宽颜色或几何断言。额外暴露的既有 fixture 问题是 2D Sprite 使用 NoCpuCulling 导致
不进入逐 view 的 VisibleEntities；局部改为 NoFrustumCulling 后红色主体恢复，crop 未变。
早期失败输出保留用于诊断，最终通过输出有 binary SHA256、采集时 HEAD（源码修改当时
在 working tree）、进程和端口清理记录。没有修改截图 transport 或 pending capture contract。

## Bevy 0.20 升级：阶段 05 的 runtime/MCP 协议回归

固定上游 transport/schedule 接缝核对见 [runtime 来源说明](../crates/runtime/UPSTREAM.md)。
HTTP 与 Remote 装配未重写；wake、背压、双端点 lifecycle、30 秒 deadline、Cleanup 后
progress 和端口优先级保持。runtime 的 15 项测试以及全仓 279 passed、0 failed、9 ignored
通过；all-targets check、test --no-run、examples build、Clippy 与 runtime/MCP 两种
feature 独立检查通过。证据位于 `target/bevy020-stage05/`。

新增真实协议入口使用既有 event_test（非活动窗口、desktop_app 休眠模式）：

```powershell
cargo build -p bevy_brp_mcp --locked
cargo build --workspace --example event_test --locked
python tests/protocol-regression.py --port 15936 --output target/protocol-evidence
```

Main 与默认 Render=15703 均可发现/响应；并发 unknown→valid 请求后仍可继续处理。安全
fixture 的 component/resource insert、mutate、remove、query/get/list、名称查找、spawn、
reparent/despawn 均核对实际状态。实际 observer 收到 unit/payload event 并更新 tracker。
生成的 primitive、enum、Option、nested struct、list/map 示例可用；u64=9007199254740993
保持精度，缺失字段的 Default 与显式 null 的类型错误分别验证。Entity 使用本轮有效 ID；
Handle 的 Uuid 变体可写，Strong 变体有明确不可写理由，包含它的 struct 不生成整体示例。
计算态 CalculatedClip 仅核对 schema/读取，没有写入运行中的主场景。

Bevy 0.20 的 Resource 同时暴露 Component 反射，原优先级使 resource 指导退化为 spawn。
局部改为 Resource 优先，保留 resource insert 示例；独立 Red→Green 与真实插入/读回通过。
live registry 确认 Tonemapping/DebandDither 为 bevy_render::view 路径，静态知识未存旧路径。
普通 47/诊断 49 的完整工具 schema 与阶段 01 对照，仅 rpc_discover 描述中的引擎版本
由 0.19 更新为 0.20；工具名、参数、required/optional、annotation 和注册边界未变。

watch 记录实际两次 component 更新后停止，无残留 MCP watch；SSE 保留字符串 request id，
关闭 stream 后 watcher 调用停止。普通 HTTP 客户端中断与真实 30 秒 timeout 都释放隐式
Watching 请求，随后调用计数停止增长、更新回落、activity 为零且窗口未获焦点。
分别占用 Main/Render 端口的 bind 失败使另一 listener 释放、App 退出码为 1；日志确认
对应 endpoint 的 bind failure。event_test 现在把 AppExit 返回给进程，避免 fixture 丢弃
失败码。正常回归要求 shutdown_method=clean_shutdown，并核对 App/MCP 和两个端口退出。
停止 watch 与正常 shutdown 的清理互相独立；故障注入同时使两者抛错后，仍核实并清理
本轮 event_test PID、MCP 和双端口，保留最初的 watch 错误及各清理错误记录。正常路径
另行复跑通过，未触发强制清理，证据见 protocol-cleanup-grace 与 protocol-fault-cleanup-grace。
独立消费者、安装版 MCP、最终桌面与历史回放仍由阶段 06 复验；不以此替代外部 Widgetry。

## Bevy 0.20 升级：阶段 06 最终验收与交接

2026-10-09 在 Windows 10 19045、PowerShell 7.6.6、Python 3.14.3、Rust/Cargo 1.99.0
和 NVIDIA GeForce RTX 4070 Ti SUPER（Vulkan，595.79）上完成最终回归。功能代码固定为
`ad87f2334c11d99b80ba8a04931c28aa51ad8b09`，Bevy 为 0.20.0；采证时 working tree
包含 README 与 Extras/runtime crate rustdoc 的文档修改，随后补充本节和宿主前提说明。
没有修改功能代码、manifest 或 lockfile；不能将本节所在的未提交文档冒充已经提交的 revision。
workspace Cargo.lock SHA256 为
`da1443fef4c09d666fe4fcd63024fbd247b0bba68009a0dc8165a161974dd930`。

证据位于本地忽略目录 `target/bevy020-stage06/`。`environment.json`、
`source-at-validation.patch` 记录首轮环境与采证 diff；`cargo-results.json`、
`desktop-results.json` 保存实际命令、退出码和耗时，桌面记录另含运行时 MCP SHA256。
各链的 `summary.json`、交互记录、App/MCP 日志和 PNG 保留对应 PID、port、binary 标识
及清理结果。中断的 `keyboard/` 保留，最终成功结果使用新的 `keyboard-resumed/`。

### 编译与 feature

最终功能状态的 fmt、workspace all-targets check、all-targets Clippy、完整 test、test
--no-run、workspace build 和 examples build 全部退出 0；完整测试为 279 passed、
0 failed、9 ignored。Extras 无默认 feature、独立 diagnostics、独立 ui、runtime，
MCP 无默认 feature和 mcp-debug 的独立 check 均退出 0。Pointer example build/test、
Keyboard example build 和 runtime_custom_port example build 也通过。

最终 rustdoc 修改后复跑 fmt、workspace all-targets check、test --no-run、Extras/runtime
doctest、cargo doc --no-deps 与 diff --check，均退出 0，见 `final-results.json`。
仅补充说明文字，没有改变行为或 feature；既有桌面证据继续对应同一功能 revision。

既有同名 example 输出冲突 warning 保留；extras_plugin 使用的 Interaction alias 在
0.20 被弃用，fixture 的两条 warning 保留，不影响测试结果。9 个 ignored doctest
未增加。历史两项 Windows dep-info freshness 测试本轮通过，没有沿用旧失败结论。

### 桌面、图像与协议链

| 链与证据子目录 | 实际端口 | 结果与边界 |
| --- | --- | --- |
| `pointer/` | 15950 | 实际 Button、多击、拖放、Line/Pixel 滚动、取消、UUID、窗口销毁和 idle activity 通过。 |
| `keyboard-resumed/` | 15952 | secondary 路由、默认 PrimaryWindow、无效/销毁窗口、长 typing/hold 期间关闭窗口、Ctrl/Shift 清理通过。 |
| `screenshots/` | 15954 | 五张 crop 的精确尺寸、颜色和 marker，以及 hidden/partial UI、camera、重名和 bounds 错误通过。 |
| `protocol/` | 15956、15703 | Main/Render、反射 CRUD/type-guide、observer、watch、SSE、取消、实际 30 秒 deadline、分别占用双端口的失败清理通过。 |
| public MCP、普通 diagnostics | 15958、15960 | 普通 47 工具、trace 工具不可调用、App 日志与 trace 读取边界及清理通过。 |
| runtime-port | 15752、15702、15703 | 自定义 Main port、默认 Main 未监听、Render discovery、shutdown 通过。 |
| debug diagnostics、`replay/` | 15962、15964 | 诊断 49 工具、握手、Sprite Transform 0→42、watch、输入接受、精确截图错误、日志 marker、两次 clean shutdown 和 NatesList 像素通过。 |
| `catalog-ordinary/`、`catalog-diagnostic/` | 15961、15965 | 与阶段 01 的完整 schema 对照通过，仅 rpc_discover 描述中的 Bevy 版本变化。 |

Pointer、Keyboard 的两窗口图像、五张 screenshot crop 和历史回放 NatesList 图像均已
实际查看。Pointer OS cursor 前后均为 `(63, 766)`，foreground PID 保持 17160，两个
宿主窗口未获焦点；motion/buttons/wheels/cursor/window_events 五个 raw 通道零增长。
空闲 0.5 秒 updates 为 234→238，activity 为零。Keyboard 图像分别显示
`PrimaryWindow: pOK`、`SecondaryWindow: aAZ!`，不外推 Widgetry 编辑或 focus 行为。
截图中 NatesList 为 64×48 蓝底/黄/品红 marker，旋转 UI 为 32×56 青色，嵌套 clip
为 40×40 绿中心/黑角落；2D/3D AABB 为 12×60 红色、12×48 绿色及黄色 marker。

满 mailbox 的提交前/后 wake 顺序由 runtime unit test 验证；真实协议链验证并发
unknown→valid 请求继续推进、双端点和取消后的休眠恢复，不能将它写成真实 HTTP 压满
mailbox 的压力测试。正常链均完成 shutdown、watch 停止、App/MCP 和端口清理。
历史回放恢复共享 trace 长度 636870 bytes；旧 before 样本未覆盖。

### 独立消费与临时安装

`consumer/` 和 `extras-only/` 各自声明独立 workspace，Cargo metadata 确认只有自身
一个成员、Bevy 精确为 0.20.0，并使用独立 lockfile；不依赖测试宿主的 feature 合并。
使用同一源码的 path dependency，offline --locked debug build，共用 target 编译缓存。
`consumer-metadata.json` 保存实际 feature、workspace root 及 manifest/lockfile 标识。

runtime 消费者在 DefaultPlugins 后安装 `BrpRuntimePlugin::with_port(15966)`，导入
activity/agent-tool 公共入口；安装版 MCP 的 47 工具和动态 catalog 可用，实际
`consumer/multiply(6, 7)` 得到 42，Resource 读回为 42，Render=15703 可调用。
Extras-only 使用 `default-features=false`、`MinimalPlugins`、InputPlugin 和无窗口的
WindowPlugin（DontExit），以 `BrpExtrasPlugin::with_port(15968)` 提供普通 BRP 与
agent catalog；不依赖 runtime，也不安装 Picking backend、Winit 或 GPU 插件。
首次临时宿主遗漏输入消息导致 panic；补充 WindowPlugin 后默认无窗口退出又使进程提前
结束，最终显式 DontExit 后通过。两次失败证据保留，未通过放宽断言或修改生产行为处理。

临时 MCP 的实际安装命令为：

```powershell
cargo install --path crates/mcp --locked --offline --debug --root target/bevy020-stage06/mcp-install --target-dir target
```

真实验证使用 `mcp-install/bin/bevy_brp_mcp.exe`，SHA256 为
`311c3cd21bdbde55a9a2e20ee07cdb74505028ec04daad556a0b9b987b83e0a8`，
没有用 workspace 中另一个 binary 替代。两个消费者均从该 MCP 调用 shutdown，App/MCP
退出码为 0，Main/Render 端口释放；最终证据为 `installed-consumer-pass/summary.json`。
`consumer-results.json` 保留首次失败，补齐宿主后的命令/退出码见
`consumer-resume-results.json`，不把初次失败记录覆盖成成功。
桌面普通构建 SHA256 为
`32bff357a2db413e8264b61d63eaa70a26ec36970ed3757bb41ec4fe20ab75c9`，
诊断构建为 `c4ce69a60e618faa827d9cc1dd1063aad2ce79030ec19e227e155abcf939c34b`；
两种构建都重新构建并采集 catalog。安装版的独立 feature 构建与 workspace feature
合并构建分别记录标识，不要求二进制 hash 相同。

### 交接与未测范围

README 提供上述完整 revision 的 runtime/Extras/MCP 消费说明及旧版回退路径。
现有 `v0.3.1` 的 commit 仍为 `ae9fdaec25cb1f02b501f6e07199a27df144501e`，对应
Bevy 0.19.1；没有创建或移动 tag、发布 package、push 或修改 bevy_widgetry。
本机原 MCP SHA256 保持
`1e89bef7bec10d5aa0da79ef5615488cd9436a947f51adbf5c261bebcd7d9c9a`。
回退须恢复整个旧 revision/Cargo.lock 及其匹配的 App 库和 MCP，不能只回退 manifest。

本轮验证的是本地 path、offline debug 安装与 Windows 桌面；未验证远程 Git 下载、
release 安装、其他平台、真实硬件多 DPI、native 窗口标题栏移动/缩放或 Widgetry 控件。
功能 revision 尚未 push，远程 Git 消费示例须在该 revision 可获取后使用。外部消费者
后续须统一升级 Bevy、App 库与 MCP revision，不能将 0.20 Plugin 混入 0.19 App。

## 0.4.0 版本发布验证

2026-10-09 在阶段 06 文档提交 `e07bf0cb` 之后，将四个生产 package 及 workspace
内部 dependency requirement 统一改为 0.4.0，Bevy 保持 0.20.0。Cargo.lock 只有四个
生产 package 的 version 变化，外部依赖和测试宿主版本没有改变；本轮 lock SHA256 为
`1216abec1241b14c262fc8c175ab9b1d1b69fa3a76022d9a5841fcf4ff0c7b1c`。
本节记录的是提交前的版本 diff；前面阶段 06 的未提交、未打 tag 和未 push 描述是
当时的采证状态，旧记录未改写为本次版本发布的结果。

证据保留在 `target/release-0.4.0/`，`results.json` 记录命令、退出码与耗时，
`source.patch` 记录采证时 HEAD 上的 diff。fmt、workspace all-targets check、
Clippy、完整 test、workspace build、examples build，以及 Extras 无默认 feature/
diagnostics/ui、runtime、MCP 普通/mcp-debug 独立检查均退出 0。测试仍为
279 passed、0 failed、9 ignored，既有 fixture warning 和 ignored doctest 保留。

重新构建 0.4.0 普通和诊断 MCP，完整 47/49 工具 schema 与阶段 06 完全相同。
`public-mcp-regression.ps1 -Port 15970` 验证动态 catalog、执行、错误结构与 watch
拒绝边界，随后确认宿主/MCP 与端口退出；catalog 采证端口为 15972 和 15974。
本次只改变版本和消费文档，Pointer/Keyboard/GPU 截图的全套桌面证据仍见阶段 06，
没有将它们宣称为改版本后的新跑次。

独立 runtime 和无默认 feature 的 Extras-only 消费者使用本轮源码的 path dependency
及各自 lockfile 重新构建。MCP 通过 local path、offline、debug 安装到独立 root
`target/release-0.4.0/mcp-install`；原安装不覆盖。实际安装版 SHA256 为
`7f1b72b7185dea10897cc5e1f471e41a5ae37962990fda1d2faee08fea207105`。
两个消费者的 47 工具、agent catalog、BRP discovery、shutdown 都通过；runtime 的
自定义 Main=15966 与 Render=15703 可调用，乘法 6×7=42 及 Resource 读回通过。
App/MCP 退出码为 0、端口释放，见 `installed-consumers/summary.json`。远程 Git
下载、release 安装及阶段 06 列出的硬件/平台项仍未由这些 local debug 结果证明。

0.4.0 的 Git 消费标识为 `v0.4.0`，runtime、直接导入的 Extras 与 MCP 使用同一 tag。
旧 `v0.3.1` 保留为 Bevy 0.19.1 回退组合；本次不发布 crates.io package，也不修改
外部 Widgetry 或本机原 MCP 安装。
