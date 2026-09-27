# 06 · Workspace 与 crate 目标架构决策

状态：**待用户确认；尚未实施**。本文件是方案 06 的设计交付，依据当前源码、Cargo metadata、rustdoc、MCP help text、测试宿主以及只读的直接消费者配置。确认后才进入方案 07；这里不改变已实施事实。

## 结论与目录

保留四个生产 crate，将其完整目录移入 `crates/`。三个独立的集成测试宿主 package 整体移入根目录的 `tests/`。不新增 crate，不改 package、binary、feature、发布状态或 MCP/BRP 接口。

```text
bevy_brp/
├── Cargo.toml                 # virtual workspace；更新 members 和内部 path dependency
├── Cargo.lock
├── AGENTS.md
├── rules/
├── crates/
│   ├── mcp/                   # bevy_brp_mcp；bin: bevy_brp_mcp
│   │   ├── src/
│   │   └── help_text/
│   ├── mcp_macros/            # bevy_brp_mcp_macros；proc-macro library
│   ├── extras/                # bevy_brp_extras；library、examples、tests/
│   └── runtime/               # bevy_brp_runtime；library、UPSTREAM.md、licenses
├── tests/                     # 容纳实际 Cargo package，不是根 package 的 test harness
│   ├── test-app/              # bevy_brp_test_apps；bin、examples、screenshot fixture
│   ├── test-duplicate-a/      # test-app-a；examples
│   └── test-duplicate-b/      # test-app-b；example
└── plans/
```

上图只突出参与本次迁移的目录；四个生产 crate 和三个测试宿主的其余源码、manifest、license、example 与 fixture 随各自目录原样移动。根 workspace 是 virtual manifest，`tests/` 只是归类目录；实际 Cargo 测试与运行入口仍由其下三个 package 提供。

## Crate 去留、职责与依赖

| 当前 crate → 目标路径 | 决定与 Cargo 身份 | 职责、依赖方向与对外入口 |
| --- | --- | --- |
| `mcp/` → `crates/mcp/` | 保留 `bevy_brp_mcp`，bin 仍为 `bevy_brp_mcp` | Agent 侧 stdio MCP 服务。内部模块继续负责工具注册、BRP 客户端、HTTP、App 发现与进程管理；只依赖内部 `bevy_brp_mcp_macros`，不依赖 App 侧 crate。对外是可安装二进制及现有 MCP tool/schema/help text，不提供 Rust library API。 |
| `mcp_macros/` → `crates/mcp_macros/` | 保留 `bevy_brp_mcp_macros`，仍为 `proc-macro` crate | 为 MCP 内部生成工具注册、描述、参数和结果映射；保持现有五个 derive：`ToolDescription`、`BrpTools`、`ParamStruct`、`ResultStruct`、`ToolFn`。独立编译目标是 Rust proc macro 的实际边界；改成普通模块无法替代 derive。删去它需要手写各工具的 metadata、handler/参数提取、结果包装和 help text 关联，重复与遗漏风险高，当前没有这种收益。宏的公开派生名称是编译期入口，但当前直接消费者只有 MCP crate，不扩大为面向其他 crate 的设计承诺。 |
| `extras/` → `crates/extras/` | 保留 `bevy_brp_extras` library | Bevy App 侧 BRP 扩展方法、输入、截图、关闭、诊断及独立 HTTP 插件配置。对外入口维持 `BrpExtrasPlugin`、`AgentTool`、`AppAgentToolExt`、端口/transport 配置类型及 `BrpExtrasActivity`/`BrpExtrasActivityState` 等当前 `lib.rs` re-export。只依赖 Bevy/`bevy_remote` 等外部库，不反向依赖 runtime。 |
| `runtime/` → `crates/runtime/` | 保留 `bevy_brp_runtime` library | Bevy App 侧 wake-aware HTTP transport 与进度推进。依赖 `bevy_brp_extras`；对外只暴露 `BrpRuntimePlugin`。插件组合 `BrpExtrasPlugin::without_http_transport()` 与自身 HTTP transport，避免装入两套 HTTP transport。`UPSTREAM.md` 和 license 随 crate 移动。 |

生产依赖图：`bevy_brp_mcp → bevy_brp_mcp_macros`；`bevy_brp_runtime → bevy_brp_extras → Bevy / bevy_remote`。两条链之间没有内部 Cargo 依赖；MCP 通过 BRP 协议与 App 通信。模块边界继续在现有 crate 内表达，不把 MCP 的工具框架、客户端或 HTTP 通信另拆成 crate。

