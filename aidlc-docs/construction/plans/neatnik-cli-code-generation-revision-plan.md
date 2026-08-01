# Code Generation Revision Plan: neatnik-cli

v0.1.1リリース後のFunctional Design改訂(`JobConfig`のstagesリスト化、`targets`のステージ別分離、BR-1撤回、`enabled`廃止、バンドル命名変更、BR-2.2追加、`basis: ctime`削除)を実装コードへ反映する。

## 参照元
- requirements.md(2026-08-02改訂: FR-1, FR-2, FR-5, FR-6, FR-7)
- Functional Design(2026-08-02複数回改訂: domain-entities.md, business-rules.md, business-logic-model.md)

## Steps

### Step 1: config モジュールの改修
- [ ] `WatchTarget`から`BasisKind::Ctime`を削除(`Mtime`/`FilenameDate`の2択に)
- [ ] `JobConfig`を`{name, stages: Vec<StageConfig>}`に変更
- [ ] `StageConfig`(タグ付き列挙型: `Archive(ArchiveConfig)`/`Relocate(RelocateConfig)`/`Delete(DeleteConfig)`)を新設
- [ ] `ArchiveConfig`: `enabled`削除、`name: String`(必須)追加、`targets: Vec<WatchTarget>`追加
- [ ] `RelocateConfig`: `enabled`削除、`targets: Vec<WatchTarget>`追加
- [ ] `DeleteConfig`: `enabled`削除、`targets: Vec<WatchTarget>`追加
- [ ] バリデーション改修: BR-1(削除)、BR-2(空`stages`は警告)、BR-2.1(各エントリの`targets`必須)、BR-2.2(バンドルモードのarchiveエントリ内でターゲット名重複禁止)
- [ ] 単体テスト・プロパティテストを更新(削除したロジックのテストを除去、新規ロジックのテストを追加)

### Step 2: scan モジュールの改修
- [ ] `BasisKind::Ctime`関連コード(`ctime_to_utc`等)を削除
- [ ] 関連テストを更新

### Step 3: archive モジュールの改修
- [ ] `ArchiveNamer::bundle_name`/`run_bundle`等の`job_name`パラメータを`archive_name`に改名(意味変更、シグネチャの型自体は変わらず)
- [ ] 関連テスト・コメントの文言更新

### Step 4: pipeline モジュールの全面改修
- [ ] `run_job_locked`を「固定archive→relocate→deleteフェーズ」から「`stages`を先頭から順に1エントリずつ処理」する方式に書き換え
- [ ] archive/relocateエントリは即時実行、deleteエントリはそのエントリ単位でセーフティブレーキ評価
- [ ] 単体テストを更新(複数stagesエントリ、任意順序、複数archiveエントリ等のシナリオを追加)

### Step 5: CLI(main.rs)・サンプル設定の更新
- [ ] `--job`フィルタ等、`JobConfig`構造変更に伴うコンパイルエラーを解消
- [ ] `config.example.en.yaml`/`config.example.ja.yaml`を新スキーマに合わせて全面改訂

### Step 6: ドキュメント・全体確認
- [ ] README.mdの設定ファイル節を新スキーマに合わせて更新
- [ ] `cargo test`・`cargo clippy --all-targets -- -D warnings`・`cargo fmt --check`で全件成功を確認
- [ ] `demo/run-demo.sh`が新スキーマで動作することを確認・必要なら修正
- [ ] `aidlc-docs/construction/neatnik-cli/code/code-generation-summary.md`を更新

## 完了基準
- 上記全ステップのチェックボックスが完了
- `cargo test`(lib + bin + 統合)全件成功、`cargo clippy -D warnings`・`cargo fmt --check`警告0件
- 手動E2E確認(`neatnik init`→`validate`→`run --dry-run`→`run`)で新スキーマの設定ファイルが期待通り動作することを確認
