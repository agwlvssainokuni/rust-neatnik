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
#   - ディレクトリ階層の捌き方: `include: ["**/*.log"]`によるサブディレクトリを
#     含む再帰的な走査、退避時の`layout: preserve`(basedirからの相対階層を保持)と
#     `layout: year_month`(階層を無視し基準日時のYYYY/MM単位に再分類、同名ファイルの
#     衝突は`on_conflict`で解決)の違い、バンドル圧縮ではサブディレクトリ構成の
#     ファイルをまとめても各ファイルの相対パスがtar.gz/zip内部にそのまま保持されること
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
LAYOUT_DIR="$WORKSPACE/logs/layout-demo"
STORAGE_DIR="$WORKSPACE/storage"
STORAGE_YEAR_MONTH_DIR="$WORKSPACE/storage-year-month"
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
mkdir -p "$SINGLE_DIR/service-a" "$SINGLE_DIR/service-b" \
    "$BUNDLE_DIR/region-a" "$BUNDLE_DIR/region-b" \
    "$LAYOUT_DIR/team-x" "$LAYOUT_DIR/team-y" "$STORAGE_DIR" "$STORAGE_YEAR_MONTH_DIR"

REFERENCE_EPOCH=$(($(date +%s) - 60))

# 単体ファイル圧縮(bundle: none)の対象。サブディレクトリ(service-a/、service-b/)に
# 分けて配置し、include: ["**/*.log"]による再帰的な走査と、退避(layout: preserve)での
# 階層保持を確認できるようにする
echo "service a access log" > "$SINGLE_DIR/service-a/access.log"
echo "service b access log" > "$SINGLE_DIR/service-b/access.log"

# バンドル圧縮(bundle: daily)の対象。同じ日に属する複数ファイルを1つのtar.gzにまとめる。
# region-a/、region-b/のサブディレクトリに分けて配置し、include: ["**/*.log"]による
# 再帰的な走査と、バンドル内部で相対パス(階層)がそのまま保持されることを確認できるようにする
echo "worker 1 output" > "$BUNDLE_DIR/region-a/worker-1.log"
echo "worker 2 output" > "$BUNDLE_DIR/region-a/worker-2.log"
echo "worker 3 output" > "$BUNDLE_DIR/region-b/worker-3.log"

# layout: preserve と layout: year_month の違いを見せる対象。あえて同じファイル名
# (report.log)を異なるサブディレクトリ(team-x/、team-y/)に配置する。year_monthは
# 元のディレクトリ階層を無視して基準日時のYYYY/MM単位に再分類するため、この2つは
# 退避先で同名衝突を起こし、on_conflict: renameによる連番付与が発生する
echo "team x report" > "$LAYOUT_DIR/team-x/report.log"
echo "team y report" > "$LAYOUT_DIR/team-y/report.log"

# 全ファイルのmtimeをREFERENCE_EPOCH(秒精度)に揃え、以降の経過日数計算のずれを防ぐ
touch_at_offset 0 \
    "$SINGLE_DIR/service-a/access.log" \
    "$SINGLE_DIR/service-b/access.log" \
    "$BUNDLE_DIR/region-a/worker-1.log" \
    "$BUNDLE_DIR/region-a/worker-2.log" \
    "$BUNDLE_DIR/region-b/worker-3.log" \
    "$LAYOUT_DIR/team-x/report.log" \
    "$LAYOUT_DIR/team-y/report.log"

