# twokey

**让 AI 写代码，由你决定它不能改哪里。**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

代理通过弱化测试来修复错误。一次重构改动了令牌验证。一次清理修改了已在生产环境执行的数据库迁移。

**twokey 把“别改这里”变成可检查的仓库策略。** 在 `.twokey.toml` 中划定范围，在本地获得反馈，并要求检查通过后才能合并。

- 保护特定函数，不必锁定整个文件。
- 允许添加新测试，同时阻止 AI 修改已有测试。
- CI 从目标分支读取策略，PR 无法放宽自身的检查规则。
- 默认将无标签的更改视为 AI 编写，也适用于不留归属信息的本地模型。

检查使用文件模式、语法树、标记和正则表达式，不需要调用模型、提供 API 密钥或向 AI 服务上传代码。审查和合并权限仍由 GitHub 管理。

## 你想保护什么？

| 交给 AI 的工作 | 需要保护的内容 | 规则类型 |
| --- | --- | --- |
| 登录界面 | 令牌验证与权限检查 | 符号 |
| 错误修复与新增测试 | 已有测试 | 符号的修改或删除 |
| 新增数据库迁移 | 已执行的迁移 | 路径的修改或删除 |
| 功能开发 | CI、依赖文件和加密代码 | 路径 |
| 周边实现 | 已审查的计算或常量 | `twokey:lock` 标记 |
| 文档 | 不需要的长破折号或措辞 | 内容正则表达式 |

示例规则默认被注释。启用你需要的规则，并调整对应路径。

### 让认证逻辑只能由人类修改

```toml
[[rule]]
id = "token-validation"
kind = "symbol"
paths = ["/src/auth/**"]
symbols = ["verify_token", "Session::verify"]
action = "deny"
reason = "Token validation must be changed by a human."
```

此规则阻止 AI 修改 `verify_token` 和 `Session::verify`，也会捕获重命名和删除。同一文件中未受保护的 `greeting()` 仍可修改。如果导入语句和辅助函数也需要保护，请使用路径规则保护整个模块。

### 允许新增测试，保护已有测试

```toml
[[rule]]
id = "existing-rust-tests"
kind = "symbol"
paths = ["**/*.rs"]
symbols = ["tests::*"]
on = ["modified", "deleted"]
action = "deny"
reason = "Add regression tests; preserve existing test behavior."
```

保护 Rust `tests` 模块内的已有函数。允许添加测试，但阻止修改已有断言或删除测试。

### 锁定几行关键代码

```rust
// twokey:lock-begin
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
```

标记默认启用，并从基础版本读取。AI 修改常量或删除标记都会被阻止，无需为该区域另加路径规则。

### 自动检查写作规范

```toml
[[rule]]
id = "no-em-dashes"
kind = "content"
paths = ["*.md"]
pattern = '\x{2014}'
lines = "added"
action = "deny"
human_action = "deny"
reason = "Use a comma, colon, parentheses, or a separate sentence."
```

拒绝在新增或替换的 Markdown 行中使用长破折号，包括人类编写的更改。未改动的行不受影响。省略 `human_action` 即可只约束 AI 更改。

语法规则支持 Rust、Python、JavaScript、TypeScript/TSX、Go、Java、C# 和 Ruby。路径与内容规则也适用于其他文件类型。

[初始策略](../../examples/twokey.toml)还包含数据库迁移、跳过测试、依赖和禁用静态检查等示例。

## 安装与快速开始

CI 推荐使用 [GitHub Action](../github-actions.md)，本地可从 crates.io 安装 CLI。Cargo 构建需要 Rust 1.90 或更高版本、Git 和 C 编译器，也可使用 [Windows、Linux 和 macOS 的预编译程序与安装脚本](../../README.md#installation)。

```sh
cargo install twokey --locked
```

在需要保护的仓库中生成策略，取消所需规则的注释并调整路径：

```sh
twokey init
```

示例规则默认被注释。启用你需要的规则，并调整对应路径。

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --dry-run
```

`--dry-run` 只报告结果，不阻止操作。启用强制检查前先将策略提交到 Git。策略及其集成配置的内置保护、源码标记，以及对符号链接、子模块和模式变更的默认限制仍然启用。

### 声明完全由人类编写的更改

默认将更改视为 AI 编写。初始配置使用 `#Human`，可通过 `[ai].human_marker` 自定义。完全由人类编写的更改可以这样提交：

```toml
[ai]
human_marker = "#Human"
```

```sh
git commit -m "Fix token validation" -m "#Human"
```

将标记单独放在提交消息末尾的一行，前面留一个空行。使用 Git 编辑器时，请设置 `git config core.commentChar ';'`，避免 `#Human` 被作为注释删除。明确的 AI 信号仍优先；后续的人类声明不会清除同一文件中已有的 AI 更改归属。这个标记是声明，不是作者身份的证明。

## 文档与贡献

本页是翻译后的入门介绍，以[英文文档](../../README.md)为准。请参阅[配置说明](../configuration.md)、[GitHub 集成](../github-actions.md)和[安全模型](../security.md)。本地钩子可以被绕过；强制执行需要可信的 CI 和分支保护。欢迎提交翻译修正。

维护者：Nick Darnell。项目采用 [CC0 1.0 Universal](../../LICENSE)。第三方依赖保留各自的许可证。
