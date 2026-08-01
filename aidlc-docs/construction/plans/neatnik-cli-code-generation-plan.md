# Code Generation Plan: neatnik-cli

## Unit Context
- **Unit**: `neatnik-cli`(単一ユニット。Units Generation/Application Designはスキップ)
- **参照元**: requirements.md、Functional Design(domain-entities.md, business-rules.md, business-logic-model.md)、NFR Requirements(nfr-requirements.md, tech-stack-decisions.md)、NFR Design(nfr-design-patterns.md, logical-components.md)
- **依存関係**: なし(Greenfield、単一クレート)
- **配置場所**: ワークスペースルート(`/Users/agawa/Documents/project/git/rust-neatnik/`)。アプリケーションコードは`aidlc-docs/`配下には置かない

## 横断的ルール(全ステップ共通)
- 新規作成する`.rs`ソースファイルには、先頭にApache License 2.0のヘッダーコメントを付与する(著作権者: `agwlvssainokuni`、年: `2026`固定)
- lib(`src/lib.rs`он以下)+ bin(`src/main.rs`)構成とする(requirements.md 2.1、logical-components.md)
- 各モジュールはPBT-01で識別済みのテスト可能プロパティ(business-rules.md 8章)を踏まえ、Partial適用でブロッキングの PBT-02/03/07/08/09 に該当する箇所は`proptest`によるプロパティテストを実装する

## Steps

### Step 1: Project Structure Setup
- [x] `Cargo.toml`作成(lib+binターゲット、tech-stack-decisions.mdの依存クレート一覧を反映)
- [x] `src/lib.rs`(公開モジュール宣言)、`src/main.rs`(スケルトン)を作成
- [x] `src/config.rs`, `src/clock.rs`, `src/scan.rs`, `src/archive.rs`, `src/relocate.rs`, `src/delete.rs`, `src/lock.rs`, `src/notify.rs`, `src/error.rs`, `src/pipeline.rs`の空モジュールファイルを作成
- [x] `deny.toml`(cargo-deny設定)を作成
- [x] `rustfmt.toml`を作成(デフォルト設定+明示化)

### Step 2: error モジュール
- [x] `thiserror`ベースのドメインエラー型を実装(各モジュール共通のエラー分類)

### Step 3: clock モジュール
- [x] `Clock`トレイト、`SystemClock`、`FixedClock`(`--now`用、FR-13)を実装
- [x] 単体テスト

### Step 4: config モジュール
- [x] `JobConfig`, `WatchTarget`, `ArchiveConfig`, `RelocateConfig`, `DeleteConfig`, `SafetyBrakeConfig`, `FilenameDateRule`を実装(domain-entities.md)
- [x] `serde_norway`によるYAMLパース
- [x] バリデーションロジック(BR-1: N1<=N2<=N3、BR-2: 全ステージ無効化警告、BR-4: 未知フィールド候補提示)。BR-3(バンドル冪等性)はarchiveモジュール(Step 6)、BR-5(設定ファイル不在時の案内)はCLI(Step 12)で対応
- [x] パストラバーサル対策(NFR-Design: basedir正規化・包含チェック)
- [x] 単体テスト + プロパティテスト(BR-1の大小関係検証、PBT-03)

### Step 5: scan モジュール
- [x] `FileCandidate`、`WriteGuardDetector`トレイトを実装
- [x] `UnixWriteGuardDetector`(flock検知+直近更新時刻ヒューリスティック、BR-7)
- [x] `WindowsWriteGuardDetector`(`windows-sys`による共有モードオープン試行、BR-7)。`cfg(windows)`のためmacOS開発環境ではビルド未検証、Build and Testステージでの検証が必要
- [x] `FilenameDateRule`の順次照合ロジック(BR-7.1、basenameのみに照合)
- [x] include/exclude評価(BR-6)
- [x] 単体テスト

### Step 6: archive モジュール
- [x] `ArchiveNamer`(単体ファイル命名 BR-8、バンドル命名 BR-8)
- [x] `BundleKey`(期間キー計算、タイムゾーン対応、BR-10)
- [x] 圧縮実行(`flate2`/`tar`/`zip`、アトミック書き込み BR-2)
- [x] mtime継承(BR-9)、バンドル冪等性のmtime比較判定(BR-3、`on_stale_bundle_member`)。mtime設定に`filetime`クレートを追加(tech-stack-decisions.md補正、`tar`の既存推移的依存を採用)
- [x] 単体テスト + プロパティテスト(`ArchiveNamer`/`BundleKey`の決定性、PBT-02/03/07/08)

