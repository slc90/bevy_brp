# Git 规则

## 用户授权

只有用户明确要求时才创建 commit；只有用户明确要求 push 时才推送。不得自动创建 tag、release 或发布 crate。

不得使用 `git reset --hard`、`git checkout --` 或其他会丢失用户修改的命令，除非用户明确要求且目标已核实。

## 提交边界

提交前检查完整 status 和 diff：

- 只包含当前目标的最终修改；
- 不包含用户的无关 staged/unstaged/untracked 文件；
- 不包含运行时临时文件、凭据、用户绝对路径或未脱敏日志；
- 相关验证和独立 Review 已完成，或明确报告未完成原因。

如果当前任务需要多个语义独立阶段，按用户要求或已有书面计划拆成 focused commit。不要为了文件数量机械拆分。

## Commit message

message 必须依据最终 diff 编写，而不是照抄任务、计划或开发过程。subject 应准确概括实际变化，避免“更新代码”“修复问题”等空泛描述。

默认使用仓库近期历史一致的简洁 subject。需要 body 时，用有语义的条目说明最终行为、结构或规则变化；不要按文件、function 或 diff hunk 罗列。

message 不记录：

- 执行过哪些测试或格式化命令；
- 临时尝试和已经撤销的实现；
- 没有独立语义的生成文件变化；
- 尚未实现的后续计划。

技术标识保持原名。除非仓库已有明确且一致的约定，不强制类型前缀、issue ID 或 Change-Id。

## Windows 命令

PowerShell 中包含 `@{upstream}`、通配符或特殊字符的 Git revision/range 必须正确引用，例如：

```powershell
git log '@{upstream}..HEAD'
```

文件操作始终在同一 shell 内完成，不把 PowerShell 枚举结果拼接给另一 shell 做删除或移动。

## 提交后

提交后报告 commit hash、subject 和工作区是否干净。未获 push 授权时明确保留在本地。
