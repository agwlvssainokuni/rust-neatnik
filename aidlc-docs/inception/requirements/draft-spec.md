# Neatnik 仕様書

## 1. 概要

**Neatnik** は、ログファイル・作業用ファイル・一時ファイルの後片付け（ハウスキーピング）を自動化するRust製CLIツールである。

- 圧縮・アーカイブ、退避（保管用フォルダへの移動）、一定期間経過後の削除、という3段階の処理を持つ
- 各段階は独立に有効/無効化でき、猶予日数も個別に設定できる
- 単一バイナリで配布し、cron等の外部スケジューラから定期実行することを前提とする

### 名前について
- crates.ioの `neatnik` クレート名は本仕様策定時点では未使用と思われるが、公開前に最終確認が必要
- GitHub上には `neatnik` という組織アカウント（別業種、非アクティブ）が既に存在するため、リポジトリは `neatnik-rs` や個人アカウント配下などで作成する想定
- 発想の近い既存ツールとして `neatcli`（Rust製、ディレクトリ整理ツール）が存在する。機能の重複はないが、紛らわしくないか留意する

## 2. 対象ファイルの想定

| 種別 | 例 | 特徴 |
|---|---|---|
| ログファイル | `app.log`, `access-2026-07-01.log` | 日次/サイズローテーションされることが多い |
| 作業用ファイル | 一時出力、中間生成物 | 用途完了後は不要になる |
| 一時ファイル | `*.tmp`, `*.bak`, OSやミドルウェアの一時領域 | 短命、放置されがち |

対象は拡張子・パスパターン・更新日時等で柔軟に指定できるようにする。

## 3. 機能要件

### 3.1 共通：対象ファイルの検出
- 監視対象ディレクトリ（複数指定可）を走査し、ファイルパターン（glob）でマッチしたファイルを対象とする
- 各ファイルの「基準日時」は更新日時（mtime）をデフォルトとし、作成日時・ファイル名中の日付のいずれかを選択可能とする
- シンボリックリンク、ロック中ファイル（他プロセスが書き込み中）は除外する
- 除外パターン（exclude glob）を指定可能とする

### 3.2 圧縮・アーカイブ機能
- 基準日時から **アーカイブ猶予日数（N1日）** 経過したファイルを対象とする
- 圧縮形式：`gzip`（単体ファイル向け）/ `zip` または `tar.gz`（フォルダ単位のまとめ圧縮）を選択可能
- まとめ方針：
  - ファイル単位で個別に圧縮（例: `app.log` → `app.log.gz`）
  - 期間単位でバンドル圧縮（例: 1日/1週間/1ヶ月ごとに1アーカイブへまとめる）
- 圧縮後、元ファイルは削除するか残すかを設定可能とする（デフォルトは削除）
- 圧縮ファイル名にはタイムスタンプを付与し、重複を防止する

### 3.3 退避（保管用フォルダへの移動）機能
- 基準日時から **退避猶予日数（N2日）** 経過したファイル（圧縮済みアーカイブを含む）を、指定の保管用フォルダへ移動する
- 移動先ディレクトリ構造は、元のディレクトリ階層を保持する、または `年/月` 単位で分類する、のいずれかを選択可能とする
- 移動先に同名ファイルが存在する場合の衝突解決ルール（リネーム／上書き禁止でスキップ／エラー）を設定可能とする
- 移動元と移動先が別ディスク／別ボリュームの場合も考慮し、コピー後に元ファイルを削除する方式で実装する（アトミック性の担保）

### 3.4 削除機能
- 保管用フォルダ内のファイル、または退避を経ずに直接削除対象となるファイルについて、基準日時から **削除猶予日数（N3日）** 経過したものを削除する
- 削除前に対象一覧をログ出力し、`--dry-run`（実際には削除しない）モードで事前確認できるようにする
- 誤削除防止のため以下を設ける
  - 削除件数・容量が閾値を超える場合は自動実行を止め、確認を要求する「セーフティブレーキ」

### 3.5 ステージの関係
```
[通常領域]
   │  経過 N1日
   ▼
[圧縮・アーカイブ]（オプション、スキップ可）
   │  経過 N2日
   ▼
[保管用フォルダへ退避]
   │  経過 N3日
   ▼
[削除]
```
- 各ステージはそれぞれ独立に有効/無効化でき、「圧縮せず直接退避」「退避せず直接削除」といった構成も可能とする
- N1 < N2 < N3 の順に猶予日数が大きくなることをバリデーションで担保する

## 4. CLIインターフェース

