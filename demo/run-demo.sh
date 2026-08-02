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
#   - 各ステージの猶予日数(after_days)が「以上(>=)」の境界値であること。
#     ファイル自体はほぼ作成時刻(mtime)のまま動かさず、`--now`に与える日時だけを
#     1日ずつ動かして`neatnik run`を実行することで、閾値の1日手前では
#     何も起きず、閾値ちょうどの日で処理されることを示す
#
# ステージ間で`targets`を共有しない設計(README参照)のため、各ステージは自身の
# targets/includeで前段の出力を監視する。`--now`はシステム時計を変更せずに
# 未来日時をエミュレートする仕組みで、これを使って経過日数の閾値
# (archive: 7日 / relocate: 30日 / delete: 365日)を1ステップずつ超えさせることで、
# 同一実行内でのカスケードを起こさずに段階を分離する。
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

# BSD(macOS)/GNU(Linux)双方のdateコマンドに対応するか判定する
IS_BSD_DATE=0
if date -v-1d >/dev/null 2>&1; then
    IS_BSD_DATE=1
fi

# 起点となるエポック秒(ワークスペース初期化直前に1回だけ取得)。
# ファイルのmtime・`--now`の両方をこの1点からの相対オフセットで計算することで、
# ファイル作成時刻のサブ秒精度と`--now`(秒精度)のずれによる「境界ちょうどのはずが
# 1日分足りない」誤判定を避ける。また、実時刻より60秒差し引くことで、書き込み中
# ファイルを検出するwrite-guard(実時計基準でmtimeが直近5秒以内かを見る。`--now`とは
# 無関係)に「書き込み中」と誤検出されるのを避ける
REFERENCE_EPOCH=""

# 指定エポック秒をtouch -t形式(YYYYMMDDHHMM.SS、ローカル時刻)に変換する
epoch_to_touch_ts() {
    local epoch="$1"
    if [ "$IS_BSD_DATE" = "1" ]; then
        date -r "$epoch" +%Y%m%d%H%M.%S
    else
        date -d "@${epoch}" +%Y%m%d%H%M.%S
    fi
}

# 指定エポック秒をRFC3339(UTC)形式に変換する
epoch_to_rfc3339() {
    local epoch="$1"
    if [ "$IS_BSD_DATE" = "1" ]; then
        date -r "$epoch" -u +%Y-%m-%dT%H:%M:%SZ
    else
        date -u -d "@${epoch}" +%Y-%m-%dT%H:%M:%SZ
    fi
}

# REFERENCE_EPOCHから指定日数だけ進めた時刻に、渡されたファイルのmtimeを設定する
touch_at_offset() {
    local offset_days="$1"
    shift
    local epoch=$((REFERENCE_EPOCH + offset_days * 86400))
    touch -t "$(epoch_to_touch_ts "$epoch")" "$@"
}

# REFERENCE_EPOCHから指定日数だけ進めた時刻をRFC3339(UTC)で返す(--now用)
rfc3339_at_offset() {
    local offset_days="$1"
    local epoch=$((REFERENCE_EPOCH + offset_days * 86400))
    epoch_to_rfc3339 "$epoch"
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

REFERENCE_EPOCH=$(($(date +%s) - 60))

# 単体ファイル圧縮(bundle: none)の対象
echo "single file archive demo" > "$SINGLE_DIR/app-access.log"

# バンドル圧縮(bundle: daily)の対象。同じ日に属する複数ファイルを1つのtar.gzにまとめる
echo "worker 1 output" > "$BUNDLE_DIR/worker-1.log"
echo "worker 2 output" > "$BUNDLE_DIR/worker-2.log"
echo "worker 3 output" > "$BUNDLE_DIR/worker-3.log"

# 全ファイルのmtimeをREFERENCE_EPOCH(秒精度)に揃え、以降の経過日数計算のずれを防ぐ
touch_at_offset 0 \
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

run_at() {
    local days="$1"
    local now
    now="$(rfc3339_at_offset "$days")"
    "$NEATNIK" run --config "$CONFIG_PATH" --now "$now"
}

section "ステージ0: 通常(初期状態、作成直後)"
show_state

section "neatnik validate(設定ファイルの検証。ファイルには一切触れない)"
"$NEATNIK" validate --config "$CONFIG_PATH"

ARCHIVE_UNDER=$((ARCHIVE_AFTER_DAYS - 1))
section "ステージ1a: 圧縮・アーカイブの${ARCHIVE_UNDER}日後(--now +${ARCHIVE_UNDER}日、archive閾値${ARCHIVE_AFTER_DAYS}日未満のため何も起きない)"
run_at "$ARCHIVE_UNDER"
show_state

section "ステージ1b: 圧縮・アーカイブの${ARCHIVE_AFTER_DAYS}日後(--now +${ARCHIVE_AFTER_DAYS}日、archive閾値${ARCHIVE_AFTER_DAYS}日に到達し圧縮される)"
run_at "$ARCHIVE_AFTER_DAYS"
show_state

RELOCATE_UNDER=$((RELOCATE_AFTER_DAYS - 1))
section "ステージ2a: 退避の${RELOCATE_UNDER}日後(--now +${RELOCATE_UNDER}日、relocate閾値${RELOCATE_AFTER_DAYS}日未満のため何も起きない)"
run_at "$RELOCATE_UNDER"
show_state

section "ステージ2b: 退避の${RELOCATE_AFTER_DAYS}日後(--now +${RELOCATE_AFTER_DAYS}日、relocate閾値${RELOCATE_AFTER_DAYS}日に到達し退避される)"
run_at "$RELOCATE_AFTER_DAYS"
show_state

DELETE_UNDER=$((DELETE_AFTER_DAYS - 1))
section "ステージ3a: 削除の${DELETE_UNDER}日後(--now +${DELETE_UNDER}日、delete閾値${DELETE_AFTER_DAYS}日未満のため何も起きない)"
run_at "$DELETE_UNDER"
show_state

section "ステージ3b: 削除の${DELETE_AFTER_DAYS}日後(--now +${DELETE_AFTER_DAYS}日、delete閾値${DELETE_AFTER_DAYS}日に到達し削除される)"
run_at "$DELETE_AFTER_DAYS"
show_state

section "まとめ"
cat <<SUMMARY
- app-access.log        : 単体ファイル圧縮(bundle: none)で個別に.gz化 -> 退避 -> 削除
- worker-1/2/3.log      : バンドル圧縮(bundle: daily)で1つの.tar.gzにまとめて圧縮 -> 退避 -> 削除

ファイルのmtimeはほぼ作成時刻のまま動かさず(write-guard回避のため60秒だけ過去にずらす)、
\`neatnik run --now\`に与える日時だけを1日ずつ進めて複数回実行することで、
  - 各ステージの猶予日数(after_days)ちょうどの1日前では何も起きない
  - 猶予日数ちょうどの日には処理が実行される(「経過日数 >= 猶予日数」の境界)
  - archive・relocate・deleteが同一実行内でカスケードせず、段階を踏んで進む
ことを確認した。

再実行する場合はこのスクリプトを再度実行してください(workspace/は毎回リセットされます)。
SUMMARY
