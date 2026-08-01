# Code Generation Summary: neatnik-cli

`neatnik-cli-code-generation-plan.md`(17ステップ)に基づき生成したコードの要約。実装コード本体はワークスペースルート(`src/`, `tests/`, `Cargo.toml`等)にあり、本ファイルは生成物の見取り図としてのみ機能する。

## 生成物一覧

| 種別 | パス | 内容 |
|---|---|---|
| プロジェクト設定 | `Cargo.toml`, `Cargo.lock`, `deny.toml`, `rustfmt.toml` | lib+bin構成、依存クレート(tech-stack-decisions.md準拠) |
| ライブラリ | `src/lib.rs` | 公開モジュール宣言 |
| バイナリ | `src/main.rs` | clap CLI、ウェルカムガイド、tracing初期化、グローバルエラーハンドラ |
| モジュール | `src/error.rs` | 共通ドメインエラー型(thiserror) |
| モジュール | `src/clock.rs` | `Clock`/`SystemClock`/`FixedClock`(FR-13) |
| モジュール | `src/config.rs` | 設定モデル・YAMLパース・バリデーション(FR-7、BR-1/2/4) |
| モジュール | `src/scan.rs` | ファイル走査・`WriteGuardDetector`(BR-6/7/7.1) |
| モジュール | `src/archive.rs` | `ArchiveNamer`/`BundleKey`、圧縮実行(BR-3/8/9/10) |
| モジュール | `src/relocate.rs` | 退避処理(BR-11/12) |
| モジュール | `src/delete.rs` | セーフティブレーキ・削除実行(BR-13/14) |
| モジュール | `src/lock.rs` | `JobLock`/`FileJobLock`(BR-16) |
| モジュール | `src/notify.rs` | `Notifier`トレイト定義のみ(FR-10) |
| モジュール | `src/pipeline.rs` | `run_job`/`run_all`、カスケード処理(BR-9、BR-15) |
| テスト支援 | `src/test_support.rs` | proptest共通ジェネレータ(PBT-07) |
| 統合テスト | `tests/cli.rs` | CLI E2Eテスト(assert_cmd) |
| サンプル設定 | `config.example.yaml` | `neatnik init`と共通(`include_str!`) |
| ドキュメント | `README.md` | 利用者向けドキュメント |

## 要件・ビジネスルールのトレーサビリティ

各モジュールのソースコード中に、対応するFR/BR/PBT IDをコメントとして明記済み。詳細な対応関係はStep 2〜15の各コミットメッセージおよび`aidlc-docs/audit.md`を参照。

## テスト実績(最終)

- `cargo test --lib`: 73件(単体テスト+proptestプロパティテスト)
- `cargo test --test cli`: 14件(CLI統合テスト)
- `cargo clippy --all-targets`: 警告0件
- `cargo doc --no-deps`: 警告0件

## 既知の制約・保留事項

実装過程で識別され、コード中にコメントで明記済みの既知の制約:

1. **BR-13後半の永続セーフティブレーキ**(`src/delete.rs`): `enforce: true`発動後、人手でロック解除するまで次回実行も自動的に止め続ける永続的な状態保持は、具体的なロックファイル形式・解除コマンドがFunctional/NFR Designで未確定のため未実装。現状は実行のたびに閾値を再評価する
2. **Windows版`WriteGuardDetector`**(`src/scan.rs`): `windows-sys`による実装は`cfg(windows)`のためmacOS開発環境ではビルド未検証。Build and Testステージでの検証が必要
3. **既存アーカイブ・バンドルの重複作成の可能性**(`src/archive.rs`): `keep_original: true`かつ、既に退避済みのアーカイブ/バンドルが元の作成場所から移動された後に再度スキャン対象になった場合、既存ファイルの存在チェックが「不在」と判定され、重複して新規アーカイブ/バンドルが作成される可能性がある。これはFR-9(存在チェックベースの冪等性)の設計上受け入れられた既知の限界であり、要件定義段階から一貫して許容されている

## 依存クレートの変更(tech-stack-decisions.mdからの差分)

- `filetime`を追加(Step 6で判明。BR-9/BR-11のmtime設定に必要。`tar`クレートの既存の推移的依存であり新規サプライチェーン面の増加なし)
