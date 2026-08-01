# NFR Requirements: neatnik-cli

大部分の非機能要件はrequirements.md(NFR-1〜NFR-PBT)で既に確定済みである。本ドキュメントはNFR Requirementsステージとして、それらを技術スタック選定と紐づけて再整理し、拡張機能(Security/PBT)のコンプライアンス評価を行う。

## 1. パフォーマンス
- 数値目標なし、体感速度基準(requirements.md NFR-6, Q D3=A)
- `walkdir`によるストリーミング走査を用い、大量ファイルでもメモリに全件保持しない設計とする

## 2. 可用性・災害復旧
- **該当なし**。単一バイナリのCLIツールであり、常時稼働するサービスではないため、可用性SLA・フェイルオーバー・DRの概念は適用されない
- Resiliency Baseline拡張は無効(requirements.md NFR-Resiliency、Q Resiliency=No)

## 3. セキュリティ
requirements.md NFR-Securityの評価を踏襲し、技術スタックとの対応を明記する。

| SECURITY Rule | 判定 | 根拠 |
|---|---|---|
| SECURITY-01〜02, 04〜08, 11〜12, 14 | N/A | データストア・ネットワークサービス・Web API・認証機構を持たないローカルCLIのため |
| SECURITY-03(アプリケーションログ) | 対応 | `tracing`+`tracing-subscriber`(JSON)。シークレット等を出力しない(NFR-3) |
| SECURITY-09(ハードニング) | 対応 | エラー出力に内部パス・スタックトレースを含めない設計(business-logic-model.md 5章)。依存は`cargo-deny`で最新性を監視 |
| SECURITY-10(サプライチェーン) | 対応 | `Cargo.lock`をコミット、`cargo-deny`による脆弱性・ライセンスチェック(Q T3=A)。YAMLクレートは`serde_yaml`(archived)・`serde_yml`(RUSTSEC-2025-0067、archived)を避け`serde_norway`を採用 |
| SECURITY-13(整合性) | 対応 | `serde`による型付きデシリアライズ(スキーマ外構造を許容しない)。削除ログは実行日時・対象ファイル一覧を含み監査可能(NFR-3) |
| SECURITY-15(例外処理) | 対応 | `anyhow`/`thiserror`による明示的エラーハンドリング、fail closed設計(business-logic-model.md 5章) |

## 4. テスト戦略(Property-Based Testing、Partial適用)
| PBT Rule | 判定 | 根拠 |
|---|---|---|
| PBT-01 | 対応(Functional Designで実施済み) | business-rules.md 8章「テスト可能プロパティ」 |
| PBT-02, 03, 07, 08(Partial適用でブロッキング) | N/A(本ステージ対象外) | 実際のテストコード作成はCode Generationステージで評価する |
| PBT-09(フレームワーク選定) | 対応 | `proptest`を採用(tech-stack-decisions.md) |
| PBT-04, 05, 06, 10(非ブロッキング) | 該当時に考慮 | Code Generation計画時に個別評価する |

## 5. 信頼性
- 全ての外部I/O呼び出しに明示的エラーハンドリング(SECURITY-15と共通)
- ジョブ単位のロック(`fd-lock`)により多重起動を防止(FR-8)
- dry-run・セーフティブレーキによる誤操作防止(NFR-1)

## 6. 保守性
- `rustfmt`/`clippy`による一貫したコードスタイルと静的解析
- lib+bin構成(requirements.md 2.1)により、業務ロジックをテスト容易な形で分離
- `assert_cmd`/`predicates`/`tempfile`によるCLI統合テスト、`proptest`によるプロパティテスト、標準の`cargo test`による例示テストを組み合わせる(PBT-10: 両者は補完関係)

## 7. ユーザビリティ
- `--help`の充実、`init`によるサンプル設定生成、引数なし実行時のウェルカムガイド(requirements.md FR-11)
- エラーメッセージは次のアクションを案内する(requirements.md FR-12)

## 8. 拡張機能設定サマリ
| 拡張機能 | 有効 | 本ステージでの扱い |
|---|---|---|
| Security Baseline | Yes | 上記3章で評価。適用ルールはすべて「対応」、非適用ルールはN/A |
| Property-Based Testing | Partial(PBT-02,03,07,08,09のみ強制) | 上記4章で評価。PBT-09は本ステージで対応済み、他はCode Generationで評価 |
| Resiliency Baseline | No | 適用しない |
