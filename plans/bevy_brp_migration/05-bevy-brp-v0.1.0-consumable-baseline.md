# bevy_brp 形成 v0.1.0 可消费基线

**Repository:** `slc90/bevy_brp`

## 目标

在 bevy_brp 内部迁移完成后，做必要的文档收口与本地验证，形成可供其他项目通过 Git tag 稳定消费的 `v0.1.0` 基线。

## 范围

仅限 `slc90/bevy_brp` 仓库的 README/当前说明、workspace 整体验证和 Git tag 分发边界。不做额外 MCP/Extras/Runtime 重构。

## 预期产出

根 README 与 MCP/Extras README 不再引用已删除的 `.claude/`、CI 或 crates.io/docs.rs 发布路径，仓库通过本地 workspace 验证和一次完整 Agent 链路验证，并以 `v0.1.0` 作为后续 Widgetry 的 Git 依赖基线。

## 与前后方案的关系

它是所有 bevy_brp 仓库内部方案的收口点。只有这一方案形成可消费 `v0.1.0` 后，执行链才进入 `bevy_widgetry` 仓库。

## 方案内容

## bevy_brp 文档最小同步

`bevy_brp` 文档本次只做因仓库清理和分发方式改变而必须的最小修正：

- 根 README 删除 `.claude/`、CI、crates.io 发布等失效描述，并加入 `runtime/`。
- MCP / Extras README 删除失效的 CI / crates.io / docs.rs 发布入口，依赖或安装示例改成 Git tag `v0.1.0`；其余内容先保留，后续专门整理。
- repository 链接改为 `slc90/bevy_brp`。

### bevy_brp

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets
cargo test --workspace
```

另外完成一次完整 Agent 链路验证：MCP 启动测试 App，执行至少一次 BRP 查询、截图或输入操作，并通过 BRP 正常 shutdown，确认 `MCP -> runtime -> extras -> Bevy App` 可工作。

## 最终责任边界

```text
Agent
  -> bevy_brp_mcp
  -> bevy_brp_runtime
  -> bevy_brp_extras
  -> Bevy App
```