### Step 7: relocate モジュール
- [x] コピー処理(mtime/パーミッション保持、BR-11)
- [x] 衝突解決(`on_conflict`、BR-12)
- [x] 単体テスト

### Step 8: delete モジュール
- [x] 削除実行、`SafetyBrakeConfig`の閾値判定(BR-13)
- [x] dry-runモード(BR-14)
- [x] 単体テスト
- **既知の制約**: BR-13後半の「enforce発動後、人手解除まで次回実行も止め続ける」永続ロックは、具体的な形式・解除コマンドが未確定のため未実装(コード内に明記)。Build and Testまたは今後の要件確認で扱いを決める

### Step 9: lock モジュール
- [x] `JobLock`トレイトと`fd-lock`ベースの実装(BR-16)。所有権を返す`acquire()`ではなくクロージャ方式`with_lock()`を採用(実装上の簡潔さのため、domain-entities.mdの型は参考実装と明記済み)
- [x] 単体テスト

### Step 10: notify モジュール
- [x] `Notifier`トレイトの定義のみ(FR-10、実装なし)

### Step 11: pipeline モジュール
- [x] `run_job()`: 1ジョブのスキャン→カスケードパイプライン処理(business-logic-model.md 2-4章)
- [x] `run_all()`: 複数ジョブの逐次処理(BR-15)
- [x] 等号設定時のカスケード処理(BR-9)、バンドル処理単位の分岐(business-logic-model.md 4章)
- [x] ジョブサマリ集計(`JobSummary`)
- [x] 単体テスト
- **付随改修**: `archive::run_bundle`を`BundleGroupResult`に改修し、1グループの失敗が他グループを止めないようにした

### Step 12: CLI(bin/main.rs)
- [x] `clap`によるサブコマンド定義: `run`, `validate`, `init`, `list`, `completions`, `--version`(FR-6, FR-12)
- [x] 引数なし実行時のウェルカムガイド(FR-11)
- [x] `tracing`グローバルサブスクライバ初期化(JSON出力、NFR-3)
- [x] トップレベルエラーハンドリング・終了コード(NFR-Design 1章)
- [x] `--help`の充実(使用例を含む、FR-11)。`config.example.yaml`を暫定作成し`include_str!`で`init`に埋め込み(Step 13で正式整備)
- 手動E2E確認: 全サブコマンド、BR-5案内、dry-run不変更、実ファイルでのカスケード完走、`--now`オーバーライド

### Step 13: config.example.yaml とinitテンプレート
- [x] コメント付きサンプル設定を単一のテンプレート文字列として実装し、リポジトリ同梱の`config.example.yaml`と`neatnik init`の出力の両方に使う(FR-11)。`include_str!`で埋め込み、出力が完全一致することを`diff`で確認済み

### Step 14: プロパティベーステストの整備
- [ ] `proptest`のジェネレータ(ドメイン型用、PBT-07)を整備
- [ ] シュリンク・シード再現性の確認(PBT-08、テスト実行時にseedをログ出力)
- [ ] PBT-02(往復性)/PBT-03(不変条件)対象箇所(business-rules.md 8章)のテストを実装

### Step 15: CLI統合テスト
- [ ] `assert_cmd`/`predicates`/`tempfile`による統合テスト: `init`でのファイル生成、`validate`の成功/失敗、`run --dry-run`、`--now`オーバーライド、引数なし実行時のガイド表示

### Step 16: ドキュメント生成
- [ ] `README.md`(概要、インストール、クイックスタート、コマンド一覧)を作成
- [ ] 主要な公開APIにrustdocコメントを付与
- [ ] `aidlc-docs/construction/neatnik-cli/code/`にコード生成サマリ(Markdown)を作成

### Step 17: ビルド設定
- [ ] `Cargo.toml`のreleaseプロファイル設定(最適化レベル等)
- [ ] 実際のビルド・テスト実行はBuild and Testステージで行う

## Story Traceability
User Storiesステージはスキップしているため、要件(FR-1〜FR-13, NFR-1〜NFR-PBT)およびビジネスルール(BR-1〜BR-17)への対応をトレーサビリティとして用いる。各Stepの説明内に対応する要件/ルールIDを明記済み。
