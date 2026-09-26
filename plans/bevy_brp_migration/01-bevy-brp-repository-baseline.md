# bevy_brp 仓库基线、清理与版本线

**Repository:** `slc90/bevy_brp`

## 目标

建立后续迁移所依赖的独立 bevy_brp 仓库基线，清理不再使用的上游开发设施，并固定本地 Cargo 配置与 0.1.0 版本线。

## 范围

仅限 `slc90/bevy_brp` 仓库。覆盖原总方案第 1–4 节；不迁入 Extras/Runtime 业务实现，不修改 Widgetry。

## 预期产出

`main` 以 0.22.7 基线继续开发，仓库去除 Claude/CI/Nix/as-built 等已明确不再使用的内容，Windows 本地构建使用 rust-lld，`mcp-debug` 成为正常默认 feature，workspace 统一准备 0.1.0 版本线且不发布 crates.io。

## 与前后方案的关系

这是整条链的第一个方案。它为后续 Extras 合并、Runtime 迁移和测试链调整提供稳定仓库基线；`v0.1.0` 标签在 bevy_brp 内部迁移与验证完成后再形成可消费基线。

## 方案内容

## 1. bevy_brp 基线与分支

- 以原 `release-0.22.7` 的内容和历史作为新开发基线。
- 最终只保留 `main`，其内容从该基线开始继续开发。
- 删除其余 release/update 等远端分支。
- 后续不再跟踪或同步原上游仓库；`slc90/bevy_brp` 作为独立维护的代码库。

## 2. 仓库清理

删除与当前开发方式无关的上游开发基础设施：

```text
.claude/
CLAUDE.md
.github/
.envrc
flake.nix
flake.lock
nix/
docs/as-built/
```

保留 MCP 和测试链路：

```text
extras/
mcp/
mcp_macros/
test-app/
test-duplicate-a/
test-duplicate-b/
```

其中 `mcp/help_text/*.txt` 必须保留：这些文本由 `mcp_macros` 在编译时通过 `include_str!` 嵌入 MCP binary，是 Agent tool description 的一部分。

MIT / Apache-2.0 license 文本全部保留。

## 3. Cargo 本地配置

不再使用全局 rustflags 伪造 Cargo feature，也不保留 WASM runner。

`.cargo/config.toml` 只保留 Windows LLD：

```toml
[target.x86_64-pc-windows-msvc]
linker = "rust-lld.exe"

rustdocflags = ["-Clinker=rust-lld.exe"]
```

`mcp-debug` 改为 `bevy_brp_mcp` 的正常默认 feature：

```toml
[features]
default = ["mcp-debug"]
mcp-debug = []
```

## 4. 版本与分发

整个仓库从自己的版本线重新开始：

```text
bevy_brp_extras      0.1.0
bevy_brp_runtime     0.1.0
bevy_brp_mcp         0.1.0
bevy_brp_mcp_macros  0.1.0
Git tag              v0.1.0
```

根 workspace 使用统一版本：

```toml
[workspace.package]
version = "0.1.0"
repository = "https://github.com/slc90/bevy_brp"
```

四个正式 crate 使用 `version.workspace = true` / `repository.workspace = true`，并设置 `publish = false`，明确不发布 crates.io。

对外统一通过 Git tag 使用。
