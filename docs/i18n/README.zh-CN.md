# twokey

**阻止 AI 修改受保护的代码。**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey 阻止被归因于 AI 的变更修改受保护的文件、函数和代码区域。检查是确定性的，不使用 AI 模型。人工审查由 GitHub 管理；twokey 不管理用户、组或审批。

## 安装

在此仓库的本地副本中运行。需要 Rust 1.90 或更高版本、Git 和 C 编译器。

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## 快速开始

运行 `twokey init` 生成[初始配置](../../examples/twokey.toml)，取消所需规则的注释并调整路径。默认将更改视为 AI 辅助编写；完全由人类编写的提交应在提交消息末尾空一行后添加 `Human-Authored: true`。

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

预览会将变更视为 AI 编写，只报告结果而不阻止操作。启用强制检查前，请先将策略提交到 Git。

## 文档与贡献

本页是翻译后的入门介绍，以[英文文档](../../README.md)为准。请参阅[配置说明](../configuration.md)、[GitHub 集成](../github-actions.md)和[安全模型](../security.md)。本地钩子可以被绕过；强制执行需要可信的 CI 和分支保护。欢迎提交翻译修正。

维护者：Nick Darnell。项目采用 [CC0 1.0 Universal](../../LICENSE)。第三方依赖保留各自的许可证。
