# Domain Entities: neatnik-cli

技術非依存の概念モデルとして記述する(Rustの型はあくまで実装イメージの参考)。

> **改訂履歴(2026-08-02、2回目)**: `JobConfig`を「archive/relocate/deleteをそれぞれ0か1個持つ」構造から、「**ステージエントリ(archive/relocate/deleteのいずれか)を任意個・任意の順序で並べたリスト`stages`を持つ**」構造に変更した。あわせて`enabled: bool`を廃止(リストへの記載有無で有効/無効を表現)し、バンドル命名で使っていた`job名`を`ArchiveConfig.name`(新規必須フィールド)に置き換えた。背景・理由はrequirements.md FR-5/FR-1の改訂注記、およびbusiness-rules.md参照。(1回目の改訂で`targets`をステージ毎に分離した内容は本改訂に統合済み)

## 設定モデル

### JobConfig
ジョブ単位(ロックスコープ、FR-5・FR-8)の設定。

| フィールド | 型 | 説明 |
|---|---|---|
| name | String | ジョブ名。ロックファイル名(`.{name}.lock`)にのみ使う。バンドル命名にはもう使わない(2026-08-02改訂) |
| stages | List\<StageConfig\> | archive/relocate/deleteのステージエントリを**任意個・任意の順序**で並べたリスト(0個も許容、business-rules.md BR-2参照) |

### StageConfig
`stages`リストの1要素。archive/relocate/deleteのいずれか1種類の設定を保持するタグ付き共用体。

| バリアント | 内容 |
|---|---|
| Archive(ArchiveConfig) | アーカイブ(圧縮)処理1件分の設定 |
| Relocate(RelocateConfig) | 退避処理1件分の設定 |
| Delete(DeleteConfig) | 削除処理1件分の設定 |

`stages`は**書かれた順序どおりに実行する**(business-rules.md BR-9)。`enabled`という概念は持たない。「無効化したい」場合はリストから当該エントリを削除する(YAMLコメントアウトでも代替可能)。

### WatchTarget
1つの監視対象ディレクトリとそのパターンを表す(FR-1)。`ArchiveConfig`/`RelocateConfig`/`DeleteConfig`がそれぞれ自身の`targets`として個別に持つ。

| フィールド | 型 | 説明 |
|---|---|---|
| basedir | PathBuf | 監視対象ディレクトリ(絶対パス)。「元階層保持」レイアウト(RelocateConfig.layout)の相対パス計算の基準にもなる |
| name | Option\<String\> | ターゲット識別子。省略時は`basedir`から自動導出する(例: `/var/log/app` → `var-log-app`)。バンドルアーカイブの命名で衝突を避けるために使う |
| include | List\<GlobPattern\> | `basedir`からの相対パスによる対象ファイルパターン(複数可) |
| exclude | List\<GlobPattern\> | `basedir`からの相対パスによる除外パターン |
| basis | BasisKind(Mtime\|Ctime\|FilenameDate) | 基準日時の情報源。デフォルトMtime。ターゲット単位で持つ(ファイル命名規則がターゲットごとに異なりうるため) |
| filename_date_rules | List\<FilenameDateRule\> | `basis: FilenameDate`の場合のみ使用。複数設定可能で、ファイル名に対して上から順に照合し最初にマッチしたものを採用する |

### FilenameDateRule
`basis: FilenameDate`における日付抽出ルール1件。

| フィールド | 型 | 説明 |
|---|---|---|
| regex | String | ファイル名(**basename、ディレクトリ階層を除いた部分**)に対する正規表現。日付部分を名前付きキャプチャグループ`(?P<date>...)`で指定する。キャプチャの前後には任意のリテラル文字列(プレフィックス・サフィックス)を含められる |
| format | String | キャプチャした文字列をパースする日付フォーマット(例: `%Y-%m-%d`) |

