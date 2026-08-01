# Business Logic Model: neatnik-cli

> **改訂履歴(2026-08-02、2回目)**: `JobConfig`を「archive/relocate/deleteをそれぞれ0か1個持つ」構造から「ステージエントリ(archive/relocate/deleteのいずれか)を任意個・任意の順序で並べたリスト`stages`を持つ」構造に変更したことに伴い、1章・2章・3章・4章・7章を全面改訂した。実行順序は固定の「archive→relocate→delete」フェーズではなく、`stages`に**書かれた順序どおり**に実行する方式になった。あわせてBR-1(猶予日数の大小関係)の撤回、セーフティブレーキ評価単位のdeleteエントリ単位への変更(BR-13)を反映した。1回目の改訂(`targets`のステージ別分離)の内容は本改訂に統合済み。詳細な経緯はbusiness-rules.mdの改訂履歴、aidlc-docs/audit.mdを参照。

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
|  内部バリデーション |  (BR-2, BR-2.1。不正なら中断してエラー表示。Q C4=A)
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
| stagesを先頭から順に |  (BR-9。各エントリについて下記3章の「1エントリの処理」を実行。
| 1エントリずつ処理    |   あるエントリの処理が完了してから次のエントリへ進む)
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

`stages`が空の場合は警告(BR-2)を出すが、ジョブ自体は「何もせず正常終了」として扱う。

## 3. 1エントリの処理(スキャン→判定→実行、BR-9)

`stages`の各エントリ`E`(種別`archive`/`relocate`/`delete`のいずれか、猶予日数`E.after_days`)について、以下を実行する。archive/relocate/deleteで共通のパターンだが、判定後の実行内容と(deleteのみ)セーフティブレーキの扱いが異なる。

```
candidates = []
for target in E.targets:
    scanned = scan(target)   # FR-1。include/exclude(BR-6)、シンボリックリンク除外、
                              # 書き込み中判定(BR-7)、basis_datetime決定(BR-7.1)
    candidates.extend(scanned)

eligible = [c for c in candidates if (now - c.basis_datetime) >= E.after_days]
not_eligible = candidates - eligible
record StageOutcome(E.kind, Skipped) for each in not_eligible

if E.kind == Delete:
    # BR-13: このdeleteエントリの対象全体をまとめて評価してからセーフティブレーキを判定する
    # (ジョブ全体ではなく、このエントリ単位で独立に評価する)
    if safety_brake_would_trigger(eligible, E.safety_brake):
        handle_safety_brake(E.safety_brake)   # enforce設定に従う。dry-runの場合は判定のみ(6章)
        continue to next stages entry           # このエントリはブロックするが、後続のエントリは継続する
    for c in eligible:
        if not job.dry_run:
            execute_delete(c)
        record StageOutcome(Delete, Processed)
elif E.kind == Archive:
    for c in eligible:
        if not job.dry_run:
            execute_archive(c, E)   # BR-8/BR-9(命名・mtime継承)、4章のバンドル処理を含む
        record StageOutcome(Archive, Processed)
else:  # Relocate
    for c in eligible:
        if not job.dry_run:
            execute_relocate(c, E)  # BR-11(mtime/パーミッション保持)、BR-12(衝突解決)
        record StageOutcome(Relocate, Processed)
```

**同一実行内カスケードの成立**(BR-9): `stages`を書かれた順に実行するため、あるarchiveエントリの直後に、その出力先を監視するrelocateエントリを書いておけば、archiveエントリの処理完了直後のrelocateエントリのスキャンで、archiveが今まさに生成した出力ファイル(mtimeは元ファイルの基準日時を引き継いでいる、BR-9)が即座に発見される。メモリ上でのファイルの受け渡しは行わない。`stages`の並び順が意図と異なれば、その通りの(意図しない)順序で実行される。

**注記**:
- `keep_original: true`かつ`bundle: none`の場合、元ファイルは削除されないため、同じarchiveエントリの次回スキャンでも同じファイルが候補になるが、命名規則(BR-8)により生成される名前は決定的なので、既存の宛先が見つかり再処理はスキップされる(冪等性、FR-9)
- `keep_original: true`かつ`bundle: daily/weekly/monthly`の場合は、下記4章のバンドル処理単位における冪等性判定(mtime比較、BR-3)に従う
- あるarchiveエントリの`targets`の`include`が自身の出力パターン(`*.gz`等)にマッチしない限り、その出力が再び同じarchiveエントリの候補になることはない(BR-6改訂)

## 4. バンドル圧縮時の処理単位の違い

上記「1エントリの処理」はarchiveエントリにも共通するが、バンドル圧縮(`bundle: daily/weekly/monthly`)の場合は`execute_archive`の内部処理単位が異なる:

```
1. このarchiveエントリのスキャンで得た候補のうち、猶予日数(after_days)を満たすものが `eligible` である
2. eligibleをWatchTarget(basedir)ごとに分け、ターゲット内で各候補の基準日時から
   BundleKey.compute() で期間キーを算出し、期間キーごとにグルーピング (BR-10)
   (異なるターゲットの候補は別々にグルーピングする。1つのバンドルに混在させない)
3. 期間キーに対応する既存バンドルファイル(ArchiveNamer.bundle_name: archive名(E.name),
   target_name, period_key)が存在するか確認する
   - 存在しない場合: グループ内の全候補で新規バンドルを作成する
   - 存在する場合(keep_original: true運用での再実行、または後述の自己参照ケース): グループ内の
     各候補について、candidate.basis_datetime <= 既存バンドルのmtime か比較する (BR-3)
       - 真: 既に含まれているとみなしスキップする(冪等)
       - 偽: on_stale_bundle_member設定(warn/error)に従って記録する。ファイル・
             既存バンドルには手を加えない(バンドルへの追記は行わない)
4. 新規作成 or 既存のバンドルアーカイブは、このarchiveエントリの実行結果(StageOutcome)として
   記録される。以降のstagesエントリがこのバンドルファイルを処理対象にするかどうかは、
   それぞれの`targets`が独立に判定する(旧版のような「1つのFileCandidateとして後続ステージへ
   引き継ぐ」処理は行わない)
```

**バンドル自己参照時の挙動**: 後続のrelocateエントリの`targets`の`include`がバンドル出力(`*.tar.gz`等)にマッチするよう構成されている場合、同じarchiveエントリの次回スキャンでこのバンドル自身が候補として再度拾われることがある(そのarchiveエントリのincludeが同じパターンを含む場合)。この場合、バンドル自身の基準日時は自分自身のmtimeと一致するため、上記手順3の比較で「既に含まれている」と判定され、ファイルは変化しない。ただし、バンドル自身がグループの一員として`eligible`に含まれることで、StageOutcome集計上「処理した」件数に混入する場合がある。これを避けるため、archiveエントリのincludeは自身の出力パターンを含めないよう設計することを推奨する(BR-6)。

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
- `stages`エントリのターゲット必須バリデーション(BR-2.1): 不変条件テストに適する
- 1エントリの処理(本章3節): 1つの物理ファイルに着目すると、複数回の実行を経てNormal→Archived→Relocated→Deletedという状態遷移をたどるため、ステートフルプロパティテスト(PBT-06、Partial適用では非ブロッキング)の候補ともなり得る
- archiveエントリの自己参照非再発性(本章3-4節): あるarchiveエントリの`targets`の`include`が自身の出力パターンにマッチしない構成では、同一ファイルに対する繰り返し実行(複数回の`run`)がarchive処理を再度発生させないことを確認するプロパティ(BR-6/BR-9の回帰防止、business-rules.md 8章参照)
