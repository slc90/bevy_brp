# 17 · 最终工程验收记录

记录时间：2026-09-28（Asia/Shanghai）。本次在本地 checkout 执行，Rust/Cargo 1.98.1、Windows MSVC、PowerShell 7；没有提交、打 tag、推送或发布。

## 收口结果

- 新增根 `README.md`，给出当前 App 插件接入、stdio MCP 构建与客户端启动、默认/诊断功能、开发验证入口，并说明现有 `v0.1.0` tag 仍指向旧版本。
- 修正 `docs/testing.md` 的图形回归构建命令：它调用方案 01 固定重放器中的两项 server trace 工具，需要 `mcp-debug`。另明确普通构建的公共协议回归入口与构建切换顺序。
- 补齐 runtime 双 HTTP transport 测试宿主的 `AssetPlugin`，使测试在检查预期冲突之前完成所需 Bevy 初始化。公共 runtime 行为未变。
- 公共 MCP 回归脚本在 `brp_launch` 后等待 App 的监听端口。此前一次真实运行中 `brp_launch` 已返回，但 App 尚未完成图形初始化，catalog 请求因连接时序失败；补齐等待后同一协议链通过。
- 修复 Windows Cargo dep-info 解析：只把后接空白的冒号识别为规则分隔符，并在 Windows 路径中保留非转义用途的反斜杠。新增盘符与转义空格的回归测试；方案 01 遗留的两项 freshness 测试现已通过。其他平台保留原有反斜杠转义行为。
- 根 Cargo metadata、统一依赖、lint、crate 角色、公共入口、`docs/architecture.md`、`docs/mcp.md`、许可证与 `crates/runtime/UPSTREAM.md` 经核对未发现本项需要修改的失效事实。仓库没有 `.github/workflows`；保留本地可重复命令，不创建新 CI 基础设施。没有证实可删除的兼容代码或配置，本项未删除。

## 本轮验证

| 验证 | 结果 |
| --- | --- |
| `cargo fmt --all -- --check` | 通过。 |
| `cargo check --workspace --locked` | 通过。 |
| `cargo clippy --workspace --all-targets --locked` | 通过。 |
| `cargo build --workspace --locked` | 通过，普通 MCP binary 已构建。 |
| 四项独立 feature 检查 | `bevy_brp_extras` 的 `--no-default-features`、`diagnostics`、`ui`，以及 `bevy_brp_mcp --no-default-features` 均通过；命令见 `AGENTS.md`。 |
| `cargo test --workspace --locked --no-fail-fast` | 通过；runtime 13/13、extras 80/80 加 1 项 integration、MCP 118/118，以及可执行的 doctest 均通过。Cargo 的同名 example 输出文件 warning 来自保留的歧义 fixture。 |
| 普通 MCP 公共协议链 | `cargo build --workspace --locked` 后执行 `tests/public-mcp-regression.ps1 -Port 15742` 通过；覆盖 tool catalog、动态 method、结构化错误与 watch 拒绝。首次运行因 App 未开始监听而失败，补齐脚本等待后复跑通过。 |
| 普通/诊断 registry 与 trace 隔离 | 对应构建下 `tests/diagnostics-regression.ps1` 均通过；普通 47 tools，`-DebugBuild` 为 49 tools。 |
| 图形 MCP→BRP 重放 | `cargo build -p bevy_brp_mcp -p bevy_brp_test_apps --locked --features bevy_brp_mcp/mcp-debug` 后运行 `tests/regression.ps1 -Port 15712` 通过。脚本检查握手、发现、读写、watch、输入排队、日志、截图像素、clean shutdown、进程/端口清理，成功后删除临时产物。 |

两项 Windows freshness 测试在本项初次完整测试时仍失败；新增 regression 后先确认旧解析器失败，修复后定向测试与完整 workspace 测试均通过。图形重放中的最小化 `test_app` 主窗口截图错误是方案 01 固定的预期场景，离屏截图按像素验证成功。当前机器的可见桌面与图形设备足以运行该链路；没有把它外推为其他客户端或平台的验收。

## 已知问题与未测项

- 固定真实链路仅验证输入排队，未断言最终 UI 文本；其他键盘、鼠标、手势、资源 CRUD、entity 操作、事件和 type guide 的完整 GUI 交互未在本项逐一重放。严格外部 JSON Schema 客户端、其他 OS、发布包与托管 CI runner 未测。
- 真实模型客户端的输入/缓存/输出 token、模型轮次和费用未采集。方案 16 的原始 MCP wire 对照可复现：`tools/list` 从方案 15 的 109,281 bytes 降到 A/B 后的 86,650 bytes（同一固定宿主 47 tools）；Gallery 的同一 10 次 tool call 响应从 C 前的 11,345 bytes 降到 C 后的 4,875 bytes。D 的 Gallery 计数代理实测 6 次 `brp_execute` 调用发出 7 次 BRP 请求，详情见[方案 16 结果](16-efficiency-results.md)。这些是各专项样本，不是 token 节省比例，也没有把动态响应字节差当成固定收益。本项没有新效率改动，因此未生成新的 before/after 样本。

## 消费者与交接

方案 01 的功能清单中，MCP 启动/枚举/调用、BRP 发现与读写、App 启动/状态/关闭、watch、日志、截图、输入排队及 runtime 推进，在本项的 Cargo 测试、普通公共协议链或图形重放中保留相应验证；原始未测的完整输入效果与其余工具仍按未测列出。迁移后的当前仓库路径和公共 Rust 入口见 `README.md` 与 `docs/architecture.md`；默认隐藏的 server trace 工具可通过显式诊断构建使用。方案批准删除的旧目录已由此前阶段迁移；本项没有新增删除或公共契约变更。

仓库外 `bevy_widgetry/Cargo.toml` 当前仍以 Git tag `v0.1.0` 消费 `bevy_brp_runtime`，其 lockfile 固定旧提交 `60aa2d4b…`；该 tag 与其 MCP 安装注释均不会自动取得本 checkout 的新实现。若消费者要使用本轮改造，需另行更新其 Git 引用、lockfile 和 MCP binary 安装来源，并验证 Gallery；本项按只读边界未修改它。直接使用旧本地 manifest 目录的其他消费者需改为 `crates/<name>` 路径，未知消费者无法穷举。

本项交接为：已具备当前 README、可重复本地验证入口、通过的完整 workspace test 和代表性真实链路。独立 Code Review 结论另在本任务交接中报告。
