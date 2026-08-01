# neatnik

ログ・作業ファイル・一時ファイルのハウスキーピング(**アーカイブ→退避→削除**)を自動化するCLIツールです。設定ファイル(YAML)でジョブを定義し、基準日時からの経過日数に応じて古いファイルを段階的に圧縮・保管フォルダへの移動・削除します。

## 特徴

- **3段階パイプライン**: アーカイブ(圧縮)→退避(保管フォルダへの移動)→削除。各段階は独立して有効/無効化(スキップ)できます
- **冪等性**: 同じジョブを複数回実行しても、既に処理済みのファイルに対して二重処理・エラーが発生しません
- **安全機構**: `--dry-run`によるプレビュー実行、削除件数・容量閾値によるセーフティブレーキ
- **クロスプラットフォーム**: Linux/macOS/Windowsで動作します(書き込み中ファイルの検出方式のみOS別に実装)
- **外部コマンド非依存**: 圧縮・ロック等はすべてRustクレートで実装しており、`zip`コマンド等の外部プロセスを呼び出しません

## インストール・ビルド

```sh
cargo build --release
```

生成されたバイナリは `target/release/neatnik` です。

## クイックスタート

```sh
# 1. サンプル設定ファイルを生成する
neatnik init

# 2. config.yaml を編集し、監視対象ディレクトリ・保管先等を実環境に合わせて修正する

# 3. 設定内容を検証する
neatnik validate

# 4. 実際にファイルを操作せず、何が起きるかを確認する
neatnik run --dry-run

# 5. 実行する
neatnik run
```

引数なしで `neatnik` を実行すると、上記の手順を案内するガイドが表示されます。

## コマンド一覧

| コマンド | 説明 |
|---|---|
| `neatnik run [--config <path>] [--job <name>] [--dry-run] [--now <RFC3339>]` | 設定ファイルに従いジョブを実行する。`--job`省略時は全ジョブを対象とする |
| `neatnik validate [--config <path>]` | 設定ファイルの構文・整合性(猶予日数の大小関係等)を検証する |
| `neatnik init [--output <path>] [--force]` | コメント付きのサンプル設定ファイルを生成する(既定の出力先: `config.yaml`) |
| `neatnik list [--config <path>]` | 設定済みジョブの一覧を表示する |
| `neatnik completions <shell>` | 指定シェル(bash/zsh/fish等)向けの補完スクリプトを生成する |
| `neatnik --version` | バージョンを表示する |

`--config`省略時は、カレントディレクトリの `config.yaml` を使用します。ファイルが見つからない場合は `neatnik init` の実行を提案するメッセージが表示されます。

### `--now` オプション

`run`/`validate`/`list`は共通で`--now <RFC3339形式の日時>`を受け付けます。指定すると、システムの実時刻の代わりに指定した日時を「現在時刻」として扱い、猶予日数の経過判定を行います。システムの時計を変更せずに、未来日時をエミュレートした動作確認ができます。

```sh
neatnik run --dry-run --now 2027-01-01T00:00:00Z
```

### 表示言語(`--lang`)

すべてのサブコマンドで共通の`--lang <en|ja>`オプションを受け付けます。省略時は`LC_ALL`/`LC_MESSAGES`/`LANG`環境変数を順に見て、値が`ja`で始まれば日本語、それ以外は英語になります。

```sh
neatnik --lang ja --help
LANG=ja_JP.UTF-8 neatnik run --dry-run
```

対象はCLIが表示するヘルプ・ウェルカムガイド・サマリ出力・CLI固有のエラー案内(設定ファイル不在時の案内等)です。以下は対象外です。

- `clap`自体が生成する構造テキスト(`Usage:`, `Options:`, `Print help`, `Print version`等) — clapに日本語化の仕組みがないための制約
- 設定ファイルのバリデーションエラーメッセージ本文(`neatnik::config`等のライブラリ層が生成するもの) — 技術的な詳細情報のため英語のまま

## 設定ファイル

設定ファイル(YAML)は`jobs`のリストとして複数のジョブを定義します。各ジョブは以下から構成されます。

- **targets**: 監視対象ディレクトリ(`basedir`)と、その配下の対象ファイルを指定する`include`/`exclude`のglobパターン(複数指定可)
- **archive**: 圧縮設定(猶予日数、形式`gzip`/`zip`/`tar.gz`、単体/バンドル圧縮、元ファイル保持の有無)
- **relocate**: 退避設定(猶予日数、保管先、ディレクトリ構造、同名ファイル衝突時の挙動)
- **delete**: 削除設定(猶予日数、セーフティブレーキ)

完全な例は [`config.example.ja.yaml`](./config.example.ja.yaml)(日本語コメント)または [`config.example.en.yaml`](./config.example.en.yaml)(英語コメント)を参照してください。`neatnik init`は`--lang`/`LANG`環境変数に応じてどちらかと同一内容を出力します。

**猶予日数の制約**: 有効化されているステージ同士でのみ、アーカイブ猶予日数(N1) <= 退避猶予日数(N2) <= 削除猶予日数(N3) である必要があります(等号は許容し、同一実行内でのカスケード処理として扱われます)。

## アーキテクチャ

lib(`src/lib.rs`以下)+ bin(`src/main.rs`)構成です。CLIをメインターゲットとしつつ、ファイル走査・アーカイブ命名・ロック等の再利用性の高い部品はライブラリのモジュール(`config`/`scan`/`archive`/`relocate`/`delete`/`lock`/`clock`/`notify`/`error`/`pipeline`)として分離しています。詳細は`cargo doc --open`で生成されるAPIドキュメント、および`aidlc-docs/construction/neatnik-cli/`配下の設計ドキュメントを参照してください。

## 開発

```sh
# 単体テスト・プロパティテスト
cargo test --lib

# CLI統合テスト
cargo test --test cli

# 静的解析
cargo clippy --all-targets

# フォーマット
cargo fmt
```

## リリース

`main`ブランチで`Cargo.toml`の`version`を更新し、同じバージョンの`vX.Y.Z`タグをpushすると、GitHub Actions(`.github/workflows/release.yml`)がLinux(x86_64)/macOS(x86_64, aarch64)/Windows(x86_64)向けのリリースビルドを作成し、GitHub Releaseにバイナリを添付します。

```sh
git tag v0.1.0
git push origin v0.1.0
```

タグのバージョンと`Cargo.toml`の`version`が一致しない場合、ワークフローは(ビルドを行わずに)失敗します。手動でのワークフロー起動には対応していません(タグpushのみがトリガーです)。

## ライセンス

Apache License, Version 2.0. 詳細は[LICENSE](./LICENSE)を参照してください。
