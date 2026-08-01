# Logical Components: neatnik-cli

requirements.md 2.1(アーキテクチャ方針)のlib+bin構成を、Functional Designのドメインモデル(domain-entities.md)に基づき具体的なモジュール構成に落とし込む。

## モジュール依存関係

```
+-----------------------------------------------------+
|         bin: main.rs (CLI entrypoint)                |
|   run / validate / init / list / completions         |
+-----------------------------------------------------+
                          |
                          v
+-----------------------------------------------------+
|                 lib: pipeline                         |
|         run_job() / run_all()                         |
+-----------------------------------------------------+
                          |
                          v
+-----------------------------------------------------+
|  lib: config / scan / archive / relocate / delete /   |
|  lock                                                 |
+-----------------------------------------------------+
                          |
                          v
+-----------------------------------------------------+
|        lib: clock / notify / error (shared)           |
+-----------------------------------------------------+
```

### テキスト代替
```
bin (main.rs)
  -> lib::pipeline
       -> lib::config, lib::scan, lib::archive, lib::relocate,
          lib::delete, lib::lock
            -> lib::clock, lib::notify, lib::error (共有)
```

## モジュール一覧

| モジュール | 責務 | 主な型・関数 |
|---|---|---|
| `config` | 設定モデル定義・パース・バリデーション | `JobConfig`, `WatchTarget`, `ArchiveConfig`, `RelocateConfig`, `DeleteConfig`, `SafetyBrakeConfig`, `FilenameDateRule`。BR-1〜BR-5のバリデーション、NFR-Design 4章のパストラバーサル検証(basedir正規化・包含チェック) |
| `scan` | ファイル走査・基準日時決定・書き込み中判定 | `FileCandidate`, `WriteGuardDetector`(trait)、`UnixWriteGuardDetector`, `WindowsWriteGuardDetector`。BR-6, BR-7, BR-7.1 |
| `archive` | 圧縮・アーカイブ処理 | `ArchiveNamer`, `BundleKey`。単体/バンドル圧縮の実行、mtime継承(BR-8, BR-9, BR-10) |
| `relocate` | 退避処理 | コピー(mtime/パーミッション保持)、衝突解決(BR-11, BR-12) |
| `delete` | 削除処理・セーフティブレーキ | 削除実行、閾値判定(BR-13, BR-14) |
| `lock` | 多重起動防止 | `JobLock`(trait)。`fd-lock`ベースの実装(BR-16) |
| `clock` | 時刻抽象 | `Clock`(trait)、`SystemClock`、`FixedClock`(`--now`用、FR-13) |
| `notify` | 通知抽象(MVPでは未実装) | `Notifier`(trait)のみ定義(FR-10) |
| `error` | ドメインエラー型 | `thiserror`ベースの各モジュール共通エラー型。`anyhow`でアプリケーション層に伝播 |
| `pipeline` | 全体オーケストレーション | `run_job()`, `run_all()`。business-logic-model.mdの全体処理フロー・カスケード処理を実装。上記モジュールを組み合わせる |

## ライブラリ公開API方針
`lib.rs`は`pipeline`・`config`・`scan`の主要な型/関数のみを`pub`として公開し、内部実装(OS別の`WriteGuardDetector`実装等)はモジュール非公開とする。これにより、requirements.md 2.1で挙げた再利用性の高い部品(書き込み中ファイル検出、アトミック書き込み、経過日数ベースのファイル走査等)を将来的に独立クレートへ切り出す際の境界が明確になる。

## バイナリ(main.rs)の責務
- `clap`によるCLI引数パース(FR-6)
- `tracing`グローバルサブスクライバの初期化(NFR-3)
- サブコマンドに応じて`lib::pipeline`または個別のユーティリティ(`init`のサンプル生成、`list`のジョブ一覧表示、`completions`の補完生成)を呼び出す
- トップレベルのエラーハンドリング(NFR-Design 1章のグローバルエラーハンドラパターン)、終了コードの決定
