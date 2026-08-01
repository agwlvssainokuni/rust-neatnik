#!/usr/bin/env bash
# Copyright 2026 agwlvssainokuni
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
# neatnikのハウスキーピング(アーカイブ→退避→削除のカスケード処理)を
# 実際に動かして確認するデモスクリプト。
#
# 経過日数の異なる4つのログファイルを用意し、
#   1. neatnik validate
#   2. neatnik run --dry-run
#   3. neatnik run
# を順に実行して、猶予日数(archive:7日 / relocate:30日 / delete:365日)に応じて
# ファイルがどう変化するかを示す。
#
# 生成物はすべて demo/workspace/ 配下(このプロジェクトディレクトリ内)に作られる。
# .gitignore対象であり、再実行のたびにリセットされる。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE="$SCRIPT_DIR/workspace"
LOGS_DIR="$WORKSPACE/logs"
STORAGE_DIR="$WORKSPACE/storage"
CONFIG_PATH="$WORKSPACE/config.yaml"

# BSD(macOS)/GNU(Linux)双方のdateコマンドに対応する
days_ago() {
    local days="$1"
    if date -v-1d >/dev/null 2>&1; then
        date -v-"${days}"d +%Y%m%d%H%M
    else
        date -d "-${days} days" +%Y%m%d%H%M
    fi
}

section() {
    echo
    echo "===================================================================="
    echo "$1"
    echo "===================================================================="
}

section "neatnikバイナリをビルドします(cargo build --release)"
(cd "$REPO_ROOT" && cargo build --release --quiet)
NEATNIK="$REPO_ROOT/target/release/neatnik"

section "デモ用ワークスペースを初期化します: $WORKSPACE"
rm -rf "$WORKSPACE"
mkdir -p "$LOGS_DIR" "$STORAGE_DIR"

echo "recent activity"      > "$LOGS_DIR/app-recent.log"
echo "will be archived"     > "$LOGS_DIR/app-archive-only.log"
echo "will be relocated"    > "$LOGS_DIR/app-relocate.log"
echo "will be fully purged" > "$LOGS_DIR/app-old.log"

# mtimeを意図的にずらし、猶予日数に対する各ファイルの立ち位置を作る
touch -t "$(days_ago 2)"   "$LOGS_DIR/app-recent.log"
touch -t "$(days_ago 10)"  "$LOGS_DIR/app-archive-only.log"
touch -t "$(days_ago 40)"  "$LOGS_DIR/app-relocate.log"
touch -t "$(days_ago 400)" "$LOGS_DIR/app-old.log"

cat > "$CONFIG_PATH" <<EOF
jobs:
  - name: demo-job
    targets:
      - basedir: "$LOGS_DIR"
        include: ["*.log"]
    archive:
      enabled: true
      after_days: 7
      format: gzip
      bundle: none
      keep_original: false
    relocate:
      enabled: true
      after_days: 30
      destination: "$STORAGE_DIR"
      layout: preserve
      on_conflict: rename
    delete:
      enabled: true
      after_days: 365
EOF

section "logs/ の初期状態(mtime = 経過日数)"
ls -l "$LOGS_DIR"

section "neatnik validate"
"$NEATNIK" validate --config "$CONFIG_PATH"

section "neatnik run --dry-run (何も変更しない事前確認)"
"$NEATNIK" run --config "$CONFIG_PATH" --dry-run

section "dry-run後もlogs/は無変更"
ls -l "$LOGS_DIR"

section "neatnik run (実行)"
"$NEATNIK" run --config "$CONFIG_PATH"

section "logs/ の実行後の状態"
ls -l "$LOGS_DIR"

section "storage/ の実行後の状態(退避されたアーカイブ)"
find "$STORAGE_DIR" -type f -exec ls -l {} \;

section "まとめ"
cat <<'SUMMARY'
- app-recent.log        (2日経過)  : 猶予日数(7日)未満のため無変更
- app-archive-only.log (10日経過)  : アーカイブ(圧縮)のみ実行、まだ退避猶予(30日)未満
- app-relocate.log     (40日経過)  : アーカイブ→退避まで同一実行内でカスケード処理
- app-old.log         (400日経過)  : アーカイブ→退避→削除まで同一実行内で完走(跡形なし)

再実行する場合はこのスクリプトを再度実行してください(workspace/は毎回リセットされます)。
SUMMARY
