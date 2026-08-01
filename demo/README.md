# neatnik デモ

`neatnik`のハウスキーピング(アーカイブ→退避→削除)を実際に動かして確認するデモです。

## 実行方法

```sh
./demo/run-demo.sh
```

経過日数の異なる4つのログファイルを`demo/workspace/logs/`に用意し、`neatnik validate` → `neatnik run --dry-run` → `neatnik run`の順に実行します。設定した猶予日数(archive: 7日、relocate: 30日、delete: 365日)に応じて、各ファイルがどう扱われるかを一度に確認できます。

生成物(`demo/workspace/`)はこのプロジェクトディレクトリ内に作られますが、`.gitignore`で除外されており、スクリプトを再実行するたびに初期化されます。
