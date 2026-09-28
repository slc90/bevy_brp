# 15 · MCP 消耗归因与专项优化决定

记录时间：2026-09-28（Asia/Shanghai）。本项是设计关口；没有修改生产代码或公共契约。原始样本来自 [01-baseline.md](01-baseline.md)，当前样本为提交 `8d87467a35c2b8fa21125b2451d54b2acb2dd325` 的默认 feature 构建。当前完整 `tools/list`、逐次 MCP wire 和摘要保存在 [assets/15-efficiency/](assets/15-efficiency/)；原始记录保存在 [assets/01-baseline/](assets/01-baseline/)。

## 场景、环境与复现

- 两次都使用 Windows 10 x64、Rust/Cargo 1.98.1、debug profile、协议 `2025-11-25`、同一个 `bevy_brp_test_apps` 宿主和 `baseline` 输入。当前轮使用独立端口 15713；原始轮使用 15712。两个目标依次为 `test_app` 和 `extras_plugin`。
- 用户目标相同：发现目标和 BRP method，查询 Sprite，读取并修改 Transform，建立/停止 watch，发送文本，检查主窗口截图错误，读取应用日志，关闭运行时宿主；再查找 `NatesList`、保存离屏截图、发送文本、读日志并 clean shutdown。当前轮保留了所有这些验证动作。
- 原始轮另有 `brp_set_tracing_level` 和 `brp_get_trace_log_path` 两次 MCP 调用；方案 13 后默认构建不再提供它们。本轮从正常任务序列移除这两项服务端诊断动作。它们的消失不是任务操作效率的收益。`brp_list_bevy`、`rpc_discover`、watch 与两次截图调用均保留。
- 当前轮先执行 `cargo build -p bevy_brp_mcp --locked`，确保 `target/debug/bevy_brp_mcp.exe` 是默认 feature 构建。现成的可执行文件曾是 `mcp-debug` 构建，一次预试验因此仍列出 49 项；该预试验没有纳入下表。
- 复现当前样本：从仓库根目录执行下方 PowerShell 7 命令。临时副本只把原始脚本的仓库定位改为当前目录，并移除两次不属于普通用户任务的 trace 调用。脚本默认把输出写入新的系统临时目录，不覆盖版本库样本。样本把仓库路径和用户目录替换为 `<REPO>` 与 `<USER_HOME>`；wire byte 数取替换前 UTF-8 newline-delimited JSON（包含行结束符）。PID、时间戳、日志文件名、entity ID、端口和路径会使响应字节变化，不应逐字节比较总量。
- 另一次同条件的新进程重放用 `Measure-Command` 测得端到端 12,010 ms，包含约 3.5 s 的显式等待、应用启动、窗口恢复和截图。原始轮没有对应耗时记录，因此没有 before/after 耗时结论。两轮均为新 MCP 进程；真实客户端的冷启动与重复会话缓存行为未测。

```powershell
cargo build -p bevy_brp_mcp --locked
$scriptText = Get-Content plans/refactor/assets/01-baseline/replay.ps1 -Raw
$scriptText = $scriptText -replace '(?m)^\$repo = .*$', '$repo = (Resolve-Path ".").Path'
$scriptText = $scriptText -replace '(?m)^\s*\$traceLevel = Call-Tool "brp_set_tracing_level".*\r?\n', ''
$scriptText = $scriptText -replace '(?m)^\s*\$tracePath = Call-Tool "brp_get_trace_log_path".*\r?\n', ''
$tempScript = Join-Path $env:TEMP 'bevy-brp-efficiency-replay.ps1'
[IO.File]::WriteAllText($tempScript, $scriptText)
& $tempScript -Port 15713
```

## 两种对照的边界

