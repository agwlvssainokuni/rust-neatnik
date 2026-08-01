# Security Test Instructions: neatnik-cli

Security Baseline拡張が有効(Requirements Analysis時点でOptIn)。適用対象ルール(SECURITY-03, 09, 10, 13, 15)に対応するテスト・確認手順をまとめる。他のSECURITY-*ルールは、本プロジェクトが認証情報・ネットワーク通信・Webエンドポイントを扱わないローカルCLIであるためN/A(nfr-requirements.md参照)。

## 1. 依存クレートの脆弱性・ライセンス・重複依存チェック(SECURITY-10)

`deny.toml`を設定済み。`cargo-deny`は本開発環境には未インストールのため、以下の手順で導入・実行する(CI/CDまたはリリース前に実施することを推奨)。

```bash
cargo install cargo-deny --locked
cargo deny check
```

**Expected**: `advisories`(既知の脆弱性)・`licenses`(承認済みライセンスのみ)・`bans`(禁止クレート・重複バージョン)・`sources`(未知のレジストリ/git依存)のすべてで違反0件。`serde_yaml`/`serde_yml`(既知の問題により不採用、tech-stack-decisions.md参照)が依存グラフに含まれないことも確認する。

代替として`cargo audit`(RustSecアドバイザリDBのみを見る、より軽量)も利用可能:
```bash
cargo install cargo-audit --locked
cargo audit
```

## 2. パストラバーサル対策の検証(SECURITY-13関連、脅威モデルQ D2=B)

`WatchTarget.basedir`の正規化・包含チェックは以下のテストで自動検証済み:
- `src/config.rs`: `is_within_basedir_detects_traversal`
- `src/scan.rs`: `scan_target_finds_matching_files_and_skips_symlinks_and_excludes`(シンボリックリンク除外を含む)

追加の手動確認をしたい場合は、`include`パターンに`../`を含む設定やシンボリックリンク経由でのbasedir脱出を試み、`neatnik run --dry-run`で対象外として警告ログに記録されることを確認する。

## 3. `unsafe`コードのレビュー(SECURITY-15関連)

本プロジェクトで`unsafe`を使用しているのは`src/scan.rs`の`windows_impl`モジュール(`cfg(windows)`)のみ。`CreateFileW`によるファイルオープン試行で、内容・タイムスタンプを変更しないことをコメントで明記済み。開発環境(macOS)では直接ビルドできないが、GitHub Actionsのリリースワークフロー(`.github/workflows/release.yml`)でWindows(x86_64-pc-windows-msvc)実機ビルドを実施し、成功を確認済み(2026-08-01、詳細は`aidlc-docs/audit.md`参照)。ローカルでクロスコンパイル確認したい場合は以下を実行する。

```bash
cargo build --release --target x86_64-pc-windows-msvc
cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
```

## 4. ロギングでのシークレット非混入確認(SECURITY-03)

本ツールは認証情報・APIキー等を一切扱わないため、ログにシークレットが混入するリスクは構造的に存在しない。`tracing`のJSON出力(`src/main.rs`)にはファイルパス・エラーメッセージのみが含まれることをコードレビューで確認済み。

## 5. グローバルエラーハンドラでの内部情報非露出確認(SECURITY-15)

`src/main.rs`の`main()`は、失敗時にユーザー向けメッセージ(`eprintln!`)とtracing経由の詳細ログを分離している。スタックトレースや内部実装の詳細がユーザー向け標準エラー出力に漏れないことを`tests/cli.rs`の各種`.failure().stderr(...)`アサーションで確認済み。

## まとめ
| チェック項目 | 状態 |
|---|---|
| cargo-deny(脆弱性/ライセンス/重複依存) | 未実行(未インストール環境)。CI/リリース前に実行を推奨 |
| パストラバーサル対策 | 自動テストで確認済み |
| unsafeコードレビュー | レビュー済み(Windows実機ビルドもGitHub Actionsで確認済み) |
| ログへのシークレット非混入 | 該当リスクなし(構造的に確認済み) |
| エラーメッセージの内部情報非露出 | 自動テストで確認済み |
