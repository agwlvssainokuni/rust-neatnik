# NFR Design Plan: neatnik-cli

## Step 1: NFR Requirements分析
- [x] `aidlc-docs/construction/neatnik-cli/nfr-requirements/`(nfr-requirements.md, tech-stack-decisions.md)を確認

## Step 2: 設計対象
- [x] NFR設計パターンの整理(該当カテゴリのみ) — `nfr-design-patterns.md`
- [x] 論理コンポーネント構成(lib+bin構成の具体化) — `logical-components.md`

## Step 3: カテゴリ評価と質問

### カテゴリ評価(該当性の判断根拠)
- **Resilience Patterns**: 部分的に該当。リトライ戦略は未決定のため質問(D1)
- **Scalability Patterns**: **該当なし**。単一ユーザー・単一ホストで動作するローカルCLIバッチツールであり、水平スケーリングやスループット拡張の概念自体が存在しない。NFR-6(パフォーマンス、数値目標なし)で既に整理済み
- **Performance Patterns**: 既に決定済み(NFR Requirements: `walkdir`によるストリーミング走査でメモリに全件保持しない)。新規論点なし
- **Security Patterns**: 部分的に該当。脅威モデル(設定ファイルの信頼レベル)が未決定のため質問(D2)
- **Logical Components**: 該当。Functional Designで定義したトレース抽象(Clock/WriteGuardDetector/JobLock/Notifier等)を、lib+bin構成の具体的なモジュール境界に落とし込む(質問不要、AIが設計し`logical-components.md`で提示)

### Question D1: 一時的なI/Oエラーに対するリトライ戦略
ファイル操作(読み取り・書き込み・移動)で一時的なエラー(例: 他プロセスとの競合による瞬間的な権限エラー)が発生した場合、リトライしますか？

A) リトライしない。エラーは即座にそのファイルの処理失敗として記録し、次のファイルに進む(business-logic-model.mdの「1ファイルの失敗はジョブ全体を止めない」方針と一貫)

B) 限定的なリトライ(例: 数回、短い間隔)を行う

C) Other (please describe after [Answer]: tag below)

**AI推奨**: A — ローカルファイルシステム操作の大半のエラー(権限不足、ディスク容量不足等)はリトライで解消しない性質のものであり、リトライは複雑さの割に効果が薄い。次回実行時の再対象化(BR-7等)が実質的なリトライの役割を果たす

[Answer]: A

---

### Question D2: 設定ファイルの信頼レベル(脅威モデル)
本ツールは削除という不可逆な操作を行うため、設定ファイル(YAML)をどの信頼レベルで扱うか明確にしたいです。

A) 設定ファイルは運用者が管理する信頼済み入力として扱う(悪意ある入力を想定した防御的なパス検証は行わない)

B) 設定ファイルは(共有環境等で)改ざんされうる半信頼入力として扱い、`include`/`exclude`パターンや`destination`に`basedir`の外側へ抜け出す`..`等が含まれる場合は`validate`でエラーにする(パストラバーサル対策)

C) Other (please describe after [Answer]: tag below)

**AI推奨**: B — 実装コストは低い一方、削除・移動という破壊的操作を扱うツールとして、設定ミス(意図しない`../`混入等)による想定外ディレクトリへの影響を防げる。SECURITY-11(設計時点でのmisuse case考慮)の観点にも合致する

[Answer]: B

---

## Step 4: 回答受領後の進め方
回答後、`nfr-design-patterns.md`(該当カテゴリの設計パターン)と`logical-components.md`(lib+bin構成の論理コンポーネント設計)を生成します。