| 观测量 | 原始基线 | 当前默认构建 | 解释 |
| --- | ---: | ---: | --- |
| `tools/list` 工具数 | 49 | 47 | 少的两项仅是服务端 trace 工具。 |
| `tools/list` 响应 wire bytes | 111,207 | 109,281 | 单次服务端目录输出少 1,926 bytes；不代表每轮模型输入。 |
| 有 id 的 MCP request | 29 | 27 | 各含 initialize 和 tools/list；工具调用为 27 → 25，差值正好是两次诊断调用。 |
| client→server wire bytes | 4,829 | 4,601 | 少 228 bytes，来自移除诊断调用。 |
| server→client wire bytes | 174,014 | 172,374 | 原始全序列少 1,640 bytes；动态字段和日志内容影响此值。 |
| 两次诊断响应从原始轮扣除后的 server→client bytes | 172,646 | 172,374 | 同一用户目标仅少 272 bytes；目录少 1,926 bytes，其余交互合计多约 1,654 bytes，不能据此宣称稳定节省。 |
| 任务错误 | 1 | 1 | 两轮都在最小化的 `test_app` 主窗口截图上返回 `Screenshot capture requires a primary window`，随后使用离屏 fixture 完成截图。 |

当前轮 `rpc_discover` 得到 39 个 method，watch 成功建立和显式结束；文本调用返回 queued 8，未验证 UI 最终文字。离屏 PNG 为 64×48、285 bytes，与原始样本 SHA-256 相同：`173338A581192CFC7FC4BD19D56D78844A1DC28BAC953D71B1649D1980916903`。两个宿主均返回 `clean_shutdown`；重放结束后端口 15713 无连接，宿主进程已退出。没有把已知的主窗口截图错误计为纠正失败，也没有因省略它来缩短序列。

当前到方案 16 的结果尚未发生。本节的任何差值只属于“原始基线→当前”；方案 16 应另用当前样本作专项 before，并在同一条件下收集新的 after，不能把整轮重构差值加到专项收益中。

## 目录与交互归因

按每个 tool 的 JSON 值单独作紧凑 UTF-8 计数，当前 47 项的 `outputSchema` 合计 48,645 bytes，`description` 33,021，`inputSchema` 16,499，`annotations` 5,273，名称 1,031，title 974。字段计数不包含对象键、分隔符和外层协议，因此不能相加当成 109,281-byte 的 wire 长度。原始 49 项对应的三项最大字段分别为 50,715、32,505、16,418 bytes。

当前全部 47 个 `outputSchema` 在 JSON 结构上完全相同，每份 1,035 bytes，来自统一的 `ToolCallJsonResponse` schema；这一点由样本逐项比较确认，生成路径是 `ToolDef::generate_output_schema()` 和 `ToolDef::to_tool()`。每份 schema 内的 `title` 与各层 `description` 只是重复的说明性注解；保留 `$schema`、`type`、`properties`、`required`、`enum` 和 `anyOf`，仅去掉这些说明性注解时，每份变为 540 bytes，静态投影为目录少 23,265 bytes，约为当前 `tools/list` wire 的 21.3%。这是 JSON 字节投影，尚非已实施结果或 token 测量。

当前 25 个工具调用的请求 wire 合计 4,319 bytes，响应合计 62,952 bytes。最大响应来源是 `brp_list_bevy` 15,194、两次 `brp_read_log` 11,475、两次 `brp_list_logs` 11,458、三次截图 5,078 bytes；这些数含完整 MCP envelope。日志内容、已发现 target、错误详情和截图状态都对当前任务有用，不能仅凭尺寸删掉。25 个调用中，24 个响应的 text content 可解析为与 `structuredContent` 相同的 JSON；`brp_launch` 的 text 与结构化内容存在时间格式差异。`CallToolResult::structured` 同时发出两种形态，属于兼容 text-only 客户端的现有框架行为；没有真实客户端证据支持删除其中一种。