**スコープ**: `include`/`exclude`はディレクトリ階層を含んだglob(例: `**/*.log`)をサポートするが、`FilenameDateRule.regex`は常に**ファイルのbasenameのみ**に対して評価する(ディレクトリ部分は含まない)。ディレクトリ名自体に日付が埋め込まれているケース(例: `2026-07-01/access.log`)は本ルールのスコープ外とし、そのようなレイアウトでは`basis: Mtime`/`Ctime`を使用する

**例**: 1ターゲット内に複数の命名規則が混在する場合、ファイル名全体に固定するパターンをルールごとに書くことで、事実上「ファイル(命名規則)ごと」の抽出ルールとして機能する。
```
- regex: "^app_log\.(?P<date>\d{4}-\d{2}-\d{2})\.txt$"
  format: "%Y-%m-%d"
- regex: "^access-(?P<date>\d{8})\.log$"
  format: "%Y%m%d"
```

### ArchiveConfig
| フィールド | 型 | 説明 |
|---|---|---|
| name | String | **(2026-08-02追加、必須)** このarchiveエントリの識別子。バンドル命名(`<name>.<ターゲット名>.<期間キー>.tar.gz`)に使う。旧設計のジョブ名の役割を引き継ぐ。同一job内外を問わず、他のarchiveエントリと偶然一致すると命名衝突しうるが、利用者の責任とする(BR-8) |
| targets | List\<WatchTarget\> | アーカイブ対象の監視対象(複数可)。最低1件必須(BR-2.1)。バンドルモード時は解決後のターゲット名が重複してはならない(BR-2.2、2026-08-02追加) |
| after_days (N1) | u32 | アーカイブ猶予日数 |
| format | ArchiveFormat(Gzip\|Zip\|TarGz) | 圧縮形式(FR-2) |
| bundle | BundleKind(None\|Daily\|Weekly\|Monthly) | まとめ方針(FR-2) |
| bundle_timezone | Timezone | バンドル期間境界の計算に使うタイムゾーン(FD-B1) |
| keep_original | bool | 圧縮後に元ファイルを残すか(デフォルトfalse)。`bundle`の値に関わらず許可する(FD-B2、2回目の見直し)。バンドル圧縮時の冪等性はmtime比較方式(BR-3)で担保する |
| on_stale_bundle_member | OnStaleBundleMember(Warn\|Error) | バンドル対象ファイルのmtimeが既存バンドルのmtimeより新しい場合の挙動(BR-3)。デフォルトWarn |

### RelocateConfig
| フィールド | 型 | 説明 |
|---|---|---|
| targets | List\<WatchTarget\> | 退避対象の監視対象(複数可)。最低1件必須(BR-2.1)。通常はarchiveステージの出力先(単体圧縮なら元ファイルと同じディレクトリ)を指す |
| after_days (N2) | u32 | 退避猶予日数 |
| destination | PathBuf | 保管先ディレクトリ |
| layout | LayoutKind(Preserve\|YearMonth) | 移動先ディレクトリ構造。`Preserve`は当該候補が属する**このエントリ自身の`WatchTarget.basedir`**からの相対パスを保持する |
| on_conflict | ConflictPolicy(Rename\|Skip\|Error) | 同名ファイル衝突時の挙動 |

### DeleteConfig
| フィールド | 型 | 説明 |
|---|---|---|
| targets | List\<WatchTarget\> | 削除対象の監視対象(複数可)。最低1件必須(BR-2.1)。通常はrelocateステージの保管先(`RelocateConfig.destination`)を指す |
| after_days (N3) | u32 | 削除猶予日数 |
| safety_brake | SafetyBrakeConfig | セーフティブレーキ設定。**このdeleteエントリ自身の対象集合のみで判定する**(2026-08-02改訂。ジョブ全体でのバッチ評価ではない) |

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
| target | WatchTargetRef | 由来するWatchTarget(basedir・ターゲット名)への参照。発見元のステージエントリ自身の`targets`から得る。「元階層保持」の相対パス計算(FR-3)とバンドル命名(FR-2)に使う |
| basis_datetime | DateTime | 基準日時(WatchTarget.basisに従い決定) |
| size_bytes | u64 | ファイルサイズ |
| in_use | bool | 書き込み中と判定されたか(NFR-OS、OS別ロジック) |

