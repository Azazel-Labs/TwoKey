# twokey

**重要なコードの変更に、人間の承認を。**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

twokey はリポジトリのポリシーに従ってコード変更を検査する CLI ツールです。AI による変更と判定された編集を保護対象のファイルで禁止したり、指定した人の承認を必須にしたりできます。検査は決定的で、AI モデルを使用しません。

## インストール

このリポジトリをローカルに取得して実行します。Rust 1.90 以降、Git、C コンパイラが必要です。

```sh
git clone https://github.com/Azazel-Labs/twokey.git
cd twokey
cargo install --path . --locked
```

## クイックスタート

[最小構成の例](../../examples/minimal.toml)をリポジトリのルートに `.twokey.toml` としてコピーします。`your-github-login` を承認者の GitHub ユーザー名に置き換え、パスを調整してください。この例は AI による CI 設定の変更を禁止し、`/src/auth/**` の変更に承認を要求します。

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --ai --dry-run
```

このプレビューでは変更を AI によるものとして扱い、ブロックせずに結果を表示します。強制適用を有効にする前に、ポリシーを Git にコミットしてください。

## ドキュメントと貢献

このページは導入部分の翻訳です。[英語のドキュメント](../../README.md)を正式な参照先としてください。[設定](../configuration.md)、[GitHub 連携](../github-actions.md)、[セキュリティモデル](../security.md)も参照できます。ローカルのフックは回避できるため、強制適用には信頼できる CI とブランチ保護が必要です。翻訳の修正も歓迎します。

メンテナー: Nick Darnell。[CC0 1.0 Universal](../../LICENSE) で公開されています。依存ライブラリにはそれぞれのライセンスが適用されます。
