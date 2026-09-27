# 代码规则

## 服从现有 lint contract

根 `Cargo.toml` 的 workspace lint 是代码硬约束。普通改动不得降低 lint level、扩大 allow 或删除已有检查来让代码通过。当前基准禁止 `unsafe` 和生产代码中的 `unwrap`；测试中的 `unwrap` 由 `clippy.toml` 允许。

自有生产代码默认不得使用 `unwrap`、`expect`、`panic!`、`unreachable!` 或 `unsafe`。测试可以在失败即表示 fixture/test bug 时使用 `unwrap` 或 `expect`。proc macro 编译期诊断、无法恢复的内部 invariant 和框架要求的特殊路径只能使用最小 scope 的例外，并提供准确的 `reason`。

不得留下 `todo!()`、`unimplemented!()` 或 `dbg!()` 作为未完成实现。已有 trait 默认实现或 proc macro 诊断不应在无关任务中顺手清理。

## 错误处理

禁止静默吞掉错误：

1. 当前层能正确处理就在当前层处理。
2. 上层能处理则通过 `Result`、`Option` 或已有 error type 传播并保留上下文。
3. 已进入不可恢复状态且没有合理上层路径时，先记录足够诊断信息，再按既有 contract 终止。

不得用 `let _ =`、无意义的 `.ok()`、空 `Err` 分支或默认成功值丢弃失败。cleanup 中的 best-effort 失败也必须依据现有 contract 被传播、记录或明确解释。

面向 MCP/BRP 的错误应保留调用方采取下一步动作所需的 method、port、stage、field 或原因；不得用内部 debug 字符串替代已有结构化 error contract。

## Lint 抑制

优先修正 lint。确属误报、框架限制或不可避免的例外时：

- 限制到最小 scope；
- 使用 `reason` 说明语义原因；
- 不使用 `clippy::all`、`unused`、`dead_code` 等大范围兜底；
- 不为通过 lint 而重构与任务无关的代码。

根 workspace 已明确允许的 lint 不需要逐处重复说明。

## 源码组织

保持当前 module 的稳定组织，不因文件长度或个人偏好移动文件、重排 item 或机械执行“一种 type 一个文件”。

Library crate 以 `lib.rs` 为装配入口，binary 以 `main.rs` 为入口。入口文件主要承担 module 声明、re-export 和必要装配，不应无依据堆入新的具体实现。

生产源码使用 `foo.rs + foo/` 布局，不使用 `mod.rs`；integration test 的 `tests/` 目录按需要允许 `mod.rs`。不要在普通任务中迁移与目标无关的现有 module。

## Visibility 与 API

使用满足当前需求的最小 visibility：private → `pub(crate)` → `pub`。不得为了测试或跨 module 调用方便扩大公共 API。

公共 Rust API、MCP schema 和 BRP method 都是 contract。新增或修改前要确认调用方、序列化形状、默认值、error、lifecycle 和文档，不得让内部实现细节意外进入 contract。

## 并发与 lifecycle

异步任务、channel、watch、process、socket、临时文件和 activity guard 必须有明确 ownership 与结束路径。实现应覆盖成功、错误、timeout、cancellation、receiver drop 和 shutdown 中与当前功能相关的路径，避免任务、端口或进程残留。

不得持有 sync lock 跨越 `await`，除非所用 lock 的语义明确允许且有必要证据。共享状态变化应通过 type 和测试表达 generation、终态或幂等要求。

## 注释与格式

只为关键语义、非显然约束和“为什么”添加注释，不给显然语句写旁白。具体语言和 rustdoc 规则见 `rules/documentation.md`。

只格式化当前修改涉及的 Rust/TOML 文件，避免因本地 stable/nightly rustfmt 差异制造无关 churn。
