# twokey

**让重要的代码变更经过人工批准。**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey 是一个根据仓库策略检查代码变更的命令行工具。它可以阻止被识别为 AI 编写的变更修改受保护文件，或要求指定人员批准。检查过程是确定性的，不使用 AI 模型。

## 安装

在此仓库的本地副本中运行。需要 Rust 1.90 或更高版本、Git 和 C 编译器。

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## 快速开始

将[最小配置示例](../../examples/minimal.toml)复制到仓库根目录，命名为 `.twokey.toml`。把 `your-github-login` 替换为审批人的 GitHub 用户名，并调整路径。该示例禁止 AI 修改 CI 配置，并要求对 `/src/auth/**` 的变更进行人工审批。

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

预览会将变更视为 AI 编写，只报告结果而不阻止操作。启用强制检查前，请先将策略提交到 Git。

## 文档与贡献

本页是翻译后的入门介绍，以[英文文档](../../README.md)为准。请参阅[配置说明](../configuration.md)、[GitHub 集成](../github-actions.md)和[安全模型](../security.md)。本地钩子可以被绕过；强制执行需要可信的 CI 和分支保护。欢迎提交翻译修正。

维护者：Nick Darnell。项目采用 [CC0 1.0 Universal](../../LICENSE)。第三方依赖保留各自的许可证。
