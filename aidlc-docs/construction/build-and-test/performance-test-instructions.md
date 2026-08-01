# Performance Test Instructions: neatnik-cli

## Purpose
本プロジェクトは単一ユーザー・単一ホストで動作するローカルCLIバッチツールであり、Webサービスのような同時アクセス・スループット要件は存在しない(NFR Design: Scalability Patterns = 該当なし)。そのため、負荷試験・ストレステストの数値目標(応答時間・スループット・同時ユーザー数)は**設定しない**。

## Performance Requirements
- **Response Time**: 数値目標なし(要件定義段階でNFR-6として「特筆すべき性能要件なし」と確認済み)
- **Throughput**: 該当なし(バッチ処理であり、リクエスト/秒の概念がない)
- **Concurrent Users**: 該当なし(単一プロセス・単一実行)
- **Error Rate**: 該当なし(BR-13のセーフティブレーキ、BR-7の書き込み中スキップにより、エラー時は安全側にスキップする設計であり、エラー率の目標値ではなく「エラー時に安全に停止/スキップすること」自体が要件)

## 実施した性能面の設計確認

数値目標がないため負荷試験は実施しないが、以下の設計方針が実装に反映されていることをコードレビューで確認済み(NFR Design: Performance Patterns参照)。

### ストリーミングスキャン
- `src/scan.rs`の`scan_target`は`walkdir::WalkDir`のイテレータを直接消費しており、対象ファイル一覧を一括でメモリに保持しない
- バンドル圧縮(`src/archive.rs`の`run_bundle`)は期間キーごとのグルーピングが必要なため、その単位でのみ候補を保持する(全件ではない)

### 大量ファイルでの簡易確認(任意)
数値目標はないが、大量ファイルを配置した際に明らかな性能劣化がないことを確認したい場合は、以下の手順で任意に確認できる。

```bash
# 例: 10,000件のダミーファイルを用意して実行時間を計測する
mkdir -p /tmp/neatnik-perf-check/logs
for i in $(seq 1 10000); do
    echo "line" > "/tmp/neatnik-perf-check/logs/app-$i.log"
done

cargo build --release
time target/release/neatnik run --config <対応するconfig.yaml> --dry-run

rm -rf /tmp/neatnik-perf-check
```

**Status**: N/A(数値目標なし、Scalability Patterns該当なしと要件定義段階で確認済み)。上記の簡易確認は必要に応じて実施する任意のものであり、リリース判定の必須条件ではない。