```
neatnik run --config config.yaml --job app-server-logs [--dry-run]
neatnik run --config config.yaml --all
neatnik validate --config config.yaml
```

- `--dry-run`：全ジョブ・全ステージ共通で有効化できるフラグ
- `--job`：設定ファイル内の特定ジョブのみ実行（未指定なら全ジョブ）
- `validate`：設定ファイルの構文・猶予日数の大小関係（N1<N2<N3）などを事前チェック

## 5. 設定方式（YAML）

ジョブ単位（対象ディレクトリ×ルールの組）で設定する。

```yaml
jobs:
  - name: app-server-logs
    include:
      - "/var/log/app/*.log"
    exclude:
      - "/var/log/app/current.log"
    basis: mtime          # mtime | ctime | filename_date
    archive:
      enabled: true
      after_days: 7
      format: gzip        # gzip | zip | tar.gz
      bundle: daily        # none | daily | weekly | monthly
      keep_original: false
    relocate:
      enabled: true
      after_days: 30
      destination: "/mnt/storage/app-logs"
      layout: year_month    # preserve | year_month
      on_conflict: rename    # rename | skip | error
    delete:
      enabled: true
      after_days: 365
      dry_run: false
      safety_threshold_gb: 50
```

## 6. 非機能要件

- **安全性**：全ステージで dry-run 実行が可能。実行前に対象件数・合計サイズをサマリ表示する
- **冪等性**：同じジョブを複数回実行しても、既に処理済みのファイルに対して二重処理・エラーが発生しない
- **ログ出力**：処理対象・成功/失敗・スキップ理由を構造化ログ（JSON等）で出力し、後から監査できるようにする
- **通知**：エラー発生時、または削除件数が閾値を超えた場合に通知（メール／Slack等）を送れるようにする（拡張機能として）
- **実行方式**：単一バイナリのCLIとして単発実行できることに加え、cron／systemd timer等から定期実行できること
- **並行実行制御**：同一ジョブの多重起動を防止するロック機構を持つ
- **パフォーマンス**：大量ファイル（数万件規模）でも実用的な時間で走査・処理できること
- **権限**：ファイル操作に必要な権限が不足している場合はエラーとして扱い、処理を中断せず該当ファイルをスキップして継続する（設定で中断も選択可）

## 7. エラーハンドリング方針

| 状況 | 挙動 |
|---|---|
| 対象ファイルが処理中にロックされている | スキップしてログに記録、次回実行時に再対象化 |
| 圧縮/移動先の空き容量不足 | ジョブを中断し、エラー通知 |
| 移動先に同名ファイルが存在 | 設定の `on_conflict` ポリシーに従う |
| 削除件数が閾値超過 | 自動実行を停止し、確認待ちとする（セーフティブレーキ） |

## 8. 実装方式（Rust）

### 8.1 プロジェクト構成
```
neatnik/
├── Cargo.toml
├── config.example.yaml
└── src/
    ├── main.rs          # CLIエントリポイント（clap）
    ├── config.rs        # YAML設定の構造体定義・バリデーション
    ├── scan.rs          # 対象ファイル走査（walkdir + glob）
    ├── archive.rs        # 圧縮・アーカイブ処理
    ├── relocate.rs       # 退避（保管フォルダへの移動）処理
    ├── delete.rs          # 削除処理（セーフティブレーキ含む）
    └── logging.rs         # tracingセットアップ
```

### 8.2 依存クレート（想定）
```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
walkdir = "2"
glob = "0.3"
flate2 = "1"
tar = "0.4"
zip = "0.6"
chrono = "0.4"
rayon = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
anyhow = "1"
thiserror = "1"
```

### 8.3 処理フローの実装イメージ
各ステージを`Result<StageSummary, CleanupError>`を返す独立関数にし、`main.rs`側でジョブごとに順に呼び出す。

```rust
fn run_job(job: &JobConfig, dry_run: bool) -> anyhow::Result<JobSummary> {
    let targets = scan::find_targets(job)?;
    let archived = archive::process(&targets, job, dry_run)?;
    let relocated = relocate::process(&archived, job, dry_run)?;
    let deleted = delete::process(&relocated, job, dry_run)?;
    Ok(JobSummary { archived, relocated, deleted })
}
```

## 9. 今後の検討事項

- 圧縮・削除対象ファイルの世代管理（何世代分残すか、日数ではなく世代数での指定）
- マルチテナント／複数環境（開発・検証・本番）での設定分離
- 処理結果のダッシュボード化（容量削減効果の可視化）
- 監査要件がある場合、削除ログの保存期間・改ざん防止
- クラウドストレージ（Azure Blob Storage等）への退避対応