### WriteGuardDetector(トレイト/抽象)
| メソッド | 説明 |
|---|---|
| is_in_use(path) -> bool | 他プロセスが書き込み中かをベストエフォートで判定(NFR-OS)。`UnixWriteGuardDetector`(flock検知+直近更新時刻)と`WindowsWriteGuardDetector`(共有モードオープン試行)の2実装 |

### JobLock(トレイト/抽象)
| メソッド | 説明 |
|---|---|
| acquire(job_name) -> Result\<LockGuard\> | 多重起動防止のアドバイザリファイルロックを取得(FR-8)。クロスプラットフォーム対応クレート(`fd-lock`等)で実装し、ロックファイルは設定ファイルと同じディレクトリに`.<job-name>.lock`として作成(FD-A3)。ジョブ(=`stages`全体)を1つのスコープとして排他制御する |

### Notifier(トレイト/抽象、MVPでは未実装)
| メソッド | 説明 |
|---|---|
| notify(event: NotificationEvent) | エラー・セーフティブレーキ発動等の通知(FR-10)。MVPではtraitのみ定義し、具体実装(メール/Slack等)は行わない |

## 処理結果モデル

### StageOutcome
1ファイル・1ステージエントリの処理結果。

| フィールド | 型 | 説明 |
|---|---|---|
| file | FileCandidate | 対象ファイル |
| stage | StageKind(Archive\|Relocate\|Delete) | 実行されたステージの種別 |
| status | OutcomeStatus(Processed\|Skipped\|Failed) | 結果 |
| reason | Option\<String\> | スキップ・失敗理由 |

### JobSummary
ジョブ1回の実行結果サマリ(NFR-1: 実行前後のサマリ表示に使用)。`stages`内の全エントリを通じた集計値。

| フィールド | 型 | 説明 |
|---|---|---|
| job_name | String | ジョブ名 |
| archived_count / archived_bytes | u64 | アーカイブ件数・合計サイズ(全archiveエントリの合計) |
| relocated_count / relocated_bytes | u64 | 退避件数・合計サイズ(全relocateエントリの合計) |
| deleted_count / deleted_bytes | u64 | 削除件数・合計サイズ(全deleteエントリの合計) |
| skipped | List\<StageOutcome\> | スキップされた項目 |
| failed | List\<StageOutcome\> | 失敗した項目 |
| safety_brake_triggered | bool | いずれかのdeleteエントリでセーフティブレーキが発動したか(エントリ毎の判定をORで集約) |

## 補助コンポーネント(ライブラリ化候補、requirements.md 2.1参照)

### ArchiveNamer
| メソッド | 説明 |
|---|---|
| single_file_name(original_path, basis_datetime, format) -> PathBuf | 単体ファイル圧縮の命名規則を実装(FR-2: `<元ファイル名>.<YYYYMMDDTHHMMSSZ>.<拡張子>`)。出力先は元ファイルと同じディレクトリ |
| bundle_name(archive_name, target_name, period_key, format) -> PathBuf | バンドル圧縮の命名規則を実装(FR-2: `<archive名>.<ターゲット名>.<期間キー>.tar.gz`、2026-08-02改訂で`job_name`から`archive_name`(`ArchiveConfig.name`)に変更)。出力先はそのターゲットの`basedir`直下。ターゲット名を含めるのは、同一archiveエントリ内の複数ターゲットが同じ期間キーで別々にバンドルを作った際の名前衝突を避けるため |

### BundleKey
| メソッド | 説明 |
|---|---|
| compute(basis_datetime, bundle_kind, timezone) -> PeriodKey | 基準日時が属する日/週/月の期間キーを計算する(FD-B1)。同じ入力に対し常に同じ結果を返す決定的な関数 |