cat > "$CONFIG_PATH" <<EOF
jobs:
  - name: demo-job
    stages:
      # 単体ファイル圧縮(bundle: none): ファイル1件ごとに<元ファイル名>.<日時>.gzを作る。
      # include: ["**/*.log"]でサブディレクトリ(service-a/、service-b/)を再帰的に走査する
      - type: archive
        name: demo-job-archive-single
        targets:
          - basedir: "$SINGLE_DIR"
            include: ["**/*.log"]
        after_days: $ARCHIVE_AFTER_DAYS
        format: gzip
        bundle: none
      # バンドル圧縮(bundle: daily): 同じ日のファイルをまとめて1つのtar.gzにする。
      # include: ["**/*.log"]でサブディレクトリ(region-a/、region-b/)を再帰的に走査するが、
      # バンドル自体はターゲット単位で1つにまとまる(サブディレクトリごとに分かれない)
      - type: archive
        name: demo-job-archive-bundle
        targets:
          - basedir: "$BUNDLE_DIR"
            name: workers
            include: ["**/*.log"]
        after_days: $ARCHIVE_AFTER_DAYS
        format: gzip
        bundle: daily
      # 退避: 上のarchiveの出力(*.gz、*.tar.gz)をそれぞれ監視対象にする。
      # layout: preserve のため、service-a/、service-b/ の階層構造を保ったまま
      # storage/ 配下に移動される
      - type: relocate
        targets:
          - basedir: "$SINGLE_DIR"
            include: ["**/*.gz"]
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

  # layout: preserve(上のジョブ)との対比用ジョブ。team-x/、team-y/ 配下の同名ファイル
  # (report.log)を layout: year_month で退避する。year_monthは元のディレクトリ階層を
  # 無視し基準日時のYYYY/MM単位に再分類するため、2つのファイルは同じ退避先パスに
  # 衝突し、on_conflict: renameにより連番が付与される
  - name: layout-comparison-job
    stages:
      - type: archive
        name: layout-comparison-archive
        targets:
          - basedir: "$LAYOUT_DIR"
            include: ["**/*.log"]
        after_days: $ARCHIVE_AFTER_DAYS
        format: gzip
        bundle: none
      - type: relocate
        targets:
          - basedir: "$LAYOUT_DIR"
            include: ["**/*.gz"]
        after_days: $RELOCATE_AFTER_DAYS
        destination: "$STORAGE_YEAR_MONTH_DIR"
        layout: year_month
        on_conflict: rename
EOF

show_dir() {
    local label="$1"
    local dir="$2"
    echo "-- $label --"
    if [ -n "$(find "$dir" -type f 2>/dev/null)" ]; then
        find "$dir" -type f -exec ls -l {} \; 2>/dev/null
    else
        echo "(空)"
    fi
}

show_state() {
    show_dir "logs/single(service-a/, service-b/)" "$SINGLE_DIR"
    show_dir "logs/bundle" "$BUNDLE_DIR"
    show_dir "storage(layout: preserve)" "$STORAGE_DIR"
}

show_layout_state() {
    show_dir "logs/layout-demo(team-x/, team-y/)" "$LAYOUT_DIR"
    show_dir "storage-year-month(layout: year_month)" "$STORAGE_YEAR_MONTH_DIR"
}

# バンドル(tar.gz)の内部エントリ名を一覧表示し、region-a/、region-b/の相対パスが
# バンドル内部にそのまま保持されていることを確認する
show_bundle_contents() {
    local bundle
    bundle="$(find "$BUNDLE_DIR" -name '*.tar.gz' -type f 2>/dev/null | head -n1)"
    if [ -n "$bundle" ]; then
        echo "-- $(basename "$bundle") の内部エントリ --"
        tar -tzf "$bundle"
    fi
}

run_at() {
    local days="$1"
    local now
    now="$(rfc3339_at_offset "$days")"
    "$NEATNIK" run --config "$CONFIG_PATH" --now "$now"
}

section "ステージ0: 通常(初期状態、作成直後)"
show_state
show_layout_state

section "neatnik validate(設定ファイルの検証。ファイルには一切触れない)"
"$NEATNIK" validate --config "$CONFIG_PATH"

ARCHIVE_UNDER=$((ARCHIVE_AFTER_DAYS - 1))
section "ステージ1a: 圧縮・アーカイブの${ARCHIVE_UNDER}日後(--now +${ARCHIVE_UNDER}日、archive閾値${ARCHIVE_AFTER_DAYS}日未満のため何も起きない)"
run_at "$ARCHIVE_UNDER"
show_state
show_layout_state

