# neatnik デモ

`neatnik`のハウスキーピング(アーカイブ→退避→削除)を実際に動かして確認するデモです。Unix系(Linux/macOS)向けの`run-demo.sh`とWindows向けの`run-demo.ps1`があり、同一仕様(同じシナリオ・同じ設定内容)です。

## 実行方法

Unix系(bash):

```sh
./demo/run-demo.sh
```

Windows(PowerShell 7以降):

```powershell
./demo/run-demo.ps1
```

`neatnik validate` → 経過日数を1日ずつ進めながらの`neatnik run --now`複数回実行、という流れで、設定した猶予日数(archive: 7日、relocate: 30日、delete: 365日)に応じて各ファイルがどう扱われるかを段階的に確認できます。

生成物(`demo/workspace/`)はこのプロジェクトディレクトリ内に作られますが、`.gitignore`で除外されており、スクリプトを再実行するたびに初期化されます。

## Unix版とWindows版の違い

- 経過日数の判定ロジック・設定ファイルの内容は完全に同一です。
- `neatnik`の書き込み中判定はOSごとに実装が異なります(Unix: アドバイザリロック検出+直近5秒以内の更新検出、Windows: 共有モードでのオープン試行)。`run-demo.ps1`はUnix版との比較のため同様にファイルのmtimeを実行開始時刻の60秒前に揃えていますが、Windowsの書き込み中判定はこの時間差に依存しないため本質的には不要です。
- mtime操作は、Unix版が`touch -t`、Windows版が.NETの`LastWriteTimeUtc`プロパティで行います。