`extras` 能单独安装默认/自定义 HTTP 插件，`runtime` 则提供依赖 `bevy_winit` 的 wake-aware transport；测试宿主也分别直接消费两者。合并会让只需扩展方法的 App 承担 runtime 的传输依赖，并改变现有 package 依赖和插件入口。保留两者的跨 crate 活动状态 `pub` 出口是当前组合所需；不借迁移批量收窄或扩大现有 Rust 公共 API。MCP binary 内 `tool` 等模块的 re-export 只服务本 binary，不视为外部 Rust API。新实现或后续触及这些模块时仍按 private → `pub(crate)` → `pub` 选择最小 visibility。

## 旧位置到新位置

| 现位置 | 目标位置/动作 |
| --- | --- |
| `mcp/**`，含 `src/`、`help_text/`、license | `crates/mcp/**`，目录内容随迁移 |
| `mcp_macros/**` | `crates/mcp_macros/**` |
| `extras/**`，含 `examples/`、`tests/`、license | `crates/extras/**` |
| `runtime/**`，含 `UPSTREAM.md`、license | `crates/runtime/**` |
| 根 `Cargo.toml` 的四个 production `members` 和三个 `[workspace.dependencies]` 内部 `path` | 改为 `crates/mcp`、`crates/mcp_macros`、`crates/extras`、`crates/runtime` 对应路径；成员自己的 `workspace = true` 用法保持 |
| `test-app/**` | `tests/test-app/**`；package 与所有 target 保持，workspace member 改为新路径 |
| `test-duplicate-a/**` | `tests/test-duplicate-a/**`；package 与所有 target 保持，workspace member 改为新路径 |
| `test-duplicate-b/**` | `tests/test-duplicate-b/**`；package 与所有 target 保持，workspace member 改为新路径 |
| 根 `Cargo.lock`、根配置、`rules/` | 原路径保留；迁移后检查 lockfile 是否有必要的 Cargo 更新，不做无关依赖升级 |

源文件中若存在从 workspace 根构造的旧路径、`include_str!` 相对路径或测试 fixture 路径，方案 07 逐处核对并只修正真正受移动影响的引用。`mcp/help_text/` 和 `runtime/UPSTREAM.md` 在移动后按新位置更新事实性引用；无需恢复已删除的 README/CHANGELOG，也不创建空架构事实文档。

## 测试宿主与运行入口

| 位置、package | 保留的 target / fixture | 作用与运行方式 |
| --- | --- | --- |
| `tests/test-app/`，`bevy_brp_test_apps` | bin `test_app`；examples `extras_plugin`、`no_extras_plugin`、`event_test`、`mouse_test`、`test_app`；`examples/extras_plugin/screenshot_fixtures.rs` | runtime、extras、stock BRP、事件、输入、截图的真实宿主；bin 与 example 同名保护 `kind` 区分。`cargo build -p bevy_brp_test_apps --all-targets --locked`；运行时用 `cargo run -p bevy_brp_test_apps --bin test_app -- ...` 或 `--example <name> -- ...`，再走 MCP→BRP 测试链。 |
| `tests/test-duplicate-a/`，`test-app-a` | examples `extras_plugin_duplicate`、`test_app` | 与 B 的同名 example、与 `test-app` 的跨 package 同名 target，保护 `path`、`package_name`、`search_order` 消歧。`cargo build -p test-app-a --examples --locked`，协议链按该 package/target 启动。 |
| `tests/test-duplicate-b/`，`test-app-b` | example `extras_plugin_duplicate` | 与 A 的同名 example 配对。`cargo build -p test-app-b --examples --locked`，协议链按该 package/target 启动。 |
| `crates/extras/tests/`，`bevy_brp_extras` | `agent_tool_registration` integration test | 由该 library package 的 test harness 执行：`cargo test -p bevy_brp_extras --test agent_tool_registration --locked`。 |

原有 module unit tests 仍由各自 package 的 `cargo test -p <package> --locked` 运行。保留测试宿主的 Bevy full-feature 配置，不将其传播到生产 crate。`tests/` 下的 bin/example 不会仅因放进该目录就由 `cargo test` 自动执行 MCP→BRP 链；上述 `cargo run` 是真实宿主入口形状，具体端口与参数以现有 fixture 为准，运行时验证使用独立测试端口并清理进程。

## 直接消费者与兼容范围

