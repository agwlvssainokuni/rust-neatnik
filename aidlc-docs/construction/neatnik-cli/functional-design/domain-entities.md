# Domain Entities: neatnik-cli

技術非依存の概念モデルとして記述する(Rustの型はあくまで実装イメージの参考)。

## 設定モデル

### JobConfig
ジョブ単位(対象ディレクトリ×ルールの組)の設定。

| フィールド | 型 | 説明 |
|---|---|---|
| name | String | ジョブ名。ロックファイル名・バンドルアーカイブ名の一部にも使う |
| include | List\<GlobPattern\> | 対象ファイルパターン(複数可) |
| exclude | List\<GlobPattern\> | 除外パターン |
| basis | BasisKind(Mtime\|Ctime\|FilenameDate) | 基準日時の情報源。デフォルトMtime |
| archive | ArchiveConfig | アーカイブ段階の設定 |
| relocate | RelocateConfig | 退避段階の設定 |
| delete | DeleteConfig | 削除段階の設定 |

### ArchiveConfig
| フィールド | 型 | 説明 |
|---|---|---|
| enabled | bool | 有効/無効(スキップ可、FR-5) |
| after_days (N1) | u32 | アーカイブ猶予日数 |
| format | ArchiveFormat(Gzip\|Zip\|TarGz) | 圧縮形式(FR-2) |
| bundle | BundleKind(None\|Daily\|Weekly\|Monthly) | まとめ方針(FR-2) |
| bundle_timezone | Timezone | バンドル期間境界の計算に使うタイムゾーン(FD-B1) |
| keep_original | bool | 圧縮後に元ファイルを残すか(デフォルトfalse)。`bundle`の値に関わらず許可する(FD-B2、2回目の見直し)。バンドル圧縮時の冪等性はmtime比較方式(BR-3)で担保する |
| on_stale_bundle_member | OnStaleBundleMember(Warn\|Error) | バンドル対象ファイルのmtimeが既存バンドルのmtimeより新しい場合の挙動(BR-3)。デフォルトWarn |

### RelocateConfig
| フィールド | 型 | 説明 |
|---|---|---|
| enabled | bool | 有効/無効(スキップ可) |
| after_days (N2) | u32 | 退避猶予日数 |
| destination | PathBuf | 保管先ディレクトリ |
| layout | LayoutKind(Preserve\|YearMonth) | 移動先ディレクトリ構造 |
| on_conflict | ConflictPolicy(Rename\|Skip\|Error) | 同名ファイル衝突時の挙動 |

### DeleteConfig
| フィールド | 型 | 説明 |
|---|---|---|
| enabled | bool | 有効/無効(スキップ可) |
| after_days (N3) | u32 | 削除猶予日数 |
| safety_brake | SafetyBrakeConfig | セーフティブレーキ設定 |

### SafetyBrakeConfig
| フィールド | 型 | 説明 |
|---|---|---|
| enforce | bool | 閾値超過時に処理を停止するか(true)、ログ・通知のみに留めるか(false)(要件Q C1=C) |
| count_threshold | Option\<u64\> | 削除件数の閾値 |
| size_threshold_gb | Option\<f64\> | 削除容量の閾値 |

## 実行時コンテキスト

### CliInvocation
CLI引数から構築される実行コンテキスト。

| フィールド | 型 | 説明 |
|---|---|---|
| command | Command(Run\|Validate\|Init\|List\|Completions) | サブコマンド(FR-6) |
| config_path | Option\<PathBuf\> | `--config` |
| job_filter | Option\<String\> | `--job` |
| dry_run | bool | `--dry-run` |
| now_override | Option\<DateTime\> | `--now`(FR-13) |

### Clock(トレイト/抽象)
| メソッド | 説明 |
|---|---|
| now() -> DateTime | 「現在時刻」を返す。`SystemClock`(実時刻)と`FixedClock`(`--now`指定値)の2実装を持つ(FR-13) |

### FileCandidate
スキャンで検出されたファイル1件を表す。

| フィールド | 型 | 説明 |
|---|---|---|
| path | PathBuf | ファイルパス |
| basis_datetime | DateTime | 基準日時(JobConfig.basisに従い決定) |
| size_bytes | u64 | ファイルサイズ |
| in_use | bool | 書き込み中と判定されたか(NFR-OS、OS別ロジック) |

### WriteGuardDetector(トレイト/抽象)
| メソッド | 説明 |
|---|---|
| is_in_use(path) -> bool | 他プロセスが書き込み中かをベストエフォートで判定(NFR-OS)。`UnixWriteGuardDetector`(flock検知+直近更新時刻)と`WindowsWriteGuardDetector`(共有モードオープン試行)の2実装 |

### JobLock(トレイト/抽象)
| メソッド | 説明 |
|---|---|
| acquire(job_name) -> Result\<LockGuard\> | 多重起動防止のアドバイザリファイルロックを取得(FR-8)。クロスプラットフォーム対応クレート(`fd-lock`等)で実装し、ロックファイルは設定ファイルと同じディレクトリに`.<job-name>.lock`として作成(FD-A3) |

### Notifier(トレイト/抽象、MVPでは未実装)
| メソッド | 説明 |
|---|---|
| notify(event: NotificationEvent) | エラー・セーフティブレーキ発動等の通知(FR-10)。MVPではtraitのみ定義し、具体実装(メール/Slack等)は行わない |

## 処理結果モデル

### StageOutcome
1ファイル・1ステージの処理結果。

| フィールド | 型 | 説明 |
|---|---|---|
| file | FileCandidate | 対象ファイル |
| stage | StageKind(Archive\|Relocate\|Delete) | 実行されたステージ |
| status | OutcomeStatus(Processed\|Skipped\|Failed) | 結果 |
| reason | Option\<String\> | スキップ・失敗理由 |

### JobSummary
ジョブ1回の実行結果サマリ(NFR-1: 実行前後のサマリ表示に使用)。

| フィールド | 型 | 説明 |
|---|---|---|
| job_name | String | ジョブ名 |
| archived_count / archived_bytes | u64 | アーカイブ件数・合計サイズ |
| relocated_count / relocated_bytes | u64 | 退避件数・合計サイズ |
| deleted_count / deleted_bytes | u64 | 削除件数・合計サイズ |
| skipped | List\<StageOutcome\> | スキップされた項目 |
| failed | List\<StageOutcome\> | 失敗した項目 |
| safety_brake_triggered | bool | セーフティブレーキが発動したか |

## 補助コンポーネント(ライブラリ化候補、requirements.md 2.1参照)

### ArchiveNamer
| メソッド | 説明 |
|---|---|
| single_file_name(original_path, basis_datetime, format) -> PathBuf | 単体ファイル圧縮の命名規則を実装(FR-2: `<元ファイル名>.<YYYYMMDDTHHMMSSZ>.<拡張子>`) |
| bundle_name(job_name, period_key, format) -> PathBuf | バンドル圧縮の命名規則を実装(FR-2: `<ジョブ名>.<期間キー>.tar.gz`) |

### BundleKey
| メソッド | 説明 |
|---|---|
| compute(basis_datetime, bundle_kind, timezone) -> PeriodKey | 基準日時が属する日/週/月の期間キーを計算する(FD-B1)。同じ入力に対し常に同じ結果を返す決定的な関数 |