section "ステージ1b: 圧縮・アーカイブの${ARCHIVE_AFTER_DAYS}日後(--now +${ARCHIVE_AFTER_DAYS}日、archive閾値${ARCHIVE_AFTER_DAYS}日に到達し圧縮される)"
run_at "$ARCHIVE_AFTER_DAYS"
show_state
show_layout_state
show_bundle_contents
echo
echo "-> service-a/、service-b/ それぞれの階層内でその場に.gz化されたことを確認"
echo "-> region-a/、region-b/ に分かれていた3ファイルは1つのtar.gzにまとまるが、"
echo "   バンドル内部のエントリ名にはregion-a/、region-b/の相対パスがそのまま保持される"

RELOCATE_UNDER=$((RELOCATE_AFTER_DAYS - 1))
section "ステージ2a: 退避の${RELOCATE_UNDER}日後(--now +${RELOCATE_UNDER}日、relocate閾値${RELOCATE_AFTER_DAYS}日未満のため何も起きない)"
run_at "$RELOCATE_UNDER"
show_state
show_layout_state

section "ステージ2b: 退避の${RELOCATE_AFTER_DAYS}日後(--now +${RELOCATE_AFTER_DAYS}日、relocate閾値${RELOCATE_AFTER_DAYS}日に到達し退避される)"
run_at "$RELOCATE_AFTER_DAYS"
show_state
show_layout_state
echo
echo "-> layout: preserve(storage/)では service-a/、service-b/ の階層がそのまま保たれる"
echo "-> layout: year_month(storage-year-month/)では階層が無視されYYYY/MM単位に再分類され、"
echo "   同名だったreport.logどうしが衝突しon_conflict: renameで連番(_1等)が付与される"

DELETE_UNDER=$((DELETE_AFTER_DAYS - 1))
section "ステージ3a: 削除の${DELETE_UNDER}日後(--now +${DELETE_UNDER}日、delete閾値${DELETE_AFTER_DAYS}日未満のため何も起きない)"
run_at "$DELETE_UNDER"
show_state

section "ステージ3b: 削除の${DELETE_AFTER_DAYS}日後(--now +${DELETE_AFTER_DAYS}日、delete閾値${DELETE_AFTER_DAYS}日に到達し削除される)"
run_at "$DELETE_AFTER_DAYS"
show_state

section "まとめ"
cat <<SUMMARY
- service-a/access.log, service-b/access.log : 単体ファイル圧縮(bundle: none)で
  サブディレクトリごとに個別に.gz化 -> 退避(layout: preserveで階層保持) -> 削除
- region-a/worker-1.log, region-a/worker-2.log, region-b/worker-3.log : バンドル圧縮
  (bundle: daily)で1つのtar.gzにまとめて圧縮(内部にregion-a/、region-b/の相対パスを
  保持) -> 退避 -> 削除
- team-x/report.log, team-y/report.log : layout: year_monthとの対比用。退避先で
  ディレクトリ階層が失われ、同名ファイルどうしが衝突・連番付与されることを確認

ファイルのmtimeはほぼ作成時刻のまま動かさず(write-guard回避のため60秒だけ過去にずらす)、
\`neatnik run --now\`に与える日時だけを1日ずつ進めて複数回実行することで、
  - 各ステージの猶予日数(after_days)ちょうどの1日前では何も起きない
  - 猶予日数ちょうどの日には処理が実行される(「経過日数 >= 猶予日数」の境界)
  - archive・relocate・deleteが同一実行内でカスケードせず、段階を踏んで進む
  - include: ["**/*.log"]によるサブディレクトリの再帰的な走査
  - layout: preserve(階層保持)とlayout: year_month(階層を無視した再分類、
    on_conflict: renameによる衝突解決)の違い
  - バンドル圧縮では、複数のサブディレクトリにまたがるファイルも1つのアーカイブに
    まとまるが、各ファイルの相対パス(階層)はアーカイブ内部のエントリ名として
    そのまま保持される
ことを確認した。

再実行する場合はこのスクリプトを再度実行してください(workspace/は毎回リセットされます)。
SUMMARY
