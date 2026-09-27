# 03 · Rust 工具链与 workspace 工程基准决策

状态：**用户已确认，可作为方案 04 的实施输入。**

本文件固定下一阶段要实施的工程基准。用户已明确决定：删除 `cargo-deny`、`cargo-mend`、Taplo；Rustfmt 与 Clippy 直接采用本机 `bevy_widgetry` 当前配置；Cargo resolver 改为 3；当前不支持 Linux/WASM，并可删除专用于这两个平台的实现与配置。

## 1. 决策摘要

| 项目 | 决策 |
| --- | --- |
| Rust 工具链 | 固定 `1.98.1`，使用 `rust-toolchain.toml` |
| Rust 组件 | `rustfmt`、`clippy`；不再需要 `rustc-dev` |
| Rust target | 只要求宿主 `x86_64-pc-windows-msvc`，不安装额外交叉编译 target |
| Rust edition | 保持 2024，在 `[workspace.package]` 集中声明 |
| Cargo resolver | 从 2 改为 3 |
| Bevy | 保持 0.19.1，本项不升级依赖 |
| Rustfmt | 完整复制 `bevy_widgetry/rustfmt.toml` 当前内容 |
| Clippy | 完整复制 Widgetry 的 workspace lint、`clippy.toml` 和验证命令口径 |
| 辅助工具 | 删除 `deny.toml`、`mend.toml`、`taplo.toml`，不设置替代工具 |
| 平台范围 | 只验证 Windows MSVC；删除 Linux/WASM 专用配置、实现、测试和当前能力说明 |

## 2. Rust 版本与组件

根目录新增：

```toml
[toolchain]
channel = "1.98.1"
profile = "minimal"
components = ["clippy", "rustfmt"]
```

选择 `1.98.1` 的依据：

- 方案 01 使用 `rustc 1.98.1 (48a229cea 2026-09-01)` 和 Cargo 1.98.1 完成了当前 Windows 基线；本次 Clippy 也在该版本通过。
- 当前 edition 2024 和 Bevy 0.19.1 依赖图可由该版本解析和编译。
- 不使用浮动 `stable`、`nightly` 或 `latest`。

删除 `cargo-mend` 后不再需要 `rustc-dev`。不声明 `rust-src`、`llvm-tools-preview` 或 `wasm32-unknown-unknown`；开发者本机额外安装的组件不构成仓库要求。

## 3. 平台边界

当前工程基准只承诺并验证：

```text
x86_64-pc-windows-msvc
```

保留 `.cargo/config.toml` 中该 target 的 `rust-lld.exe` 和 rustdoc linker 配置。

Linux 和 WASM 不再属于当前支持范围。方案 04 可删除直接服务于这两个平台的内容：

- `rust-toolchain.toml` 不安装 wasm target，验证命令不包含 Linux/WASM job 或 wasm check。
- `extras/Cargo.toml` 删除 `cfg(not(target_arch = "wasm32"))` dependency table，把 Windows 实际使用的依赖恢复为普通 dependency。
- `extras` 源码删除 `cfg(target_arch = "wasm32")` 分支、WASM fallback、WASM 专用错误和 WASM 专用测试；原生路径改为无条件路径。
- `extras/README.md`、rustdoc 和 `[Unreleased]` changelog 准确记录不再支持 WASM；历史 changelog 条目保留，不能改写过去发布事实。
- 删除 test app 中仅为 Linux/Wayland 准备的 minimize 分支，保留 Windows 实际路径。
- 删除 MCP help text 中仅面向 Linux 的按键说明。

共享实现中顺带提到 Linux、但同时承担 Windows/macOS 通用职责的代码不机械删除；只有专用分支、专用测试和不再成立的支持承诺属于本次范围。

这是用户明确批准的平台支持范围收缩。除上述平台边界外，不改变公共 Rust API、MCP tool contract 或 Windows 运行行为。

## 4. edition、resolver 与 workspace metadata

### 4.1 edition 与 resolver

- edition 保持 2024，并在 `[workspace.package]` 定义 `edition = "2024"`。
- 根 `[workspace]` 改为显式 `resolver = "3"`。
- 在 `[workspace.package]` 定义 `rust-version = "1.98.1"`。该字段表达最低受支持编译器；精确复现由 `rust-toolchain.toml` 完成。

Resolver 3 保留 resolver 2 的 feature 隔离行为，并在选择依赖版本时默认优先考虑与 `rust-version` 相容的版本。方案 04 必须检查 `Cargo.lock`；如果仅修改 resolver 就改变了解析结果，应展示并解释差异，不得借机升级不相关依赖。

