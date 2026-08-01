# Code Generation Summary: neatnik-cli

`neatnik-cli-code-generation-plan.md`(17ステップ)に基づき生成したコードの要約。実装コード本体はワークスペースルート(`src/`, `tests/`, `Cargo.toml`等)にあり、本ファイルは生成物の見取り図としてのみ機能する。

> **改訂(2026-08-02)**: `neatnik-cli-code-generation-revision-plan.md`(6ステップ)に基づき、`targets`をジョブ層からarchive/relocate/delete各ステージエントリ層へ移動する再設計を実施。`JobConfig`は`stages: Vec<StageConfig>`(任意順序・任意回数のarchive/relocate/deleteエントリのリスト)を持つロックスコープに変わり、各ステージは前段の結果をメモリ越しに引き継がず自身の`targets`を独立にスキャンする。これに伴いBR-1(N1<=N2<=N3の閾値順序検証)を撤回、BR-2.1(targets必須)・BR-2.2(バンドルモードでのターゲット名重複禁止)を新設、`basis`から`ctime`を削除。以下の記述は改訂後の状態を反映済み。

## 生成物一覧

| 種別 | パス | 内容 |
|---|---|---|
| プロジェクト設定 | `Cargo.toml`, `Cargo.lock`, `deny.toml`, `rustfmt.toml` | lib+bin構成、依存クレート(tech-stack-decisions.md準拠) |
| ライブラリ | `src/lib.rs` | 公開モジュール宣言 |
| バイナリ | `src/main.rs` | clap CLI、ウェルカムガイド、tracing初期化、グローバルエラーハンドラ |
| モジュール | `src/error.rs` | 共通ドメインエラー型(thiserror) |
| モジュール | `src/clock.rs` | `Clock`/`SystemClock`/`FixedClock`(FR-13) |
| モジュール | `src/config.rs` | 設定モデル(`JobConfig.stages: Vec<StageConfig>`)・YAMLパース・バリデーション(FR-7、BR-2/2.1/2.2/4) |
| モジュール | `src/scan.rs` | ファイル走査・`WriteGuardDetector`(BR-6/7/7.1) |
| モジュール | `src/archive.rs` | `ArchiveNamer`/`BundleKey`、圧縮実行(BR-3/8/9/10) |
| モジュール | `src/relocate.rs` | 退避処理(BR-11/12) |
| モジュール | `src/delete.rs` | セーフティブレーキ・削除実行(BR-13/14、per-エントリ評価) |
| モジュール | `src/lock.rs` | `JobLock`/`FileJobLock`(BR-16、ロックスコープはジョブ単位のまま) |
| モジュール | `src/notify.rs` | `Notifier`トレイト定義のみ(FR-10) |
| モジュール | `src/pipeline.rs` | `run_job`/`run_all`、ステージ独立スキャン(BR-9、BR-15) |
| モジュール(bin専用) | `src/i18n.rs` | CLI表示メッセージの英語/日本語対応(`--lang`、`LANG`/`LC_ALL`自動判定) |
| テスト支援 | `src/test_support.rs` | proptest共通ジェネレータ(PBT-07) |
| 統合テスト | `tests/cli.rs` | CLI E2Eテスト(assert_cmd) |
| サンプル設定 | `config.example.en.yaml`, `config.example.ja.yaml` | `neatnik init`と共通(`include_str!`、`--lang`/`LANG`で切替) |
| ドキュメント | `README.md` | 利用者向けドキュメント |

## 要件・ビジネスルールのトレーサビリティ

各モジュールのソースコード中に、対応するFR/BR/PBT IDをコメントとして明記済み。詳細な対応関係はStep 2〜15の各コミットメッセージおよび`aidlc-docs/audit.md`を参照。

## テスト実績(最終)

- `cargo test --lib`: 74件(単体テスト+proptestプロパティテスト)
- `cargo test --bin neatnik`: 4件(i18nの`--lang`パース単体テスト)
- `cargo test --test cli`: 22件(CLI統合テスト、i18n関連6件を含む)
- `cargo clippy --all-targets -- -D warnings`: 警告0件
- `cargo fmt --check`: 差分0件
- `cargo doc --no-deps`: 警告0件

## 既知の制約・保留事項

実装過程で識別され、コード中にコメントで明記済みの既知の制約:

1. **BR-13後半の永続セーフティブレーキ**(`src/delete.rs`): `enforce: true`発動後、人手でロック解除するまで次回実行も自動的に止め続ける永続的な状態保持は、具体的なロックファイル形式・解除コマンドがFunctional/NFR Designで未確定のため未実装。現状は実行のたびに閾値を再評価する
2. **既存アーカイブ・バンドルの重複作成の可能性**(`src/archive.rs`): `keep_original: true`かつ、既に退避済みのアーカイブ/バンドルが元の作成場所から移動された後に再度スキャン対象になった場合、既存ファイルの存在チェックが「不在」と判定され、重複して新規アーカイブ/バンドルが作成される可能性がある。これはFR-9(存在チェックベースの冪等性)の設計上受け入れられた既知の限界であり、要件定義段階から一貫して許容されている
3. **i18n(`src/i18n.rs`)の対象範囲**: CLI層(ヘルプ・ウェルカムガイド・サマリ・CLI固有のエラー案内)のみが英語/日本語対応。`clap`自体が生成する構造テキスト(`Usage:`, `Options:`, `Print help`等)はclapに日本語化の仕組みがなく英語のまま。ライブラリ層(`neatnik::config`等)のバリデーションエラーメッセージ本文も英語のまま(技術的詳細情報のため、意図的なスコープ外)

**解決済みの制約**: Windows版`WriteGuardDetector`(`src/scan.rs`)は、GitHub ActionsのリリースワークフローでWindows実機ビルドを実施し、`windows-sys`のfeature不足によるビルドエラー(`E0432`)を修正済み(2026-08-01、詳細はaudit.md参照)。現在はLinux/macOS(x86_64, aarch64)/Windows全プラットフォームでビルド成功を確認済み。

## 依存クレートの変更(tech-stack-decisions.mdからの差分)

- `filetime`を追加(Step 6で判明。BR-9/BR-11のmtime設定に必要。`tar`クレートの既存の推移的依存であり新規サプライチェーン面の増加なし)
