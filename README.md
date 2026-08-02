# neatnik

ログ・作業ファイル・一時ファイルのハウスキーピング(**アーカイブ→退避→削除**)を自動化するCLIツールです。設定ファイル(YAML)でジョブを定義し、基準日時からの経過日数に応じて古いファイルを段階的に圧縮・保管フォルダへの移動・削除します。

## 特徴

- **柔軟なステージ構成**: アーカイブ(圧縮)・退避(保管フォルダへの移動)・削除の各ステージを、ジョブごとに任意の順序・任意の回数(0回以上)で並べられます。各ステージは自身が監視する対象(`targets`)を個別に持ち、書かれた順序どおりに実行されます
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
| `neatnik validate [--config <path>]` | 設定ファイルの構文・整合性(`targets`の必須性、バンドルモードでのターゲット名重複禁止等)を検証する |
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

設定ファイル(YAML)は`jobs`のリストとして複数のジョブを定義します。各ジョブは`name`(ロックのスコープ・識別子)と`stages`(archive/relocate/deleteエントリを並べた順序付きリスト)から構成されます。

- **stages**: `type: archive` / `type: relocate` / `type: delete`のいずれかを持つエントリのリスト。各種別は0回以上・任意の順序で書けますが、実行順は**書かれた順序どおり**です
- **targets**: 各ステージエントリが個別に持つ、監視対象ディレクトリ(`basedir`)と対象ファイルを指定する`include`/`exclude`のglobパターン(複数指定可)。ステージ間で共有しないため、後続ステージが前段の出力(例: archiveが作った`*.gz`、relocateの`destination`)を追跡したい場合は、そのステージ自身の`targets`でその出力パターン/ディレクトリを明示的に指定する必要があります
- **経過日数の判定**: どのステージも「基準日時からの経過日数 >= `after_days`」(閾値ちょうどの日数を含む)を対象条件とします。経過日数は実時間ベース(`現在時刻 - 基準日時`の合計秒数を86400で割った整数部分)で判定し、暦日の日付が変わったかどうかではありません
- **ロック**: ジョブ単位(`name`)で排他制御します。ロックファイルは設定ファイルと同じディレクトリに`.<job名>.lock`として作成されます。既にロック中の場合、そのジョブの実行はスキップされます(他のジョブの処理には影響しません)
- **書き込み中判定**: 他プロセスが書き込み中の可能性があるファイルはベストエフォートで検出し、そのステージの対象から除外します(Unix: アドバイザリロック検出 + 直近5秒以内に更新されたファイルの検出。Windows: 共有モードでのオープン試行による検出)

### `targets`(各ステージ共通)

| フィールド | 必須/既定 | 説明 |
|---|---|---|
| `basedir` | 必須 | 監視対象ディレクトリ。相対パスを指定した場合は実行時のカレントディレクトリが基準になる |
| `name` | 省略可(既定: `basedir`から自動導出) | バンドル命名(`<archive名>.<name>.<期間キー>.tar.gz`)に使う識別子 |
| `include` | 省略可、既定`[]` | 対象ファイルのglobパターン(`basedir`からの相対パスに対して評価、複数指定可)。空の場合は何もマッチしない |
| `exclude` | 省略可、既定`[]` | 除外するglobパターン。`include`より優先する |
| `basis` | 省略可、既定`mtime` | 基準日時(経過日数計算の起点)の情報源。`mtime`(ファイルの更新日時) \| `filename_date`(ファイル名から抽出) |
| `filename_date_rules` | 省略可、既定`[]` | `basis: filename_date`の場合のみ使用。上から順に照合し、最初にマッチ+日時のパースに成功したものを採用する |

`filename_date_rules`の各要素は、名前付きキャプチャ`(?P<date>...)`を含む正規表現`regex`(ファイル名本体に対して照合、ディレクトリ部分は含まない)と、`chrono`のstrftime形式の日時フォーマット`format`(例: `%Y-%m-%d`、`%Y%m%d`)を持ちます。

### archiveステージ(圧縮)

