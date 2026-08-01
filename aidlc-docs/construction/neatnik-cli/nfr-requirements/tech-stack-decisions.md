# Tech Stack Decisions: neatnik-cli

## 言語・エディション
- **言語**: Rust
- **Edition/MSRV**: 最新の安定版edition・toolchainを使用し、MSRVは固定しない(Q T4=A)。Code Generation時点の最新安定版editionを採用する

## 依存クレート一覧

| クレート | 用途 | 選定理由・関連要件 |
|---|---|---|
| `clap`(derive feature) | CLIパーサー | FR-6, FR-12。deriveマクロでサブコマンド(run/validate/init/list/completions)を宣言的に定義 |
| `clap_complete` | シェル補完生成 | FR-12(`completions`サブコマンド)。clapと同じ作者が提供する標準的な組み合わせ |
| `serde`(derive feature) | シリアライズ/デシリアライズ基盤 | FR-7設定モデル全般 |
| `serde_norway` | YAML設定ファイルのパース | FR-7。`serde_yaml`はメンテナンス終了(archived)。調査の結果、RUSTSEC-2025-0067勧告で名指しで推奨され、代替候補中最多の実績(累計ダウンロード数)を持つ`serde_norway`を採用(Q T1、詳細はnfr-requirements-plan.md参照) |
| `walkdir` | ディレクトリ再帰走査 | FR-1(WatchTarget配下のファイル走査) |
| `glob` | globパターンマッチング | FR-1(include/exclude) |
| `flate2` | gzip圧縮 | FR-2(単体ファイル圧縮) |
| `tar` | tarアーカイブ生成 | FR-2(tar.gz、バンドル圧縮) |
| `zip` | zip圧縮 | FR-2 |
| `fd-lock` | クロスプラットフォーム対応アドバイザリファイルロック | FR-8(ジョブ多重起動防止)。Linux/macOS/Windowsを1つのAPIで抽象化(Q C2=A)。Unix版の書き込み中判定(BR-7)にも同じ機構を再利用する |
| `filetime` | ファイルのmtime設定(クロスプラットフォーム) | BR-9(アーカイブ出力・退避先ファイルへのmtime継承)。標準ライブラリには任意のmtimeを設定するAPIがないため追加(Code Generation Step 6で判明した抜け漏れ、NFR Requirements時点では未選定だった)。`tar`クレートの推移的依存として既にビルドグラフに含まれており、新規のサプライチェーン面の増加はない |
| `windows-sys` | Windows API直接呼び出し | BR-7(Windowsの共有モードオープン試行による書き込み中判定)。軽量なローレベルバインディングのため`windows`クレートより`windows-sys`を採用(Windows専用ターゲットでのみ依存) |
| `regex` | 正規表現 | BR-7.1(FilenameDateRuleのファイル名照合) |
| `chrono` | 日時計算・フォーマット | 基準日時計算全般、FR-2命名規則(YYYYMMDDTHHMMSSZ) |
| `chrono-tz` | IANAタイムゾーンデータベース | BR-10(バンドル期間境界のタイムゾーン計算) |
| `iana-time-zone` | OSのローカルタイムゾーン取得 | BR-10のデフォルト(ローカルタイムゾーン)実現のため(Q T2=A)。Rust標準では取得手段がないため追加 |
| `tracing` | 構造化ロギング | NFR-3 |
| `tracing-subscriber`(json feature) | ログ出力(JSON形式) | NFR-3 |
| `anyhow` | アプリケーション層のエラーハンドリング | 全体のエラー伝播(SECURITY-15) |
| `thiserror` | ドメインエラー型定義 | 全体のエラー型定義 |
| `proptest` | プロパティベーステスト | NFR-PBT(PBT-09: フレームワーク選定)。Rust向け標準的な選択 |

### 開発時のみ使用(dev-dependencies)
| クレート | 用途 |
|---|---|
| `assert_cmd` | CLI統合テスト(コマンド実行・出力検証) |
| `predicates` | `assert_cmd`と組み合わせるアサーション |
| `tempfile` | テスト用の一時ファイル・ディレクトリ生成 |

### CI/開発ツール(Cargo依存ではない)
| ツール | 用途 |
|---|---|
| `cargo-deny` | 依存クレートの脆弱性アドバイザリ・ライセンス・重複依存・バン対象クレートの一括チェック(Q T3=A、SECURITY-10) |
| `rustfmt` | コードフォーマット |
| `clippy` | 静的解析・lint |

## 採用しなかった選択肢
- `serde_yaml`: メンテナンス終了(archived)のため不採用
- `serde_yml`: 基盤ライブラリ`libyml`に未定義動作を引き起こす問題(RUSTSEC-2025-0067)がありアーカイブ済みのため不採用(調査により判明、当初の推奨から訂正)
- `rayon`: ジョブの並列実行は初回リリースでは行わない方針(FD-A2=C)のため不採用。将来必要になれば追加を検討
- `windows`(高レベルクレート): BR-7のWindows専用APIは1箇所の限定的な用途のため、より軽量な`windows-sys`を採用
