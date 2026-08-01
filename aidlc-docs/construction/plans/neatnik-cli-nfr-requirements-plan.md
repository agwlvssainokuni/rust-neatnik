# NFR Requirements Plan: neatnik-cli

## Step 1: Functional Design分析
- [x] `aidlc-docs/construction/neatnik-cli/functional-design/`(domain-entities.md, business-rules.md, business-logic-model.md)を確認
- [x] requirements.mdのNFR-1〜NFR-PBT、拡張機能設定(Security=Yes, PBT=Partial, Resiliency=No)を確認

## Step 2: 評価対象
- [x] 技術スタック(依存クレート)の確定 — `tech-stack-decisions.md`
- [x] NFR要件の確定(パフォーマンス・可用性・セキュリティ・信頼性・保守性) — `nfr-requirements.md`

## Step 3: 質問(下記の[Answer]タグに回答してください)

多くのNFRはrequirements.md(NFR-1〜NFR-PBT)で既に確定済みです。ここでは技術スタック選定に伴う新たな論点のみ質問します。

---

### Question T1: YAML設定ファイルのパースクレート
draft-spec.mdでは`serde_yaml`を想定していましたが、`serde_yaml`は原作者により**メンテナンス終了(archived)が表明されています**。どうしますか？

A) コミュニティフォークの`serde_yml`を採用する(APIはほぼ互換、開発継続中)

B) `serde_yaml`を採用する(機能を満たしており、枯れている。更新が止まっていることは許容する)

C) 他のYAMLクレート(`serde_norway`等の後継/派生)を調査して採用する

D) Other (please describe after [Answer]: tag below)

**AI推奨(訂正後)**: `serde_norway` — 当初推奨した`serde_yml`は、基盤の`libyml`に未定義動作を引き起こす問題(RUSTSEC-2025-0067)があり指摘後にアーカイブ済みであることが判明したため、推奨から除外。調査の結果、`serde_norway`(該当RUSTSEC勧告で名指しで推奨、3候補中最多の累計ダウンロード数)を採用する

[Answer]: `serde_norway`を採用(C、ただし調査の結果`serde_norway`に決定)

---

### Question T2: ローカルタイムゾーン取得の依存クレート
requirements.md(BR-10)で、バンドルの期間境界計算にデフォルトで「ローカルタイムゾーン」を使うことが決まっています。Rust標準ライブラリはOSのローカルタイムゾーンを取得する手段を持たないため、追加のクレート(`iana-time-zone` + `chrono-tz`)が必要になります。この方針でよいですか？

A) `iana-time-zone` + `chrono-tz`を追加し、ローカルタイムゾーンをデフォルトのまま維持する

B) 依存を増やしたくないので、デフォルトをUTCに変更する(ローカルタイムゾーンを使いたい場合は明示的に設定させる)

C) Other (please describe after [Answer]: tag below)

**AI推奨**: A — 依存クレートは軽量で広く使われており、既に合意した「デフォルトはローカルタイムゾーン」という要件を素直に実現できる

[Answer]: A

---

### Question T3: サプライチェーンセキュリティのツール(SECURITY-10関連)
依存クレートの脆弱性スキャン・ライセンスチェックについて、どのツールを採用しますか？

A) `cargo-deny`を採用する(脆弱性アドバイザリ・ライセンス・重複依存・バン対象クレートを一括チェック)

B) `cargo-audit`を採用する(脆弱性アドバイザリのチェックに特化、よりシンプル)

C) 両方採用する

D) 現時点では導入しない(Build and Testステージで別途検討する)

**AI推奨**: A — 単一ツールでSECURITY-10の複数観点(脆弱性・ライセンス・サプライチェーン)をカバーでき、CI設定もシンプルになる

[Answer]: A

---

### Question T4: Rust edition / MSRV(最低サポートRustバージョン)方針
新規プロジェクトのRust editionとMSRVについて、どうしますか？

A) 最新の安定版edition・toolchainを使い、MSRVは特に固定しない(個人開発の単一バイナリツールとして最新環境を前提とする)

B) MSRVを明示的に固定する(具体的なバージョンを[Answer]に記載してください)

C) Other (please describe after [Answer]: tag below)

**AI推奨**: A — 配布バイナリはビルド時の環境に依存し、外部ライブラリとして他プロジェクトから使われる想定も薄いため、MSRV固定の運用コストに見合うメリットが小さい

[Answer]: A

---

## Step 4: 回答受領後の進め方
全ての[Answer]に回答後、`nfr-requirements.md`(NFR要件の確定)と`tech-stack-decisions.md`(依存クレート一覧・選定理由)を生成します。
