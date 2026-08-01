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
# neatnikのハウスキーピング(圧縮・アーカイブ→退避→削除)を実際に動かして確認する
# デモスクリプト。
#
# このデモが示すこと:
#   - 単体ファイル圧縮(bundle: none)とバンドル圧縮(bundle: daily)の両方をサポートする
#   - 1回の`neatnik run`で最後まで一気に処理するのではなく、`neatnik`コマンドを
#     複数回に分けて実行し、「通常 → 圧縮・アーカイブ → 退避 → 削除」と段階を踏んで
#     ファイルの状態が変わっていく様子を示す
#
# ステージ間で`targets`を共有しない設計(README参照)のため、各ステージは自身の
# targets/includeで前段の出力を監視する。`--now`でシステム時計を変更せずに未来日時を
# エミュレートし、経過日数の閾値(archive: 7日 / relocate: 30日 / delete: 365日)を
# 1ステップずつ超えさせることで、同一実行内でのカスケードを起こさずに段階を分離する。
#
# 生成物はすべて demo/workspace/ 配下(このプロジェクトディレクトリ内)に作られる。
# .gitignore対象であり、再実行のたびにリセットされる。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKSPACE="$SCRIPT_DIR/workspace"
SINGLE_DIR="$WORKSPACE/logs/single"
BUNDLE_DIR="$WORKSPACE/logs/bundle"
STORAGE_DIR="$WORKSPACE/storage"
CONFIG_PATH="$WORKSPACE/config.yaml"

ARCHIVE_AFTER_DAYS=7
RELOCATE_AFTER_DAYS=30
DELETE_AFTER_DAYS=365
INITIAL_AGE_DAYS=10 # archive閾値(7日)は超えるが、relocate閾値(30日)は超えない初期経過日数

# BSD(macOS)/GNU(Linux)双方のdateコマンドに対応する(過去方向、touch -t用: YYYYMMDDHHMM)
days_ago() {
    local days="$1"
    if date -v-1d >/dev/null 2>&1; then
        date -v-"${days}"d +%Y%m%d%H%M
    else
        date -d "-${days} days" +%Y%m%d%H%M
    fi
}

# BSD(macOS)/GNU(Linux)双方のdateコマンドに対応する(未来方向、--now用: RFC3339)
days_ahead_rfc3339() {
    local days="$1"
    if date -v-1d >/dev/null 2>&1; then
        date -v+"${days}"d -u +%Y-%m-%dT%H:%M:%SZ
    else
        date -d "+${days} days" -u +%Y-%m-%dT%H:%M:%SZ
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
mkdir -p "$SINGLE_DIR" "$BUNDLE_DIR" "$STORAGE_DIR"

# 単体ファイル圧縮(bundle: none)の対象
echo "single file archive demo" > "$SINGLE_DIR/app-access.log"

# バンドル圧縮(bundle: daily)の対象。同じ日に属する複数ファイルを1つのtar.gzにまとめる
echo "worker 1 output" > "$BUNDLE_DIR/worker-1.log"
echo "worker 2 output" > "$BUNDLE_DIR/worker-2.log"
echo "worker 3 output" > "$BUNDLE_DIR/worker-3.log"

# 全ファイルのmtimeを「archive閾値は超えるがrelocate閾値は超えない」経過日数に揃える
MTIME="$(days_ago "$INITIAL_AGE_DAYS")"
touch -t "$MTIME" \
    "$SINGLE_DIR/app-access.log" \
    "$BUNDLE_DIR/worker-1.log" \
    "$BUNDLE_DIR/worker-2.log" \
    "$BUNDLE_DIR/worker-3.log"

cat > "$CONFIG_PATH" <<EOF
jobs:
  - name: demo-job
    stages:
      # 単体ファイル圧縮(bundle: none): ファイル1件ごとに<元ファイル名>.<日時>.gzを作る
      - type: archive
        name: demo-job-archive-single
        targets:
          - basedir: "$SINGLE_DIR"
            include: ["*.log"]
        after_days: $ARCHIVE_AFTER_DAYS
        format: gzip
        bundle: none
      # バンドル圧縮(bundle: daily): 同じ日のファイルをまとめて1つのtar.gzにする
      - type: archive
        name: demo-job-archive-bundle
        targets:
          - basedir: "$BUNDLE_DIR"
            name: workers
            include: ["*.log"]
        after_days: $ARCHIVE_AFTER_DAYS
        format: gzip
        bundle: daily
      # 退避: 上のarchiveの出力(*.gz、*.tar.gz)をそれぞれ監視対象にする
      - type: relocate
        targets:
          - basedir: "$SINGLE_DIR"
            include: ["*.gz"]
          - basedir: "$BUNDLE_DIR"
            include: ["*.tar.gz"]
        after_days: $RELOCATE_AFTER_DAYS
        destination: "$STORAGE_DIR"
        layout: preserve
        on_conflict: rename
      # 削除: 上のrelocateのdestinationを監視対象にする
      - type: delete
        targets:
          - basedir: "$STORAGE_DIR"
            include: ["**/*"]
        after_days: $DELETE_AFTER_DAYS
EOF

show_state() {
    echo "-- logs/single --"
    ls -l "$SINGLE_DIR" 2>/dev/null || echo "(空)"
    echo "-- logs/bundle --"
    ls -l "$BUNDLE_DIR" 2>/dev/null || echo "(空)"
    echo "-- storage --"
    find "$STORAGE_DIR" -type f -exec ls -l {} \; 2>/dev/null
    [ -n "$(find "$STORAGE_DIR" -type f 2>/dev/null)" ] || echo "(空)"
}

section "ステージ0: 通常(初期状態、経過${INITIAL_AGE_DAYS}日)"
show_state

section "neatnik validate(設定ファイルの検証。ファイルには一切触れない)"
"$NEATNIK" validate --config "$CONFIG_PATH"

section "ステージ1: 圧縮・アーカイブ(neatnik run、経過${INITIAL_AGE_DAYS}日 >= archive閾値${ARCHIVE_AFTER_DAYS}日)"
"$NEATNIK" run --config "$CONFIG_PATH"
show_state

RELOCATE_NOW="$(days_ahead_rfc3339 25)"
section "ステージ2: 退避(neatnik run --now ${RELOCATE_NOW}、経過$((INITIAL_AGE_DAYS + 25))日 >= relocate閾値${RELOCATE_AFTER_DAYS}日)"
"$NEATNIK" run --config "$CONFIG_PATH" --now "$RELOCATE_NOW"
show_state

DELETE_NOW="$(days_ahead_rfc3339 370)"
section "ステージ3: 削除(neatnik run --now ${DELETE_NOW}、経過$((INITIAL_AGE_DAYS + 370))日 >= delete閾値${DELETE_AFTER_DAYS}日)"
"$NEATNIK" run --config "$CONFIG_PATH" --now "$DELETE_NOW"
show_state

section "まとめ"
cat <<SUMMARY
- app-access.log        : 単体ファイル圧縮(bundle: none)で個別に.gz化 -> 退避 -> 削除
- worker-1/2/3.log      : バンドル圧縮(bundle: daily)で1つの.tar.gzにまとめて圧縮 -> 退避 -> 削除

各ステージは同一実行内でカスケードさせず、archive・relocate・deleteを
別々の\`neatnik run\`呼び出し(2回目以降は\`--now\`で経過日数を進める)として
実行することで、段階的に状態が変わっていく様子を示した。

再実行する場合はこのスクリプトを再度実行してください(workspace/は毎回リセットされます)。
SUMMARY
