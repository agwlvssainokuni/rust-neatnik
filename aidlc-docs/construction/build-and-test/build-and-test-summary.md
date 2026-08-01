# Build and Test Summary: neatnik-cli

> **改訂(2026-08-02)**: `targets`をジョブ層からarchive/relocate/delete各ステージエントリ層へ移動し、`JobConfig`を`stages: Vec<StageConfig>`(任意順序・任意回数のリスト)へ再設計した改修に伴い、テスト件数・シナリオ説明・既知の制約を最新化。

## Build Status
- **Build Tool**: Cargo(rustc/cargo 1.97.0)
- **Build Status**: Success(デバッグ/リリースとも)
- **Build Artifacts**: `target/release/neatnik`(単一バイナリ)
- **Build Time**: リリースビルド 約20〜45秒(環境依存、LTO有効のため)

## Test Execution Summary

### Unit Tests
- **Total Tests**: 74
- **Passed**: 74
- **Failed**: 0
- **Coverage**: 数値目標なし(NFR Requirementsで規定なし)。business-rules.md 8章のテスト可能プロパティ(PBT-01)は全項目対応済み(N1<=N2<=N3関連プロパティは撤回、BR-2.1/BR-2.2関連プロパティを新設)
- **Status**: Pass

### Integration Tests(CLI統合テスト)
- **Test Scenarios**: 22
- **Passed**: 22
- **Failed**: 0
- **Status**: Pass

### Performance Tests
- **Response Time**: 数値目標なし(N/A、ローカルCLIバッチツールのためScalability Patterns該当なし)
- **Throughput**: N/A
- **Error Rate**: N/A(エラー時は安全側にスキップ/停止する設計自体が要件)
- **Status**: N/A(設計方針の確認のみ実施、performance-test-instructions.md参照)

### Additional Tests
- **Contract Tests**: N/A(マイクロサービス構成ではないため対象外)
- **Security Tests**: Partial(自動テストで確認済みの項目はPass。`cargo-deny`によるサプライチェーンチェックは未インストール環境のため未実行、CI/リリース前の実施を推奨。security-test-instructions.md参照)
- **E2E Tests**: Pass(`tests/cli.rs`のCLI統合テストがE2Eを兼ねる。加えて`demo/run-demo.sh`による手動E2E確認も実施済み)

## 静的解析・フォーマット
- `cargo clippy --all-targets -- -D warnings`: 警告0件
- `cargo fmt --check`: 差分0件(Build and Testステージでrustfmt未適用分を解消済み)
- `cargo doc --no-deps`: 警告0件

## 既知の制約(再掲、詳細はcode-generation-summary.md参照)
1. BR-13後半の永続セーフティブレーキ(enforce発動後の次回実行自動停止)は未実装
2. `keep_original: true`かつ既存アーカイブ/バンドルが退避済みの場合、既存ファイル不在チェックにより重複アーカイブが作成される可能性がある(FR-9設計上の既知の限界)
3. `cargo-deny`によるサプライチェーンチェックは本環境未インストールのため未実行
4. ステージ間で`targets`を共有しない設計(2026-08-02改修)のため、後段のステージが前段の出力を追跡できるかは利用者が各エントリの`targets`/`include`を正しく設定することに依存する(archiveの`include`が自身の出力拡張子まで含めてしまう自己参照リスクを含め、ドキュメント上「利用者の責任」として明記済み。BR-2.2のみ静的検証あり)

**解決済みの制約**: Windows版`WriteGuardDetector`(`cfg(windows)`)は、GitHub Actionsのリリースワークフローで実機ビルドを実施し成功を確認済み(2026-08-01)。

## Overall Status
- **Build**: Success
- **All Tests**: Pass(自動テストの範囲内。cargo-deny等、環境依存で未実行の項目を除く)
- **Ready for Operations**: Yes(上記の既知の制約を許容した上で)

## Next Steps
既知の制約(特にBR-13永続ブレーキ、cargo-deny実行、ステージ間targets設定の利用者責任範囲)について、Operationsフェーズ移行前にユーザーと方針を確認することを推奨する。
