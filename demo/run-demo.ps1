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
# デモスクリプト(Windows/PowerShell版)。demo/run-demo.sh(Unix版)と同一仕様。
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
# Unix版との違い:
#   - mtime/`--now`の時刻計算は`touch -t`/`date`の代わりに.NETの
#     `[DateTimeOffset]`とFileSystemInfoの`LastWriteTimeUtc`プロパティを使う
#   - 書き込み中判定はWindowsでは共有モードでのオープン試行のみで、Unix版のような
#     「直近5秒以内の更新」チェックは存在しない(README参照)。そのため本来は
#     基準時刻を実行時刻ちょうどにしても書き込み中誤検出は起きないが、Unix版との
#     比較のしやすさのため同様に基準時刻を60秒過去にずらす
#
# 生成物はすべて demo/workspace/ 配下(このプロジェクトディレクトリ内)に作られる。
# .gitignore対象であり、再実行のたびにリセットされる。

#Requires -Version 7.0

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = $PSScriptRoot
$RepoRoot = Split-Path -Parent $ScriptDir
$Workspace = Join-Path $ScriptDir "workspace"
$SingleDir = Join-Path $Workspace "logs\single"
$BundleDir = Join-Path $Workspace "logs\bundle"
$LayoutDir = Join-Path $Workspace "logs\layout-demo"
$StorageDir = Join-Path $Workspace "storage"
$StorageYearMonthDir = Join-Path $Workspace "storage-year-month"
$ConfigPath = Join-Path $Workspace "config.yaml"

$ArchiveAfterDays = 7
$RelocateAfterDays = 30
$DeleteAfterDays = 365

# 起点となるUnixエポック秒(ワークスペース初期化直前に1回だけ取得)。
# ファイルのmtime・`--now`の両方をこの1点からの相対オフセットで計算することで、
# ファイル作成時刻のサブ秒精度と`--now`(秒精度)のずれによる「境界ちょうどのはずが
# 1日分足りない」誤判定を避ける。
$ReferenceEpoch = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - 60

function ConvertTo-UtcDateTime {
    param([Parameter(Mandatory)][long]$Epoch)
    return [DateTimeOffset]::FromUnixTimeSeconds($Epoch).UtcDateTime
}

# REFERENCE_EPOCHから指定日数だけ進めた時刻に、渡されたファイルのmtime
# (LastWriteTimeUtc)を設定する
function Set-MtimeAtOffset {
    param(
        [Parameter(Mandatory)][int]$OffsetDays,
        [Parameter(Mandatory)][string[]]$Paths
    )
    $utc = ConvertTo-UtcDateTime ($ReferenceEpoch + ($OffsetDays * 86400))
    foreach ($p in $Paths) {
        (Get-Item -LiteralPath $p).LastWriteTimeUtc = $utc
    }
}

# REFERENCE_EPOCHから指定日数だけ進めた時刻をRFC3339(UTC)で返す(--now用)
function Get-Rfc3339AtOffset {
    param([Parameter(Mandatory)][int]$OffsetDays)
    $utc = ConvertTo-UtcDateTime ($ReferenceEpoch + ($OffsetDays * 86400))
    return $utc.ToString("yyyy-MM-ddTHH:mm:ssZ")
}

function Write-DemoSection {
    param([Parameter(Mandatory)][string]$Title)
    Write-Output ""
    Write-Output "===================================================================="
    Write-Output $Title
    Write-Output "===================================================================="
}

Write-DemoSection "neatnikバイナリをビルドします(cargo build --release)"
Push-Location $RepoRoot
try {
    & cargo build --release --quiet
    if ($LASTEXITCODE -ne 0) { throw "cargo build に失敗しました(exit $LASTEXITCODE)" }
}
finally {
    Pop-Location
}
$Neatnik = Join-Path $RepoRoot "target\release\neatnik.exe"