| フィールド | 必須/既定 | 説明 |
|---|---|---|
| `name` | 必須 | このarchiveエントリの識別子。バンドル命名に使う |
| `targets` | 必須(空不可) | 監視対象(上表`targets`参照) |
| `after_days` | 省略可、既定`0` | この日数以上経過したファイルを圧縮対象にする |
| `format` | 省略可、既定`gzip` | `gzip` \| `zip` \| `tar.gz`。バンドル圧縮(`bundle`がnone以外)では、複数ファイルをまとめられない`gzip`は`tar.gz`として扱われる(`gzip`と`tar.gz`は同じ結果になる)。`zip`を指定すると複数ファイルを1つのzipにまとめる |
| `bundle` | 省略可、既定`none` | `none`(ファイル1件ごとに個別圧縮) \| `daily` \| `weekly` \| `monthly`(同じ期間に属する複数ファイルを1つにまとめて圧縮) |
| `bundle_timezone` | 省略可、既定はローカルタイムゾーン | バンドルの期間境界(日次/週次/月次の区切り)を計算するIANAタイムゾーン名(例: `Asia/Tokyo`) |
| `keep_original` | 省略可、既定`false` | 圧縮後に元ファイルを残すか |
| `on_stale_bundle_member` | 省略可、既定`warn` | 既存バンドルより基準日時が新しいファイルが同じ期間に見つかった場合の挙動。`warn`(警告ログを出し、そのファイルはバンドルに含めずスキップする) \| `error`(エラーにしてそのバンドルグループの処理を止める) |

命名規則:
- 単体ファイル圧縮(`bundle: none`): `<元ファイル名>.<基準日時YYYYMMDDTHHMMSSZ>.<拡張子>`を元ファイルと同じディレクトリに作る
- バンドル圧縮: `<name>.<ターゲットのname>.<期間キー>.<拡張子>`をターゲットの`basedir`直下に作る。拡張子は`format`が`zip`なら`zip`、`gzip`/`tar.gz`なら`tar.gz`。期間キーは`daily`が`YYYY-MM-DD`、`weekly`が`YYYY-Www`(ISO週番号)、`monthly`が`YYYY-MM`
- いずれも冪等: 出力先に同名ファイルが既に存在する場合は再作成しない

### relocateステージ(退避)

| フィールド | 必須/既定 | 説明 |
|---|---|---|
| `targets` | 必須(空不可) | 監視対象(上表`targets`参照) |
| `after_days` | 省略可、既定`0` | この日数以上経過したファイルを退避対象にする |
| `destination` | 必須 | 移動先ディレクトリ |
| `layout` | 省略可、既定`preserve` | `preserve`(`basedir`からの相対パスを保持したまま`destination`直下に配置) \| `year_month`(基準日時の`YYYY/MM`配下に分類して配置) |
| `on_conflict` | 省略可、既定`rename` | 移動先に同名ファイルが既に存在する場合の挙動。`rename`(`_1`、`_2`...と連番を付けて衝突を避ける) \| `skip`(元ファイルを残したまま何もしない) \| `error`(エラーにしてジョブを止める) |

移動したファイルは、基準日時をmtimeとして引き継ぎます(Unixではパーミッションも保持されます)。これにより後続のdeleteステージが正しく経過日数を判定できます。

### deleteステージ(削除)

| フィールド | 必須/既定 | 説明 |
|---|---|---|
| `targets` | 必須(空不可) | 監視対象(上表`targets`参照) |
| `after_days` | 省略可、既定`0` | この日数以上経過したファイルを削除対象にする |
| `safety_brake.enforce` | 省略可、既定`false` | `true`にすると、閾値超過時にこのエントリの削除処理全体をブロックする(`false`の場合は閾値を超えても警告のみで削除は続行される) |
| `safety_brake.count_threshold` | 省略可(既定なし=無効) | 削除対象の件数がこの値を**超えたら**(超過。「以上」ではなく厳密に超えた場合)閾値超過とみなす |
| `safety_brake.size_threshold_gb` | 省略可(既定なし=無効) | 削除対象の合計サイズ(GB)がこの値を**超えたら**閾値超過とみなす |

閾値評価はdeleteエントリ単位(そのエントリの1回の実行で見つかった削除対象ファイル群全体)で行います。`count_threshold`・`size_threshold_gb`はどちらか一方でも超えれば閾値超過です。

