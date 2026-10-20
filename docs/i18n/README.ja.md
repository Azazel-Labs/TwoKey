# twokey

**保護されたコードへのAIによる変更を防ぎます。**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey は、AIによるものと判定された変更が保護対象のファイル、関数、領域に及ぶことを防ぎます。チェックは決定的で、AIモデルを使用しません。人間のレビューはGitHubが管理し、twokeyにはユーザー、グループ、承認の仕組みはありません。

## インストール

このリポジトリをローカルに取得して実行します。Rust 1.90 以降、Git、C コンパイラが必要です。

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## クイックスタート

`twokey init` で[初期設定](../../examples/twokey.toml)を作成し、必要なルールのコメントを解除してパスを調整してください。変更はデフォルトでAIによるものとして扱います。完全に人間が書いたコミットには、メッセージ末尾に空行を挟んで `Human-Authored: true` を記載します。

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

このプレビューでは変更を AI によるものとして扱い、ブロックせずに結果を表示します。強制適用を有効にする前に、ポリシーを Git にコミットしてください。

## ドキュメントと貢献

このページは導入部分の翻訳です。[英語のドキュメント](../../README.md)を正式な参照先としてください。[設定](../configuration.md)、[GitHub 連携](../github-actions.md)、[セキュリティモデル](../security.md)も参照できます。ローカルのフックは回避できるため、強制適用には信頼できる CI とブランチ保護が必要です。翻訳の修正も歓迎します。

メンテナー: Nick Darnell。[CC0 1.0 Universal](../../LICENSE) で公開されています。依存ライブラリにはそれぞれのライセンスが適用されます。