Write-DemoSection "デモ用ワークスペースを初期化します: $Workspace"
if (Test-Path -LiteralPath $Workspace) {
    Remove-Item -LiteralPath $Workspace -Recurse -Force
}
New-Item -ItemType Directory -Force -Path (Join-Path $SingleDir "service-a") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $SingleDir "service-b") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $BundleDir "region-a") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $BundleDir "region-b") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $LayoutDir "team-x") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $LayoutDir "team-y") | Out-Null
New-Item -ItemType Directory -Force -Path $StorageDir | Out-Null
New-Item -ItemType Directory -Force -Path $StorageYearMonthDir | Out-Null

# 単体ファイル圧縮(bundle: none)の対象。サブディレクトリ(service-a/、service-b/)に
# 分けて配置し、include: ["**/*.log"]による再帰的な走査と、退避(layout: preserve)での
# 階層保持を確認できるようにする
$SingleAFile = Join-Path $SingleDir "service-a\access.log"
$SingleBFile = Join-Path $SingleDir "service-b\access.log"
"service a access log" | Out-File -Encoding utf8NoBOM -FilePath $SingleAFile
"service b access log" | Out-File -Encoding utf8NoBOM -FilePath $SingleBFile

# バンドル圧縮(bundle: daily)の対象。同じ日に属する複数ファイルを1つのtar.gzにまとめる。
# region-a/、region-b/のサブディレクトリに分けて配置し、include: ["**/*.log"]による
# 再帰的な走査と、バンドル内部で相対パス(階層)がそのまま保持されることを確認できるようにする
$BundleA1File = Join-Path $BundleDir "region-a\worker-1.log"
$BundleA2File = Join-Path $BundleDir "region-a\worker-2.log"
$BundleB1File = Join-Path $BundleDir "region-b\worker-3.log"
"worker 1 output" | Out-File -Encoding utf8NoBOM -FilePath $BundleA1File
"worker 2 output" | Out-File -Encoding utf8NoBOM -FilePath $BundleA2File
"worker 3 output" | Out-File -Encoding utf8NoBOM -FilePath $BundleB1File

# layout: preserve と layout: year_month の違いを見せる対象。あえて同じファイル名
# (report.log)を異なるサブディレクトリ(team-x/、team-y/)に配置する。year_monthは
# 元のディレクトリ階層を無視して基準日時のYYYY/MM単位に再分類するため、この2つは
# 退避先で同名衝突を起こし、on_conflict: renameによる連番付与が発生する
$TeamXFile = Join-Path $LayoutDir "team-x\report.log"
$TeamYFile = Join-Path $LayoutDir "team-y\report.log"
"team x report" | Out-File -Encoding utf8NoBOM -FilePath $TeamXFile
"team y report" | Out-File -Encoding utf8NoBOM -FilePath $TeamYFile

# 全ファイルのmtimeをREFERENCE_EPOCH(秒精度)に揃え、以降の経過日数計算のずれを防ぐ
Set-MtimeAtOffset -OffsetDays 0 -Paths @(
    $SingleAFile, $SingleBFile,
    $BundleA1File, $BundleA2File, $BundleB1File,
    $TeamXFile, $TeamYFile
)