完全な例は [`config.example.ja.yaml`](./config.example.ja.yaml)(日本語コメント)または [`config.example.en.yaml`](./config.example.en.yaml)(英語コメント)を参照してください。`neatnik init`は`--lang`/`LANG`環境変数に応じてどちらかと同一内容を出力します。1つ目のジョブ(`app-server-logs`)が単体ファイル圧縮(`bundle: none`)、2つ目のジョブ(`worker-batch-logs`)がバンドル圧縮(`bundle: daily`)の設定例です。

## ログ・実行エビデンス

`neatnik run`は出力を2つのストリームに分離しています。

- **標準出力**: `tracing`による構造化ログ(JSON、1行1イベント)。ハウスキーピングの実行結果(どのファイルをいつ圧縮・退避・削除したか)のエビデンスとして機能します。既定では`warn`以上のみ出力され、archived/relocated/deleted等の成功イベント(`info`レベル)を含めるには`RUST_LOG`環境変数で有効にする必要があります
- **標準エラー出力**: 人間向けの実況(ジョブごとの集計サマリ、警告、エラーメッセージ)

`RUST_LOG=info`で出力される成功イベントの例(実行結果、見やすさのため整形):

```text
{"timestamp":"2026-08-02T13:43:39.565728Z","level":"INFO","fields":{"message":"archived file","job":"demo-job","stage":"archive","path":"/data/logs/app.log","destination":"/data/logs/app.log.20191231T150000Z.gz","bytes":6,"format":"Gzip"},"target":"neatnik::pipeline"}
{"timestamp":"2026-08-02T13:43:39.582020Z","level":"INFO","fields":{"message":"archived bundle","job":"demo-job","stage":"archive","bundle":"/data/logs/workers.2019-12-31.tar.gz","member_count":2,"members":"[\"/data/logs/worker-2.log\", \"/data/logs/worker-1.log\"]","bytes":16},"target":"neatnik::pipeline"}
{"timestamp":"2026-08-02T13:43:39.583345Z","level":"INFO","fields":{"message":"relocated file","job":"demo-job","stage":"relocate","path":"/data/logs/app.log.20191231T150000Z.gz","destination":"/mnt/storage/app.log.20191231T150000Z.gz","bytes":26},"target":"neatnik::pipeline"}
{"timestamp":"2026-08-02T13:43:39.583903Z","level":"INFO","fields":{"message":"deleted file","job":"demo-job","stage":"delete","path":"/mnt/storage/app.log.20191231T150000Z.gz","bytes":26},"target":"neatnik::pipeline"}
```

共通フィールドは`timestamp`/`level`/`fields.message`/`job`(ジョブ名)/`fields.stage`(`archive`/`relocate`/`delete`)。ステージ・種別ごとの追加フィールド:

| イベント(`message`) | 追加フィールド |
|---|---|
| `archived file`(単体圧縮) | `path`(元ファイル)、`destination`(圧縮後のファイル)、`bytes`、`format` |
| `archived bundle`(バンドル圧縮) | `bundle`(バンドルファイル)、`member_count`、`members`(まとめた元ファイルのパス一覧)、`bytes`(合計) |
| `relocated file` | `path`(退避元)、`destination`(退避先)、`bytes` |
| `deleted file` | `path`、`bytes` |

`--dry-run`実行時は、上記と同じ内容のイベントに`dry_run: true`が付き、`message`も`"would archive file"`のように「would」表現になります(実際にはファイルを変更していないことを区別するため)。

```sh
# 成功イベントも含めて標準出力に構造化ログを出す
RUST_LOG=info neatnik run --config config.yaml

# cronで実行し、構造化ログをファイルに保存する例(logrotate等で別途ローテーションすること)
RUST_LOG=info neatnik run --config /etc/neatnik/config.yaml >> /var/log/neatnik/neatnik.log 2>&1
```

systemdサービスとして実行する場合、`RUST_LOG=info`を`Environment=`に設定すれば、標準出力はそのままjournaldに取り込まれます(`journalctl -u <unit> -o json`等で構造化ログを抽出可能)。標準エラー出力(サマリ)も同様にjournaldに記録されますが、`StandardOutput=`/`StandardError=`を分けて設定すれば構造化ログのみを別ファイルに切り出すこともできます。

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