| 消费点 | 影响和处理决定 |
| --- | --- |
| 本仓库内部 | root workspace 七个 `members`/内部 path 同步；各成员的 `workspace = true` 依赖、package 名、binary 名和测试 target 保持。所有 source、fixture、脚本、help text 中对旧物理路径的引用按实际受影响范围调整。 |
| 相邻 `bevy_widgetry` | 当前 `Cargo.toml` 以 Git `tag = "v0.1.0"` 消费 `bevy_brp_runtime`，另有按同一 tag 安装 MCP 的命令注释；本任务只读，不编辑该仓库。旧 tag 内容不受本地目录移动影响。未来若消费者改用含新布局的 Git ref，应在方案 07 用隔离的临时 Git 消费项目验证 package 解析及 `cargo install bevy_brp_mcp --git ... --rev ...` 路径。不能以本地 workspace 编译代替该验证。 |
| 其他 Git/package-name 消费者 | 目标保留 package、binary、公共 Rust API、MCP wire 名称及 BRP 方法；不变更 tag、URL、版本、发布方式。新 ref 下按 package 名解析需由 Git-source 验证确认。未知外部消费者无法在本仓库枚举，故不宣称已经验证。 |
| 指向旧 manifest 的路径消费者 | `path = ".../runtime"`、`.../extras`、`.../mcp_macros` 等必须改为相应 `.../crates/<crate>`。不保留旧目录 shim：它需要重复 package、额外转发 crate 或维护旧路径结构，当前没有确认的直接路径消费者。迁移说明明确这一处一次性路径变更。 |
| MCP App 发现/launch 的路径使用者 | `brp_list_bevy` 的 `relative_path` 由真实 manifest 目录计算；`brp_list_bevy`/`brp_launch` 的 `path` 是真实 OS 目录过滤。从仓库根搜索时，`extras` 示例的相对目录变为 `crates/extras`，三个测试宿主分别变为 `tests/test-app`、`tests/test-duplicate-a`、`tests/test-duplicate-b`；旧 `path=extras` 或 `path=test-duplicate-a` 等不再指向目标。调用方需改用新目录；`package_name`、`kind` 和 `search_order` 的语义保持。两个同名 `extras_plugin_duplicate` target 的消歧仍须分别用新物理路径验证。保留真实路径语义，不添加旧路径别名。 |

物理路径变化是本方案需要用户确认的兼容代价；不得将其描述为完全无行为变化。除此以外，方案 07 不改 tool 名、参数、JSON shape、error、BRP method、transport 语义或插件使用方式。若实施发现无法在这些边界内完成，应回到决策关口，而非顺手改变公共 contract。

## 方案 07 施工与验收

1. 移动四个完整生产 crate 到 `crates/`，移动三个完整测试宿主 package 到 `tests/`，更新根 workspace 七个 member 路径和内部 dependency path；定位并修正实际失效的相对引用，保持 package/target/feature/插件装配不变。
2. 同步受路径变化影响的已实施事实载体：公共 rustdoc、MCP help text、`crates/runtime/UPSTREAM.md`、根规则中的旧路径示例及真实消费说明。只描述已完成的迁移。
3. 用 `cargo metadata --format-version 1 --no-deps --locked` 核对七个 package、manifest 路径与所有 target；核对 `Cargo.lock` 没有意外版本变化。运行 `cargo fmt --all -- --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked`、`cargo build --workspace --locked`、`cargo test --workspace --locked --no-fail-fast`，以及 `AGENTS.md` 所列四个独立 feature 检查；针对 extras integration test 和三个宿主构建/运行入口做定向核对。
4. 用独立端口执行与迁移有关的真实 stdio MCP→BRP 链：握手与 tool listing、从 workspace 根发现并按 package/path 启动、位于 `tests/` 的重复 target 消歧、至少一个 runtime 宿主 BRP 调用、正常 shutdown 和端口/进程清理。检查所有被移动宿主的 `relative_path` 新值及旧/新 `path` 过滤结果；若覆盖截图，检查实际图像。用隔离临时项目验证新 Git ref 的 runtime 依赖和 MCP 安装，不修改 Widgetry 或 Git tag。
5. 将既有失败、新增失败和未运行项分开报告。方案 01 基线记录的 Windows build-freshness 两个失败（`returns_fresh_when_binary_is_newer_than_inputs`、`returns_stale_when_dependency_is_newer_than_binary`）是历史记录，不预判本次测试结果；基线回放中截图预期与实际不一致，运行时验收以本次固定参数和实际结果判定。工程行为改动完成后依 `AGENTS.md` 用全新 reviewer subagent 执行独立 Code Review，修到无 findings。

确认本设计即确认：**四个生产 crate 全部保留并移入 `crates/`；三个测试宿主整体移入根目录的 `tests/`；接受旧物理路径消费者更新路径；其余公共入口保持原状。** 方案 06 到此结束，等待用户确认后再实施。