$ConfigContent = @"
jobs:
  - name: demo-job
    stages:
      # 単体ファイル圧縮(bundle: none): ファイル1件ごとに<元ファイル名>.<日時>.gzを作る。
      # include: ["**/*.log"]でサブディレクトリ(service-a/、service-b/)を再帰的に走査する
      - type: archive
        name: demo-job-archive-single
        targets:
          - basedir: '$SingleDir'
            include: ["**/*.log"]
        after_days: $ArchiveAfterDays
        format: gzip
        bundle: none
      # バンドル圧縮(bundle: daily): 同じ日のファイルをまとめて1つのtar.gzにする。
      # include: ["**/*.log"]でサブディレクトリ(region-a/、region-b/)を再帰的に走査するが、
      # バンドル自体はターゲット単位で1つにまとまる(サブディレクトリごとに分かれない)
      - type: archive
        name: demo-job-archive-bundle
        targets:
          - basedir: '$BundleDir'
            name: workers
            include: ["**/*.log"]
        after_days: $ArchiveAfterDays
        format: gzip
        bundle: daily
      # 退避: 上のarchiveの出力(*.gz、*.tar.gz)をそれぞれ監視対象にする。
      # layout: preserve のため、service-a/、service-b/ の階層構造を保ったまま
      # storage/ 配下に移動される
      - type: relocate
        targets:
          - basedir: '$SingleDir'
            include: ["**/*.gz"]
          - basedir: '$BundleDir'
            include: ["*.tar.gz"]
        after_days: $RelocateAfterDays
        destination: '$StorageDir'
        layout: preserve
        on_conflict: rename
      # 削除: 上のrelocateのdestinationを監視対象にする
      - type: delete
        targets:
          - basedir: '$StorageDir'
            include: ["**/*"]
        after_days: $DeleteAfterDays

  # layout: preserve(上のジョブ)との対比用ジョブ。team-x/、team-y/ 配下の同名ファイル
  # (report.log)を layout: year_month で退避する。year_monthは元のディレクトリ階層を
  # 無視し基準日時のYYYY/MM単位に再分類するため、2つのファイルは同じ退避先パスに
  # 衝突し、on_conflict: renameにより連番が付与される
  - name: layout-comparison-job
    stages:
      - type: archive
        name: layout-comparison-archive
        targets:
          - basedir: '$LayoutDir'
            include: ["**/*.log"]
        after_days: $ArchiveAfterDays
        format: gzip
        bundle: none
      - type: relocate
        targets:
          - basedir: '$LayoutDir'
            include: ["**/*.gz"]
        after_days: $RelocateAfterDays
        destination: '$StorageYearMonthDir'
        layout: year_month
        on_conflict: rename
"@
$ConfigContent | Out-File -Encoding utf8NoBOM -FilePath $ConfigPath

function Show-Dir {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string]$Dir
    )
    Write-Output "-- $Label --"
    $files = Get-ChildItem -LiteralPath $Dir -Recurse -File -ErrorAction SilentlyContinue
    if ($files) {
        foreach ($f in $files) {
            "{0,10}  {1}  {2}" -f $f.Length, $f.LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss"), $f.FullName
        }
    }
    else {
        Write-Output "(空)"
    }
}

function Show-State {
    Show-Dir -Label "logs\single(service-a\, service-b\)" -Dir $SingleDir
    Show-Dir -Label "logs\bundle" -Dir $BundleDir
    Show-Dir -Label "storage(layout: preserve)" -Dir $StorageDir
}

function Show-LayoutState {
    Show-Dir -Label "logs\layout-demo(team-x\, team-y\)" -Dir $LayoutDir
    Show-Dir -Label "storage-year-month(layout: year_month)" -Dir $StorageYearMonthDir
}

# バンドル(tar.gz)の内部エントリ名を一覧表示し、region-a\、region-b\の相対パスが
# バンドル内部にそのまま保持されていることを確認する
function Show-BundleContents {
    $bundle = Get-ChildItem -LiteralPath $BundleDir -Filter "*.tar.gz" -Recurse -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($bundle) {
        $tarCmd = Get-Command tar -ErrorAction SilentlyContinue
        if ($tarCmd) {
            Write-Output "-- $($bundle.Name) の内部エントリ --"
            & tar -tzf $bundle.FullName
        }
        else {
            Write-Output "-- $($bundle.Name) の内部エントリ(tarコマンドが見つからないためスキップ) --"
        }
    }
}

function Invoke-NeatnikRunAt {
    param([Parameter(Mandatory)][int]$Days)
    $now = Get-Rfc3339AtOffset -OffsetDays $Days
    & $Neatnik run --config $ConfigPath --now $now
    if ($LASTEXITCODE -ne 0) { throw "neatnik run に失敗しました(exit $LASTEXITCODE)" }
}

Write-DemoSection "ステージ0: 通常(初期状態、作成直後)"
Show-State
Show-LayoutState