### 4.2 metadata 继承

根 `[workspace.package]` 集中：

```toml
authors = ["natepiano <slicks.curable.0k@icloud.com>"]
edition = "2024"
license = "MIT OR Apache-2.0"
repository = "https://github.com/slc90/bevy_brp"
rust-version = "1.98.1"
version = "0.1.0"
```

继承范围：

- 所有成员继承 `edition` 和 `rust-version`。
- `bevy_brp_extras`、`bevy_brp_mcp`、`bevy_brp_mcp_macros`、`bevy_brp_runtime` 继承 `authors`、`license`、`repository` 和 `version`。
- `test-app-a`、`test-app-b` 只继承当前已经声明的 `license`、`repository`，不新增作者或发布版本归属。
- `bevy_brp_test_apps` 保持测试应用身份，不凭空补作者、许可证、仓库或发布版本。
- `name`、`description`、`readme`、`categories`、`keywords`、`publish`、target 和 `[features]` 仍由各 crate 自己声明。

### 4.3 共享依赖

- 第三方依赖版本集中在 `[workspace.dependencies]`；成员负责本 crate 实际需要的 `features` 和 `default-features`。
- 内部 crate 的 path 和共同版本来源集中在根 manifest。方案 04 补充 `bevy_brp_runtime` workspace dependency，并把成员中的直接 path 写法改为继承。
- feature 不做全 workspace 强制统一。测试应用继续使用完整 Bevy defaults，extras 保留自身 Windows 所需 feature。
- 不执行无关依赖更新；最终用 `--locked` 验证解析结果。

## 5. Rustfmt：直接采用 Widgetry

方案 04 用相邻 Widgetry checkout 的 `../bevy_widgetry/rustfmt.toml` 当前内容完整替换本仓库文件：

```toml
edition = "2024"
style_edition = "2024"
newline_style = "Unix"
reorder_imports = true
reorder_modules = true
```

不叠加 BRP 原有选项。原文件中的 nightly-only comment wrapping、import granularity、字段对齐和单行函数设置全部删除。

替换后执行一次：

```powershell
cargo fmt --all
```

这会产生较大的机械格式变化。实施时必须把它识别为 Widgetry 格式基准迁移，不在同一批修改中顺手重构语义。

## 6. Clippy：直接采用 Widgetry

根 `Cargo.toml` 的 workspace lint 完整替换为 Widgetry 当前配置：

```toml
[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
unwrap_used = "deny"
type_complexity = "allow"
too_many_arguments = "allow"
```

根目录新增与 Widgetry 相同的 `clippy.toml`：

```toml
allow-unwrap-in-tests = true
```

这项决定意味着删除 BRP 当前的 `missing_docs`、`all`、`cargo`、`nursery`、`pedantic` 组级 deny，以及 `expect_used`、`panic`、`unreachable` 等单项 deny。也删除因 `cargo-mend` 才存在的 `redundant_pub_crate` 例外。方案 04 不把旧规则叠加到 Widgetry 基准上。

统一命令沿用 Widgetry 的形态，并增加 `--locked` 保护本仓库 lockfile：

```powershell
cargo clippy --workspace --all-targets --locked
```

`unsafe_code = "forbid"` 与当前源码兼容：本次搜索没有发现自有 Rust 源码中的 `unsafe`。测试中的 `unwrap` 由 `clippy.toml` 明确允许，生产代码仍由 `unwrap_used = "deny"` 约束。

Module 文件布局也直接采用 Widgetry 的实际规则，但不把它错误表达成全 workspace Clippy lint：

- 生产源码不使用 `mod.rs`，采用 `foo.rs + foo/` 的现代 module 布局。
- crate 的 `tests/` integration test 目录允许按需要使用传统 `mod.rs`。
- 不启用 `self_named_module_files`；它会强制生产源码使用 `mod.rs`，与 Widgetry 规则相反。
- 也不在 workspace 全局启用 `mod_module_files`；该 lint 无法直接表达 `tests/` 路径例外。
- 这项路径规则写入 `rules/code.md` 和 `rules/testing.md`，由 Review 执行。测试 unwrap 则继续由 Clippy 自动区分。

当前 BRP 有 19 个生产源码 `mod.rs`，且 integration test 中没有 `mod.rs`。方案 04 将这 19 个文件迁移为对应的 self-named module file；这是已确认的 Widgetry 规则迁移，不借机改变 module 层次、visibility 或实现职责。

## 7. 删除辅助工具

### 删除 `deny.toml`

