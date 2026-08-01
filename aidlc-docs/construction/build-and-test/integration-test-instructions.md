# Integration Test Instructions: neatnik-cli

## Purpose
本プロジェクトは単一ユニット(単一クレート)構成であり、マイクロサービス間のような別プロセス・別サービス間の結合テストは存在しない。ここでの「結合テスト」は、CLIバイナリを実際に起動し、複数モジュール(`config`→`scan`→`archive`/`relocate`/`delete`→`pipeline`)が正しく連携して動作することを確認するテストを指す。

## Test Scenarios

### Scenario 1: 設定ファイル読込 → バリデーション → CLI出力
- **Description**: 実在する/しない設定ファイルに対し、`validate`/`list`コマンドが適切なメッセージ・終了コードを返すこと
- **Setup**: `tempfile`で一時ディレクトリ・設定ファイルを作成
- **Test Steps**: `tests/cli.rs`の`validate_*`/`list_shows_configured_jobs`を参照
- **Expected Results**: 設定ファイル不在時はBR-5に従い`neatnik init`を提案するメッセージ、正常時は`configuration is valid`を出力
- **Cleanup**: `tempfile::tempdir()`のDropにより自動削除

### Scenario 2: scan → archive → relocate → delete のカスケード結合
- **Description**: 実ファイルシステム上で、猶予日数の条件を満たすファイルがアーカイブ→退避→削除まで1回の`run`実行内で正しくカスケードすること(BR-9)
- **Setup**: 経過日数の異なるログファイルを`tempfile`ディレクトリに作成し、`filetime`でmtimeを調整
- **Test Steps**: `tests/cli.rs`の`run_executes_the_full_pipeline_in_one_pass`、および`src/pipeline.rs`内の結合的な単体テスト(`pipeline::tests::*`、モジュール内結合のため厳密には単体テストに分類されるが、config/scan/archive/relocate/delete全モジュールを結合して検証している)
- **Expected Results**: 猶予日数を満たすファイルが圧縮され、退避先へ移動し、削除猶予も満たす場合は同一実行内で削除される。中間ファイルが残らない
- **Cleanup**: `tempfile::tempdir()`のDropにより自動削除

### Scenario 3: dry-run による無変更確認
- **Description**: `--dry-run`指定時、実際のファイル操作を一切行わずに対象件数・合計サイズのみ報告すること(NFR-1)
- **Setup**: Scenario 2と同様のファイル配置
- **Test Steps**: `tests/cli.rs`の`run_dry_run_reports_counts_without_touching_files`
- **Expected Results**: サマリの件数はScenario 2と同じだが、実行後もファイルは元の場所に残っている
- **Cleanup**: 自動

## Setup Integration Test Environment

外部サービス(データベース・Docker等)への依存はない。ローカルファイルシステム上の一時ディレクトリのみで完結する。

### 1. 依存ツールの準備
```bash
# assert_cmd/predicates/tempfileはdev-dependenciesとして宣言済み、追加インストール不要
cargo build --release  # tests/cli.rs はビルド済みバイナリ(cargo_bin)を起動して検証する
```

## Run Integration Tests

### 1. Execute Integration Test Suite
```bash
cargo test --test cli
```

### 2. Verify Service Interactions(本プロジェクトでは「モジュール間連携」の意)
- **Test Scenarios**: 14件(引数なしガイド、init生成/上書き拒否/force上書き、validate成功/失敗/`--now`形式エラー、list表示、run dry-run/実行/`--now`オーバーライド正常系・異常系、completions、`--version`)
- **Expected Results**: 全14件成功(2026-08-01時点で確認済み)
- **Logs Location**: `assert_cmd`はサブプロセスの標準出力・標準エラーをキャプチャして`predicates`でアサーションする。失敗時は`cargo test`の出力にキャプチャ内容が表示される

### 3. Cleanup
`tempfile`クレートが生成した一時ディレクトリは各テスト関数のスコープ終了時に自動削除される。手動でのクリーンアップは不要。

## 参考: 手動E2E確認(デモスクリプト)
`demo/run-demo.sh`を実行すると、経過日数の異なる4ファイルに対する`validate`→`run --dry-run`→`run`の一連の流れを、実際のプロジェクトディレクトリ内(`demo/workspace/`、`.gitignore`対象)で目視確認できる。自動テストの補完として、レビュー・デモ用途に利用する。