另做一组已知目标目录的实测：仅把 `brp_list_bevy` 的 `path` 从仓库根目录改为 `tests/test-app`，调用本来就支持的参数，其他 24 次工具调用及验证目标保持相同。根目录发现 13 个 target，该次响应 15,194 bytes；指定 package 目录发现 6 个 target，该次响应 7,080 bytes，少 8,114 bytes，请求多 17 bytes。按 package、name、kind 和 manifest 匹配的 6 个目标，其 `brp_level` 和 `built` 一致；`relative_path` 改为相对于指定搜索目录，例如 `tests\\test-app` 变为 `test-app`，消费该字段的客户端必须按调用时的搜索根解释。完整重放仍为 25 次工具调用，离屏 PNG 与 clean shutdown 成功；整轮 server→client 为 165,580 bytes，对照轮为 172,374 bytes，但日志和运行时字段会波动，不能把整轮差值 6,794 bytes 当作稳定收益。缩小搜索目录只适用于调用方已经知道目标 package 目录的任务；目标位置未知时仍需从较宽范围发现。精简的单次发现记录与该轮结果见 [scoped-discovery.json](assets/15-efficiency/scoped-discovery.json)。

服务端到 Bevy 的 BRP/HTTP 请求数**未测**。当前重放器只记录 MCP stdio，App 日志与默认构建没有逐 HTTP 请求计数；一次 MCP 调用内部可产生 discovery、查询或 watch 请求。不能把 25 次 MCP 调用当成 BRP 请求数，也不能把 trace 中仅针对带参数请求的 debug 行当完整计数。方案 16 若要证明 BRP 请求减少，应在代理或明确的请求边界单独计数，并与 MCP 次数并列报告。本次选中的目录优化不改变 BRP 请求路径，故这个未测量不影响目录字节归因。

本次使用无模型本地协议重放器。客户端实际输入 token、缓存输入 token、输出 token、可确认模型轮次、真实客户端每轮纳入的工具定义及价格均**未测**。没有用 109,281-byte 目录或 23,265-byte 投影换算 token、缓存收益或费用；也没有假定工具目录只在启动时付一次或每轮完整计费。

## 决定与方案 16 边界

15 没有规定 16 只能做一种优化。当前有两项**互相独立**且有样本支持的建议，用户可确认其中一项或两项；确认前均不是已批准的施工范围：

| 建议 | 修改边界与预期作用 | 兼容影响与复测 |
| --- | --- | --- |
| A. 精简重复的 `outputSchema` 说明性注解 | 在 MCP `tools/list` 的 output schema 生成中仅去掉 `title` / `description` 注解，保留 `$schema`、字段约束、实际 `CallToolResult`。预计单次目录响应原始字节下降约 23,265 bytes；实际值以重放为准。 | 属于公开 schema 元数据变化。检查严格 schema 客户端、text/structured 两种结果消费者；若客户端依赖这些注解或任务交互变差，应撤回。 |
| B. 指引已知目录的定向发现 | 调整 `brp_list_bevy` 的 help text/使用样例：目标 package 目录已知时把该目录作为现有 `path` 参数；未知时从 workspace 根目录发现。不改默认搜索、tool 参数或返回结构。实测该场景单次发现响应少 8,114 bytes，MCP 调用数不变。 | 属于调用建议，不能保证所有客户端照做。`relative_path` 随搜索根变化，样例须说明解释方式；以完整同目标调用链验证找到所需 target、launch 和后续操作，另测未知目录场景不会被误导成窄搜索。 |

方案 16 的共同通过条件：完整用户任务仍正确完成，错误与纠正次数不增加；MCP 次数、BRP 请求数、关键结果和端到端耗时分别记录。若实施 A，还要确认默认与 `mcp-debug` 目录的 tool 数和 output schema 字段约束不变，实际目录 wire bytes 下降。若实施 B，还要确认定向发现仍覆盖已知目标且保留宽范围发现说明。若取得真实客户端 usage，再报告输入、缓存输入、输出 token 与轮次，分别测新进程和重复会话；没有该数据时只报告字节和调用效果。任一建议若没有改善或有超出接受范围的回归，应撤回该项，不以单个字节数字判成功。

其他候选暂不进施工清单：缩短通用 tool 说明可能破坏参数和调用顺序提示；合并/省略重复 text 与 `structuredContent` 涉及消费兼容；压缩日志、错误、watch、type guide 或大结果需先证明当前任务不依赖被删信息；删除 discovery 可能增加探索或损失运行时 method 验证。当前记录没有这些证据，也没有真实客户端 token 数据支持重做整套公共 API。
