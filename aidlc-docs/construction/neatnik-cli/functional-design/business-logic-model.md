# Business Logic Model: neatnik-cli

## 1. 全体処理フロー

```
+------------------+
|  CLI引数パース    |
+------------------+
        |
        v
+------------------+
|  設定ファイル読込  |
+------------------+
        |
        v
+------------------+
|  内部バリデーション |  (BR-1〜BR-3。不正なら中断してエラー表示。Q C4=A)
+------------------+
        |
        v
+------------------+
| Clock決定         |  (--now指定ならFixedClock、なければSystemClock。FR-13)
+------------------+
        |
        v
+------------------+
| 対象ジョブ決定     |  (--jobで指定 or 全ジョブ)
+------------------+
        |
        v
+------------------+
| ジョブを逐次処理    |  (BR-15。1ジョブずつ下記「ジョブ処理フロー」を実行)
+------------------+
        |
        v
+------------------+
| 全体サマリ表示      |
+------------------+
```

## 2. ジョブ処理フロー(1ジョブあたり)

```
+-------------------+
| ジョブロック取得    |  (BR-16。失敗ならこのジョブをスキップし警告、次のジョブへ)
+-------------------+
        |
        v
+-------------------+
| ファイル走査(scan)  |  (FR-1。include/exclude、シンボリックリンク除外、
+-------------------+   書き込み中判定 BR-7)
        |
        v
+-------------------+
| 候補ごとにパイプライン |  (下記「1ファイルのパイプライン処理」)
| 処理を実行           |
+-------------------+
        |
        v
+-------------------+
| ジョブサマリ集計・出力 |  (JobSummary。dry-run時も同様に集計のみ)
+-------------------+
        |
        v
+-------------------+
| ジョブロック解放     |
+-------------------+
```

## 3. 1ファイルのパイプライン処理(カスケード処理、BR-9)

各`FileCandidate`に対し、以下を**同一実行内で順に**評価する。等号設定(N1=N2等)により複数ステージの条件を同時に満たす場合は、そのまま連続して処理する(要件FR-5)。

```
current = FileCandidate(元ファイル)

if job.archive.enabled and (now - current.basis_datetime) >= N1:
    if not job.dry_run:
        archived = execute_archive(current)   # BR-8, BR-9: 命名・mtime継承
        current = archived (keep_original=falseの場合。trueの場合は元ファイルも監視対象として残る)
    record StageOutcome(Archive, Processed)
else:
    record StageOutcome(Archive, Skipped) if enabled else N/A

if job.relocate.enabled and (now - current.basis_datetime) >= N2:
    if not job.dry_run:
        relocated = execute_relocate(current)  # BR-11: mtime/パーミッション保持、BR-12: 衝突解決
        current = relocated
    record StageOutcome(Relocate, Processed)

if job.delete.enabled and (now - current.basis_datetime) >= N3:
    if safety_brake_would_trigger():           # BR-13
        handle_safety_brake()                  # enforce設定に従う
    else:
        if not job.dry_run:
            execute_delete(current)
        record StageOutcome(Delete, Processed)
```

**注記**:
- `keep_original: true`かつ`bundle: none`の場合、元ファイルは削除されないため、次回実行時も基準日時が変わらず同じ判定を繰り返す。命名規則(BR-8)により生成される名前は決定的なので、既存の宛先が見つかり再処理はスキップされる(冪等性)
- `keep_original: true`かつ`bundle: daily/weekly/monthly`の場合は、下記4章のバンドル処理単位における冪等性判定(mtime比較、BR-3)に従う

## 4. バンドル圧縮時の処理単位の違い

上記「1ファイルのパイプライン処理」は単体ファイル圧縮を前提とした記述だが、バンドル圧縮(`bundle: daily/weekly/monthly`)の場合はアーカイブ段階のみ処理単位が異なる:

```
1. スキャンで得た候補のうち、アーカイブ条件(N1経過)を満たすものを抽出
2. 候補をWatchTarget(basedir)ごとに分け、ターゲット内で各候補の基準日時から
   BundleKey.compute() で期間キーを算出し、期間キーごとにグルーピング (BR-10)
   (異なるターゲットの候補は別々にグルーピングする。1つのバンドルに混在させない)
3. 期間キーに対応する既存バンドルファイル(ArchiveNamer.bundle_name: job_name,
   target_name, period_key)が存在するか確認する
   - 存在しない場合: グループ内の全候補で新規バンドルを作成する
   - 存在する場合(keep_original: true運用で再実行されたケース): グループ内の各候補について、
     candidate.basis_datetime <= 既存バンドルのmtime か比較する (BR-3)
       - 真: 既に含まれているとみなしスキップする(冪等)
       - 偽: on_stale_bundle_member設定(warn/error)に従って記録する。ファイル・
             既存バンドルには手を加えない(バンドルへの追記は行わない)
4. 新規作成 or 既存のバンドルアーカイブ1件を、以降の退避・削除パイプラインにおける
   「1つのFileCandidate」として扱う(基準日時 = グループ内の基準日時の最大値、BR-9)
```

## 5. エラーハンドリング(詳細化)

業務ルール(business-rules.md 7章)の一覧に対し、実装上のフロー観点を補足する。

- **すべての外部呼び出し(ファイルI/O)は明示的にエラーハンドリングする**(Security Baseline SECURITY-15、fail closed)
- **1ファイルの処理失敗は、ジョブ全体を止めない**: `StageOutcome(Failed)`として記録し、次の候補ファイルの処理を継続する。ただし「圧縮/移動先の空き容量不足」等、ジョブ全体に影響する種類のエラーは、そのジョブ全体を中断する(business-rules.md 7章)
- **リソース解放**: ジョブロック・一時ファイルハンドル等は、エラー発生時も確実に解放する(try/finally相当のパターンを徹底する。SECURITY-15)
- **ユーザー向けエラーメッセージ**: 内部パス・スタックトレース等は出力しない。原因と次のアクション(BR-4, BR-5)を案内する

## 6. dry-runとセーフティブレーキの相互作用

- `--dry-run`指定時は、セーフティブレーキの閾値判定自体は行うが(サマリに「閾値を超過する見込み」を表示)、実際の処理停止・ロックは発生しない(dry-runは常に安全な確認用途のため)
- `--now`(FR-13)と`--dry-run`を組み合わせることで、未来日時をエミュレートしたセーフティブレーキの発動確認が可能

## 7. PBT-01: 業務ロジックとテスト観点の対応

具体的なプロパティ一覧はbusiness-rules.md 8章を参照。本モデルにおいて特にプロパティテストが有効な箇所:
- `BundleKey.compute` / `ArchiveNamer.single_file_name`: 純粋関数であり、決定性・往復性のテストに適する
- 猶予日数バリデーション(BR-1): 不変条件テストに適する
- 1ファイルのパイプライン処理(本章3節): 状態遷移(Normal→Archived→Relocated→Deleted)を持つため、ステートフルプロパティテスト(PBT-06、Partial適用では非ブロッキング)の候補ともなり得る
