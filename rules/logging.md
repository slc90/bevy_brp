# 日志与诊断规则

## 边界职责

`extras` 和 `runtime` 作为 Bevy library 只产生诊断 event，不初始化全局 subscriber。宿主 App 决定 filter、格式、输出位置和 `RUST_LOG`。

`bevy_brp_mcp` 是 stdio server，可以管理自己的 tracing 文件和动态 level；stdout 必须只用于 MCP transport，禁止向 stdout 打印普通日志、诊断或调试文本。

MCP 启动的 Bevy application 日志由现有 launch/log tools 写入系统临时目录。不得把用户应用日志、watch 日志和 MCP 自身 trace 混成同一 ownership 或绕过现有读取边界。

## Level

- `error`：内部 invariant 破坏、持久状态损坏、必要 cleanup/operation 无法完成且当前层不能正常恢复。
- `warn`：操作仍能返回或降级继续，但发生非预期外部失败、fallback、protocol/serialization 异常或能力不可用。
- `info`：少量重要 lifecycle 事实，例如 server/listener 启动、target build/launch、watch start/stop、clean shutdown。
- `debug` / `trace`：请求推进、target discovery、wire/stream 解析和内部决策等按需诊断。

不要把正常每帧更新、正常等待、每个 pointer/key event 或大响应 payload 永久记录为 info/warn/error。

## 错误传播与日志

已通过结构化 MCP error、BRP error 或 `Result` 交给调用方的失败，通常不在每一层重复记录。日志放在实际消费、fallback、后台 task 终止或错误无法继续传播的 ownership 边界。

若公开前置条件或内部 invariant 按既有 contract 必须 panic，先在 library 边界记录足够上下文；日志不得替代 panic，也不得改变原有操作顺序和失败语义。

不得为了记录错误而增加 polling system、延长 activity、改变 wake/timeout 或吞掉原错误。

## Structured context

优先使用 structured field 记录 method、tool、port、entity、pid、path、stage、error、watch_id 等动态上下文。message 说明发生了什么，不把所有字段拼成难以过滤的长字符串。

不得手工重复 tracing 已提供的 file、line、module metadata。

## 敏感信息与体积

不得记录凭据、token、authorization header、完整环境变量或未经筛选的用户数据。命令、path、request/response 或 schema 可能包含敏感内容时，只记录定位所需的最小字段；需要完整诊断样本时写入明确受控、可清理的位置并在版本库前脱敏。

大响应、watch stream 和 screenshot bytes 不进入普通 tracing message。记录摘要、大小、路径或稳定 ID，并沿用现有 large-response/log 工具边界。

## 持续异常

持续状态若需要日志，应按 transition 记录而不是按 frame/request 重复刷屏：首次进入异常记录一次，恢复时在确有诊断价值时记录一次，再次进入异常才重新记录。

去重 state 必须由负责该行为的 module 持有，不能为了日志改变业务 contract 或引入全局隐式状态。
