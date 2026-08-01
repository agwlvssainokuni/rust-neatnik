# Build Instructions: neatnik-cli

## Prerequisites
- **Build Tool**: Cargo(Rust標準ツールチェイン)。動作確認済みバージョン: `rustc 1.97.0` / `cargo 1.97.0`。MSRVは固定しない方針(tech-stack-decisions.md)
- **Dependencies**: `Cargo.toml`に列挙済み。初回ビルド時に`cargo`が自動的にダウンロード・コンパイルする(追加のパッケージマネージャ不要)
- **Environment Variables**: 不要(ビルドに必須の環境変数はない)
- **System Requirements**: Linux/macOS/Windowsのいずれか。特別なメモリ・ディスク要件はない(通常のRustクレートと同程度)

## Build Steps

### 1. Install Dependencies
```bash
# 依存クレートの取得はビルド時に自動実行されるため、明示的なインストール手順は不要
cargo fetch
```

### 2. Configure Environment
特別な環境設定は不要。ワークスペースルート(`Cargo.toml`のあるディレクトリ)で以下のコマンドを実行する。

### 3. Build All Units
本プロジェクトは単一ユニット(`neatnik-cli`、lib+bin構成)。

```bash
# デバッグビルド(開発・テスト用)
cargo build

# リリースビルド(配布用、[profile.release]でopt-level=3/lto/codegen-units=1/stripを適用)
cargo build --release
```

### 4. Verify Build Success
- **Expected Output**: `Finished \`release\` profile [optimized] target(s) in ...s`
- **Build Artifacts**: `target/release/neatnik`(単一バイナリ、lib自体は`target/release/libneatnik.rlib`)
- **Common Warnings**: なし。`cargo build`/`cargo build --release`ともに警告0件であることを確認済み(2026-08-01時点)

## Troubleshooting

### Build Fails with Dependency Errors
- **Cause**: ネットワーク接続不可、または`Cargo.lock`とレジストリの不整合
- **Solution**: `cargo update`でロックファイルを更新するか、オフライン環境では`cargo build --offline`(事前に`cargo fetch`済みであること)を使う

### Build Fails with Compilation Errors
- **Cause**: 通常は発生しない想定(本ドキュメント作成時点でクリーンビルド確認済み)。Rustツールチェインのバージョン差異が原因になりうる
- **Solution**: `rustup update`で最新の安定版ツールチェインに更新する。Windows向けビルドの場合は`src/scan.rs`の`cfg(windows)`ブロック(`windows-sys`使用)がターゲットに応じて有効化される点に注意(NFR-OS参照)