Write-DemoSection "neatnik validate(設定ファイルの検証。ファイルには一切触れない)"
& $Neatnik validate --config $ConfigPath
if ($LASTEXITCODE -ne 0) { throw "neatnik validate に失敗しました(exit $LASTEXITCODE)" }

$ArchiveUnder = $ArchiveAfterDays - 1
Write-DemoSection "ステージ1a: 圧縮・アーカイブの${ArchiveUnder}日後(--now +${ArchiveUnder}日、archive閾値${ArchiveAfterDays}日未満のため何も起きない)"
Invoke-NeatnikRunAt -Days $ArchiveUnder
Show-State
Show-LayoutState

Write-DemoSection "ステージ1b: 圧縮・アーカイブの${ArchiveAfterDays}日後(--now +${ArchiveAfterDays}日、archive閾値${ArchiveAfterDays}日に到達し圧縮される)"
Invoke-NeatnikRunAt -Days $ArchiveAfterDays
Show-State
Show-LayoutState
Show-BundleContents
Write-Output ""
Write-Output "-> service-a\、service-b\ それぞれの階層内でその場に.gz化されたことを確認"
Write-Output "-> region-a\、region-b\ に分かれていた3ファイルは1つのtar.gzにまとまるが、"
Write-Output "   バンドル内部のエントリ名にはregion-a\、region-b\の相対パスがそのまま保持される"

$RelocateUnder = $RelocateAfterDays - 1
Write-DemoSection "ステージ2a: 退避の${RelocateUnder}日後(--now +${RelocateUnder}日、relocate閾値${RelocateAfterDays}日未満のため何も起きない)"
Invoke-NeatnikRunAt -Days $RelocateUnder
Show-State
Show-LayoutState

Write-DemoSection "ステージ2b: 退避の${RelocateAfterDays}日後(--now +${RelocateAfterDays}日、relocate閾値${RelocateAfterDays}日に到達し退避される)"
Invoke-NeatnikRunAt -Days $RelocateAfterDays
Show-State
Show-LayoutState
Write-Output ""
Write-Output "-> layout: preserve(storage\)では service-a\、service-b\ の階層がそのまま保たれる"
Write-Output "-> layout: year_month(storage-year-month\)では階層が無視されYYYY/MM単位に再分類され、"
Write-Output "   同名だったreport.logどうしが衝突しon_conflict: renameで連番(_1等)が付与される"

$DeleteUnder = $DeleteAfterDays - 1
Write-DemoSection "ステージ3a: 削除の${DeleteUnder}日後(--now +${DeleteUnder}日、delete閾値${DeleteAfterDays}日未満のため何も起きない)"
Invoke-NeatnikRunAt -Days $DeleteUnder
Show-State

Write-DemoSection "ステージ3b: 削除の${DeleteAfterDays}日後(--now +${DeleteAfterDays}日、delete閾値${DeleteAfterDays}日に到達し削除される)"
Invoke-NeatnikRunAt -Days $DeleteAfterDays
Show-State

Write-DemoSection "まとめ"
@'
- service-a/access.log, service-b/access.log : 単体ファイル圧縮(bundle: none)で
  サブディレクトリごとに個別に.gz化 -> 退避(layout: preserveで階層保持) -> 削除
- region-a/worker-1.log, region-a/worker-2.log, region-b/worker-3.log : バンドル圧縮
  (bundle: daily)で1つのtar.gzにまとめて圧縮(内部にregion-a/、region-b/の相対パスを
  保持) -> 退避 -> 削除
- team-x/report.log, team-y/report.log : layout: year_monthとの対比用。退避先で
  ディレクトリ階層が失われ、同名ファイルどうしが衝突・連番付与されることを確認

ファイルのmtimeはほぼ作成時刻のまま動かさず(Unix版との比較のため60秒だけ過去にずらす。
Windowsの書き込み中判定は共有モードでのオープン試行のみのため、この60秒ずらしは
本質的には不要)、`neatnik run --now`に与える日時だけを1日ずつ進めて複数回実行することで、
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
'@ | Write-Output