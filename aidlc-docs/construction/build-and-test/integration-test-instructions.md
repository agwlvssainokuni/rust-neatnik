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

### Scenario 2: archive → relocate → delete のステージ独立スキャンによる結合
- **Description**: `stages`リストに列挙したarchive/relocate/deleteの各エントリは、前段の結果をメモリ越しに引き継がず、自身の`targets`を独立にディスクスキャンする(BR-9)。実ファイルシステム上で、猶予日数の条件を満たすファイルが1回の`run`実行内で正しく連鎖処理されることを確認する。同一実行内で連鎖するのは、archiveが実際にディスクへ書き込んだ出力(例: `*.gz`)を、後続のrelocateエントリの`targets`が独立スキャンで発見できるためであり、メモリ上の受け渡しではない点に注意
- **Setup**: 経過日数の異なるログファイルを`tempfile`ディレクトリに作成し、`filetime`でmtimeを調整。relocate/deleteエントリの`targets`は、archive/relocateの出力先(拡張子・ディレクトリ)を明示的に指すよう設定する
- **Test Steps**: `tests/cli.rs`の`run_executes_the_full_pipeline_in_one_pass`、および`src/pipeline.rs`内の結合的な単体テスト(`pipeline::tests::*`、モジュール内結合のため厳密には単体テストに分類されるが、config/scan/archive/relocate/delete全モジュールを結合して検証している)。特に`pipeline::tests::delete_finds_relocated_files_independently_in_a_later_run`は、archive+relocateを行うジョブと、それとは別の`JobConfig`(別実行相当)のdeleteジョブを順に実行し、deleteが自身の`targets`だけでrelocate先のファイルを独立に発見できることを検証する
- **Expected Results**: 猶予日数を満たすファイルが圧縮され、退避先へ移動し、削除猶予も満たす場合は同一実行内で削除される。中間ファイルが残らない。別実行に分かれた場合も、各エントリの`targets`がディスク上の出力を正しく指していれば追跡が継続する
- **Cleanup**: `tempfile::tempdir()`のDropにより自動削除

### Scenario 3: dry-run による無変更確認
- **Description**: `--dry-run`指定時、実際のファイル操作を一切行わずに対象件数・合計サイズのみ報告すること(NFR-1)。ステージ独立スキャンの設計上、dry-runはarchiveの出力を実際にはディスクへ書き込まないため、後続のrelocate/deleteエントリは自身の`targets`で何も発見できず0件になる(意図した挙動であり、バグではない)
- **Setup**: Scenario 2と同様のファイル配置
- **Test Steps**: `tests/cli.rs`の`run_dry_run_reports_counts_without_touching_files`
- **Expected Results**: archiveの対象件数のみ報告され、relocate/deleteは0件。実行後もファイルは元の場所に残っている
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
- **Test Scenarios**: 22件(引数なしガイド、init生成/上書き拒否/force上書き/英語・日本語コメント生成、validate成功/失敗/`--now`形式エラー/設定ファイル不在案内の多言語化、list表示、run dry-run/実行/`--now`オーバーライド正常系・異常系、completions、`--version`、`--lang`/`LANG`環境変数による表示言語切替6件)
- **Expected Results**: 全22件成功(2026-08-02時点、`stages`リスト化改修後に確認済み)
- **Logs Location**: `assert_cmd`はサブプロセスの標準出力・標準エラーをキャプチャして`predicates`でアサーションする。失敗時は`cargo test`の出力にキャプチャ内容が表示される

### 3. Cleanup
`tempfile`クレートが生成した一時ディレクトリは各テスト関数のスコープ終了時に自動削除される。手動でのクリーンアップは不要。

## 参考: 手動E2E確認(デモスクリプト)
`demo/run-demo.sh`を実行すると、経過日数の異なる4ファイルに対する`validate`→`run --dry-run`→`run`の一連の流れを、実際のプロジェクトディレクトリ内(`demo/workspace/`、`.gitignore`対象)で目視確認できる。自動テストの補完として、レビュー・デモ用途に利用する。