同时删除 `cargo deny check` 的安装、README、脚本和 CI 入口。不引入 `cargo-audit` 或其他替代工具。结果是仓库不再自动检查 RustSec advisory、依赖许可证、registry/Git 来源和多版本策略；依赖变化只由 lockfile、Cargo 构建和 Code Review 管理。

### 删除 `mend.toml`

同时删除 cargo-mend 安装、命令和 `rustc-dev` 组件。不设置专用 visibility analyzer；可见性由编译器、Widgetry Clippy 基准和 Code Review 管理。

### 删除 `taplo.toml`

同时删除 Taplo 安装、命令和 CI gate。不引入 TOML formatter 替代品；Cargo 负责 manifest 语法有效性，TOML 排版由修改者和 Code Review 保持。

三个删除决定是用户明确选择。方案 04 不保留“配置已删除但文档仍要求运行”的残余入口。

## 8. 验证基准

### 8.1 Windows 完整验证

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
cargo build --workspace --locked
```

不再运行 `cargo deny`、`cargo mend`、Taplo、Linux 或 wasm 命令。

方案 01 已记录 Windows 测试中的两个 `mcp/src/app_tools/launch/build_freshness.rs` dep-info 路径失败。方案 04 必须把它们与新增失败分开报告；工具链迁移不得引入新的失败。

### 8.2 必要 feature 组合

默认 workspace 检查覆盖 extras 的 `diagnostics + ui` 和 MCP 的 `mcp-debug`。继续检查 Windows 上的 feature 缺席与独立启用：

```powershell
cargo check -p bevy_brp_extras --locked --no-default-features
cargo check -p bevy_brp_extras --locked --no-default-features --features diagnostics
cargo check -p bevy_brp_extras --locked --no-default-features --features ui
cargo check -p bevy_brp_mcp --locked --no-default-features
```

不再建立 WASM feature matrix。

### 8.3 GUI 与 CI

- 本地 Windows：执行完整验证和开发中的定向测试，同时验证 `rust-lld` 配置。
- GUI Windows：执行 `cargo run -p bevy_brp_test_apps --example extras_plugin --locked`，确认窗口、renderer、runtime 与 BRP 启动链。
- CI：若方案 04 建立 workflow，只设置 Windows runner，执行格式、check、Clippy、test、build 和 feature matrix；不设置 Linux、macOS 或 WASM job。

## 9. 方案 04 受影响位置

方案 04 按本决策修改：

- 新增根 `rust-toolchain.toml` 与 `clippy.toml`。
- 用 Widgetry 当前内容替换 `rustfmt.toml`。
- 更新根和成员 `Cargo.toml`：resolver 3、metadata/依赖继承、Widgetry lint、Windows-only dependency 结构。
- 更新 `rules/code.md`、`rules/testing.md`：生产源码禁止 `mod.rs`，integration test 允许例外；删除原 `self_named_module_files` 规则。
- 把当前 19 个生产源码 `mod.rs` 迁移为 self-named module file，只改变文件入口形式，不重组 module 层次。
- 删除 `deny.toml`、`mend.toml`、`taplo.toml` 及其所有命令引用。
- 删除 extras 的 WASM 专用源码、测试、manifest 分支和支持说明；同步 README、rustdoc 和 `[Unreleased]` changelog。
- 删除 test app 与 MCP help text 中仅为 Linux 存在的分支或说明。
- 更新根 `README.md`，只保留 Windows 工具链和统一验证入口。
- 如建立 CI，仅建立 Windows workflow。
- 应用 Widgetry rustfmt 产生的机械 Rust 源码格式变化。
- 仅接受 resolver/manifest 变更所必需且可解释的 `Cargo.lock` 变化，不更新依赖版本。

不修改 Widgetry 仓库，不改变 MCP tool 名称、参数、结果、annotation 或 Windows BRP transport 行为。

## 10. 确认结果

用户已确认：

1. Rust 固定为 `1.98.1`，edition 2024，resolver 3。
2. Rustfmt、Clippy 配置和 Clippy 命令口径按 Widgetry 当前快照执行，不保留 BRP 原 lint 叠加项；module 文件布局按 Widgetry 规则由工程规则和 Review 执行。
3. 删除 deny/mend/Taplo，接受不设置替代检查器。
4. 只保证 Windows MSVC，删除 Linux/WASM 专用实现、测试、配置和当前支持说明。
5. 保持 Bevy 0.19.1 和其他依赖版本不变。

方案 03 至此完成。按设计关口停止；不自动实施方案 04，不提交、不推送。
