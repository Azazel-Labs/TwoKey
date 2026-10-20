# twokey

**AIにコードを書かせる。変更してはいけない場所は、あなたが決める。**

[English](../../README.md) · [Español](README.es.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [日本語](README.ja.md) · [简体中文](README.zh-CN.md)

エージェントがテストを弱めてバグを直す。リファクタリングがトークン検証に触れる。ちょっとした整理で、本番環境に適用済みのマイグレーションを書き換える。

**twokey は「そこは変更しないで」を検証可能なリポジトリのポリシーにします。** `.twokey.toml` で境界を定め、ローカルでフィードバックを受け取り、マージ前のチェック成功を必須にできます。

- ファイル全体を固定せず、特定の関数を保護できます。
- 新しいテストの追加を許可し、既存テストへのAIの変更をブロックできます。
- CIはマージ先ブランチのポリシーを読みます。PRが自身のチェックを緩めることはできません。
- タグのない変更もデフォルトでAIとして扱うため、帰属情報を残さないローカルモデルにも対応します。

チェックにはファイルパターン、構文木、マーカー、正規表現を使います。モデルの呼び出し、APIキー、AIサービスへのコード送信は不要です。レビューとマージ権限は引き続きGitHubが管理します。

## どこを保護しますか？

| AIに任せる作業 | 保護する対象 | ルール |
| --- | --- | --- |
| ログイン画面 | トークン検証と権限チェック | シンボル |
| バグ修正と新しいテスト | 既存のテスト | シンボルの変更・削除 |
| 新しいマイグレーション | 適用済みのマイグレーション | パスの変更・削除 |
| 機能開発 | CI、依存関係、暗号処理 | パス |
| 周辺の実装 | レビュー済みの計算や定数 | `twokey:lock` マーカー |
| ドキュメント | エムダッシュや避けたい表現 | 内容の正規表現 |

サンプルルールはコメントアウトされています。必要なルールだけ有効にし、パスを調整してください。

### 認証ロジックの変更は人間に限定する

```toml
[[rule]]
id = "token-validation"
kind = "symbol"
paths = ["/src/auth/**"]
symbols = ["verify_token", "Session::verify"]
action = "deny"
reason = "Token validation must be changed by a human."
```

このルールは `verify_token` と `Session::verify` へのAIの変更を、名前変更や削除も含めてブロックします。同じファイル内の `greeting()` の変更は許可できます。インポートや補助関数も保護したい場合は、パスルールでモジュール全体を保護してください。

### 既存テストを変えずに新しいテストを追加する

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

Rustの `tests` モジュール内の既存関数を保護します。新しいテストの追加は許可し、既存のアサーションの変更やテストの削除をブロックします。

### 重要な数行をロックする

```rust
// twokey:lock-begin
pub const MAX_TRANSFER: u64 = 10_000;
// twokey:lock-end
```

マーカーはデフォルトで有効で、ベースリビジョンから読み取ります。AIによる定数の変更やマーカーの削除はブロックされます。この領域に別途パスルールを設定する必要はありません。

### 文章のルールも自動チェックする

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

追加・置換されたMarkdown行のエムダッシュを、人間の変更も含めて拒否します。変更のない行は対象外です。AIの変更だけに適用する場合は `human_action` を省略してください。

構文ルールは Rust、Python、JavaScript、TypeScript/TSX、Go、Java、C#、Ruby に対応しています。パスと内容のルールはその他のファイル形式にも使えます。

[初期ポリシー](../../examples/twokey.toml)には、マイグレーション、テストのスキップ、依存関係、リンターの抑制の例もあります。

## インストールとクイックスタート

CIでは [GitHub Action](../github-actions.md)、ローカルでは crates.io からのCLIインストールを利用できます。Cargoでのビルドには Rust 1.90 以降、Git、Cコンパイラが必要です。[Windows、Linux、macOS向けのビルド済みバイナリとインストーラ](../../README.md#installation)もあります。

```sh
cargo install twokey --locked
```

保護したいリポジトリ内でポリシーを生成し、必要なルールのコメントを解除してパスを調整します。

```sh
twokey init
```

サンプルルールはコメントアウトされています。必要なルールだけ有効にし、パスを調整してください。

```sh
twokey validate --policy .twokey.toml
twokey check --staged --policy .twokey.toml --dry-run
```

`--dry-run` はブロックせずに結果を表示します。強制適用の前にポリシーをGitにコミットしてください。ポリシーと連携設定の組み込み保護、マーカー、シンボリックリンク・サブモジュール・モード変更のデフォルト制限は有効なままです。

### 完全に人間が書いた変更を宣言する

変更はデフォルトでAIによるものとして扱います。初期設定のマーカーは `#Human` で、`[ai].human_marker` で変更できます。完全に人間が書いた変更には、次のように指定します。

```toml
[ai]
human_marker = "#Human"
```

```sh
git commit -m "Fix token validation" -m "#Human"
```

コミットメッセージの末尾に空行を挟み、マーカーだけの行を追加します。Gitのエディタを使う場合は、`#Human` がコメントとして削除されないよう `git config core.commentChar ';'` を設定してください。明示的なAIシグナルが優先され、後続の人間による宣言で同じファイルの過去のAI変更を取り消すことはできません。宣言は執筆者の証明ではありません。

## ドキュメントと貢献

このページは導入部分の翻訳です。[英語のドキュメント](../../README.md)を正式な参照先としてください。[設定](../configuration.md)、[GitHub 連携](../github-actions.md)、[セキュリティモデル](../security.md)も参照できます。ローカルのフックは回避できるため、強制適用には信頼できる CI とブランチ保護が必要です。翻訳の修正も歓迎します。

メンテナー: Nick Darnell。[CC0 1.0 Universal](../../LICENSE) で公開されています。依存ライブラリにはそれぞれのライセンスが適用されます。
