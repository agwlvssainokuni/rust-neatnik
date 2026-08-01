# Unit Test Execution: neatnik-cli

## Run Unit Tests

### 1. Execute All Unit Tests
```bash
cargo test --lib
```
これには通常の例示テスト(`#[test]`)と、`proptest`によるプロパティベーステスト(PBT-02/03/07/08/09対応、各モジュールの`#[cfg(test)] mod tests`内)の両方が含まれる。

### 2. Review Test Results
- **Expected**: 74 tests passed, 0 failures(2026-08-02時点の実測値、`stages`リスト化改修後)
- **内訳**(モジュール別、代表例):
  - `config`: 10件(YAMLパース、BR-2/BR-2.1/BR-2.2/BR-4バリデーション、パストラバーサル対策)
  - `scan`: 12件(include/exclude、FilenameDateRule、書き込み中判定、往復性)
  - `archive`: 17件(ArchiveNamer/BundleKey決定性、単体/バンドル圧縮、往復性)
  - `relocate`: 10件(コピー、mtime/パーミッション保持、衝突解決)
  - `delete`: 7件(セーフティブレーキ閾値評価)
  - `lock`: 4件(ジョブロックの取得・競合・解放)
  - `notify`: 1件 / `clock`: 2件 / `error`: 2件
  - `pipeline`: 9件(ステージ独立スキャン、in_use/猶予未経過スキップ、dry-run、セーフティブレーキ、バンドル統合、複数ジョブ、別実行での退避先独立発見)
- **Test Coverage**: 数値目標は設定していない(NFR Requirementsで規定なし)。PBT-02/03/07/08/09の対象箇所(business-rules.md 8章のテスト可能プロパティ表)はすべてテスト実装済み(詳細は`aidlc-docs/audit.md`のStep 4/6/7/8/14、および2026-08-02のstages再設計の記録を参照)
- **Test Report Location**: 標準出力(CI環境では`cargo test --lib -- --format=json`等でレポート形式に変換可能。本プロジェクトでは追加のレポートツールは導入していない)

### 3. Fix Failing Tests
テストが失敗した場合:
1. `cargo test --lib -- --nocapture`で詳細な出力(`tracing`ログ含む)を確認する
2. `proptest`が失敗した場合は`proptest-regressions/`ディレクトリに再現用のシードが自動保存される(PBT-08)。同じ入力で再度失敗するか`cargo test`を再実行して確認できる
3. 該当モジュールのコードを修正し、`cargo test --lib`を再実行してすべて成功することを確認する

## 静的解析・フォーマットチェック
単体テストと合わせて、以下も実行することを推奨する(CI相当のチェック)。

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

2026-08-02時点でいずれも警告・差分0件であることを確認済み。
