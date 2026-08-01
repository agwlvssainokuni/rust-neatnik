// Copyright 2026 agwlvssainokuni
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! 圧縮・アーカイブ処理(単体ファイル/バンドル、FR-2、BR-3、BR-8、BR-9、BR-10)。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Utc};
use chrono_tz::Tz;
use filetime::FileTime;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::config::{ArchiveConfig, ArchiveFormat, BundleKind, OnStaleBundleMember};
use crate::error::{ArchiveError, NeatnikError};
use crate::scan::{FileCandidate, WatchTargetRef};

impl ArchiveFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ArchiveFormat::Gzip => "gz",
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::TarGz => "tar.gz",
        }
    }
}

/// アーカイブファイルの命名規則(BR-8)。
pub struct ArchiveNamer;

impl ArchiveNamer {
    /// 単体ファイル圧縮の命名: `<元ファイル名>.<基準日時のYYYYMMDDTHHMMSSZ>.<拡張子>`。出力先は元ファイルと同じディレクトリ
    pub fn single_file_name(original_path: &Path, basis_datetime: DateTime<Utc>, format: ArchiveFormat) -> PathBuf {
        let parent = original_path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archive");
        let timestamp = basis_datetime.format("%Y%m%dT%H%M%SZ");
        parent.join(format!("{file_name}.{timestamp}.{}", format.extension()))
    }

    /// バンドル圧縮の命名: `<ジョブ名>.<ターゲット名>.<期間キー>.tar.gz`。出力先はターゲットの`basedir`直下
    pub fn bundle_name(job_name: &str, target_name: &str, period_key: &str, basedir: &Path) -> PathBuf {
        basedir.join(format!("{job_name}.{target_name}.{period_key}.tar.gz"))
    }
}

/// バンドルの期間キー(BR-10)。同じ入力に対し常に同じ結果を返す決定的な関数
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BundleKey(pub String);

impl BundleKey {
    pub fn compute(basis_datetime: DateTime<Utc>, bundle_kind: BundleKind, timezone: &Tz) -> Self {
        let local = basis_datetime.with_timezone(timezone);
        let key = match bundle_kind {
            BundleKind::None | BundleKind::Daily => local.format("%Y-%m-%d").to_string(),
            BundleKind::Weekly => {
                let iso = local.iso_week();
                format!("{}-W{:02}", iso.year(), iso.week())
            }
            BundleKind::Monthly => local.format("%Y-%m").to_string(),
        };
        BundleKey(key)
    }
}

/// `bundle_timezone`設定(BR-10)からタイムゾーンを解決する。省略時はローカルタイムゾーンを使う
pub fn resolve_timezone(configured: Option<&str>) -> Result<Tz, ArchiveError> {
    let name = match configured {
        Some(tz) => tz.to_string(),
        None => iana_time_zone::get_timezone().map_err(|source| ArchiveError::TimezoneDetection {
            message: source.to_string(),
        })?,
    };
    Tz::from_str(&name).map_err(|_| ArchiveError::InvalidTimezone { name })
}

/// 単体ファイルアーカイブの実行結果。
#[derive(Debug, Clone)]
pub struct SingleFileRunResult {
    pub destination: PathBuf,
    pub created: bool,
}

/// BR-8/BR-9/BR-3: 単体ファイルを圧縮する。既に決定的な名前の出力が存在する場合は再作成しない(冪等性)
pub fn run_single_file(candidate: &FileCandidate, config: &ArchiveConfig) -> Result<SingleFileRunResult, NeatnikError> {
    let destination = ArchiveNamer::single_file_name(&candidate.path, candidate.basis_datetime, config.format);

    if destination.exists() {
        return Ok(SingleFileRunResult {
            destination,
            created: false,
        });
    }

    let temp_path = temp_path_for(&destination);
    match config.format {
        ArchiveFormat::Gzip => write_gzip(&candidate.path, &temp_path)?,
        ArchiveFormat::Zip => write_zip_single(&candidate.path, &temp_path)?,
        ArchiveFormat::TarGz => {
            let file_name = candidate
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            write_tar_gz(&[(candidate.path.clone(), file_name)], &temp_path)?;
        }
    }
    fs::rename(&temp_path, &destination).map_err(|source| ArchiveError::Create {
        path: destination.clone(),
        source,
    })?;
    set_mtime(&destination, candidate.basis_datetime).map_err(|source| ArchiveError::Create {
        path: destination.clone(),
        source,
    })?;

    if !config.keep_original {
        fs::remove_file(&candidate.path).map_err(|source| ArchiveError::Create {
            path: candidate.path.clone(),
            source,
        })?;
    }

    Ok(SingleFileRunResult {
        destination,
        created: true,
    })
}

/// バンドルアーカイブ1件分の実行結果。
#[derive(Debug, Clone)]
pub struct BundleRunResult {
    pub bundle_path: PathBuf,
    pub created: bool,
    pub included: Vec<PathBuf>,
    pub stale: Vec<PathBuf>,
}

/// 期間キー1件分のバンドル処理結果。1グループの失敗が他のグループの処理を止めないよう、
/// グループ単位で成功/失敗を保持する(「1ファイルの失敗はジョブ全体を止めない」方針と同じ考え方)
#[derive(Debug)]
pub struct BundleGroupResult {
    pub period_key: String,
    pub members: Vec<PathBuf>,
    pub outcome: Result<BundleRunResult, ArchiveError>,
}

/// BR-10: 各ファイルの基準日時でグルーピングし、期間キーごとにバンドルを作成する(ターゲット単位)
pub fn run_bundle(
    job_name: &str,
    target: &WatchTargetRef,
    candidates: &[FileCandidate],
    config: &ArchiveConfig,
) -> Result<Vec<BundleGroupResult>, NeatnikError> {
    let timezone = resolve_timezone(config.bundle_timezone.as_deref())?;
    let mut groups: BTreeMap<String, Vec<&FileCandidate>> = BTreeMap::new();
    for candidate in candidates {
        let key = BundleKey::compute(candidate.basis_datetime, config.bundle, &timezone);
        groups.entry(key.0).or_default().push(candidate);
    }

    let results = groups
        .into_iter()
        .map(|(period_key, members)| {
            let member_paths = members.iter().map(|member| member.path.clone()).collect();
            let outcome = run_bundle_group(job_name, target, &period_key, &members, config);
            BundleGroupResult {
                period_key,
                members: member_paths,
                outcome,
            }
        })
        .collect();
    Ok(results)
}

fn run_bundle_group(
    job_name: &str,
    target: &WatchTargetRef,
    period_key: &str,
    members: &[&FileCandidate],
    config: &ArchiveConfig,
) -> Result<BundleRunResult, ArchiveError> {
    let bundle_path = ArchiveNamer::bundle_name(job_name, &target.name, period_key, &target.basedir);

    if let Ok(metadata) = fs::metadata(&bundle_path) {
        let existing_mtime: DateTime<Utc> = metadata
            .modified()
            .map(DateTime::<Utc>::from)
            .map_err(|source| ArchiveError::Create {
                path: bundle_path.clone(),
                source,
            })?;

        let mut included = Vec::new();
        let mut stale = Vec::new();
        for member in members {
            if member.basis_datetime <= existing_mtime {
                included.push(member.path.clone());
                continue;
            }
            match config.on_stale_bundle_member {
                OnStaleBundleMember::Warn => {
                    tracing::warn!(
                        path = %member.path.display(),
                        bundle = %bundle_path.display(),
                        "bundle member is newer than existing bundle (BR-3)"
                    );
                    stale.push(member.path.clone());
                }
                OnStaleBundleMember::Error => {
                    return Err(ArchiveError::StaleBundleMember {
                        path: member.path.clone(),
                        bundle: bundle_path.clone(),
                    });
                }
            }
        }
        // BR-3: 既存バンドルが見つかった場合、スキップ・記録のいずれのケースでもファイル・バンドルには手を加えない
        return Ok(BundleRunResult {
            bundle_path,
            created: false,
            included,
            stale,
        });
    }

    let entries: Vec<(PathBuf, String)> = members
        .iter()
        .map(|member| {
            let relative = member
                .path
                .strip_prefix(&target.basedir)
                .unwrap_or(&member.path)
                .to_string_lossy()
                .replace('\\', "/");
            (member.path.clone(), relative)
        })
        .collect();

    let temp_path = temp_path_for(&bundle_path);
    write_tar_gz(&entries, &temp_path)?;
    fs::rename(&temp_path, &bundle_path).map_err(|source| ArchiveError::Create {
        path: bundle_path.clone(),
        source,
    })?;

    let max_basis = members
        .iter()
        .map(|member| member.basis_datetime)
        .max()
        .unwrap_or_else(Utc::now);
    set_mtime(&bundle_path, max_basis).map_err(|source| ArchiveError::Create {
        path: bundle_path.clone(),
        source,
    })?;

    if !config.keep_original {
        for member in members {
            if let Err(source) = fs::remove_file(&member.path) {
                tracing::warn!(
                    path = %member.path.display(),
                    error = %source,
                    "failed to remove original file after bundling"
                );
            }
        }
    }

    Ok(BundleRunResult {
        bundle_path,
        created: true,
        included: members.iter().map(|member| member.path.clone()).collect(),
        stale: Vec::new(),
    })
}

fn temp_path_for(destination: &Path) -> PathBuf {
    let file_name = destination.file_name().and_then(|n| n.to_str()).unwrap_or("archive");
    destination.with_file_name(format!(".{file_name}.tmp"))
}

/// 任意のファイルのmtimeを設定する(BR-9/BR-11で共通に使う)。relocateモジュールからも再利用する
pub(crate) fn set_mtime(path: &Path, datetime: DateTime<Utc>) -> std::io::Result<()> {
    let file_time = FileTime::from_system_time(SystemTime::from(datetime));
    filetime::set_file_mtime(path, file_time)
}

fn io_err<E: std::fmt::Display>(err: E) -> std::io::Error {
    std::io::Error::other(err.to_string())
}

fn write_gzip(source: &Path, destination: &Path) -> Result<(), ArchiveError> {
    let mut input = fs::File::open(source).map_err(|source_err| ArchiveError::Create {
        path: source.to_path_buf(),
        source: source_err,
    })?;
    let output = fs::File::create(destination).map_err(|source_err| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: source_err,
    })?;
    let mut encoder = GzEncoder::new(output, Compression::default());
    std::io::copy(&mut input, &mut encoder).map_err(|source_err| ArchiveError::WriteEntry {
        archive: destination.to_path_buf(),
        entry: source.to_path_buf(),
        source: source_err,
    })?;
    encoder.finish().map_err(|source_err| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: source_err,
    })?;
    Ok(())
}

fn write_zip_single(source: &Path, destination: &Path) -> Result<(), ArchiveError> {
    let file = fs::File::create(destination).map_err(|source_err| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: source_err,
    })?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let name = source.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    zip.start_file(name, options).map_err(|e| ArchiveError::WriteEntry {
        archive: destination.to_path_buf(),
        entry: source.to_path_buf(),
        source: io_err(e),
    })?;
    let mut input = fs::File::open(source).map_err(|source_err| ArchiveError::Create {
        path: source.to_path_buf(),
        source: source_err,
    })?;
    std::io::copy(&mut input, &mut zip).map_err(|source_err| ArchiveError::WriteEntry {
        archive: destination.to_path_buf(),
        entry: source.to_path_buf(),
        source: source_err,
    })?;
    zip.finish().map_err(|e| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: io_err(e),
    })?;
    Ok(())
}

fn write_tar_gz(entries: &[(PathBuf, String)], destination: &Path) -> Result<(), ArchiveError> {
    let file = fs::File::create(destination).map_err(|source_err| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: source_err,
    })?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (path, arcname) in entries {
        builder
            .append_path_with_name(path, arcname)
            .map_err(|source_err| ArchiveError::WriteEntry {
                archive: destination.to_path_buf(),
                entry: path.clone(),
                source: source_err,
            })?;
    }
    let encoder = builder.into_inner().map_err(|source_err| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: source_err,
    })?;
    encoder.finish().map_err(|source_err| ArchiveError::Create {
        path: destination.to_path_buf(),
        source: source_err,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use proptest::prelude::*;
    use tempfile::tempdir;

    fn make_target_ref(basedir: &Path, name: &str) -> WatchTargetRef {
        WatchTargetRef {
            basedir: basedir.to_path_buf(),
            name: name.to_string(),
        }
    }

    fn make_candidate(basedir: &Path, target_name: &str, path: PathBuf, basis: DateTime<Utc>) -> FileCandidate {
        FileCandidate {
            size_bytes: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            path,
            target: make_target_ref(basedir, target_name),
            basis_datetime: basis,
            in_use: false,
        }
    }

    #[test]
    fn single_file_name_matches_br8_format() {
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 3, 15, 42).unwrap();
        let name = ArchiveNamer::single_file_name(Path::new("/var/log/app/app.log"), basis, ArchiveFormat::Gzip);
        assert_eq!(name, PathBuf::from("/var/log/app/app.log.20260725T031542Z.gz"));
    }

    #[test]
    fn bundle_name_matches_br8_format() {
        let name = ArchiveNamer::bundle_name("app-server-logs", "var-log-app", "2026-07-25", Path::new("/var/log/app"));
        assert_eq!(
            name,
            PathBuf::from("/var/log/app/app-server-logs.var-log-app.2026-07-25.tar.gz")
        );
    }

    #[test]
    fn bundle_key_daily_uses_calendar_date_in_target_timezone() {
        let tz: Tz = "UTC".parse().unwrap();
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 23, 59, 0).unwrap();
        let key = BundleKey::compute(basis, BundleKind::Daily, &tz);
        assert_eq!(key.0, "2026-07-25");
    }

    #[test]
    fn bundle_key_respects_configured_timezone_at_day_boundary() {
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        // UTC 2026-07-25T23:00:00 は Asia/Tokyo(UTC+9)では2026-07-26
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 23, 0, 0).unwrap();
        let key = BundleKey::compute(basis, BundleKind::Daily, &tokyo);
        assert_eq!(key.0, "2026-07-26");
    }

    #[test]
    fn bundle_key_monthly_groups_by_year_month() {
        let tz: Tz = "UTC".parse().unwrap();
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 0, 0, 0).unwrap();
        let key = BundleKey::compute(basis, BundleKind::Monthly, &tz);
        assert_eq!(key.0, "2026-07");
    }

    #[test]
    fn resolve_timezone_accepts_a_valid_iana_name() {
        assert!(resolve_timezone(Some("Asia/Tokyo")).is_ok());
    }

    #[test]
    fn resolve_timezone_rejects_an_unknown_name() {
        assert!(resolve_timezone(Some("Not/A_Zone")).is_err());
    }

    #[test]
    fn run_single_file_compresses_and_removes_original_by_default() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("app.log");
        fs::write(&source, b"hello world").unwrap();
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 3, 15, 42).unwrap();
        let candidate = make_candidate(dir.path(), "t", source.clone(), basis);
        let config = ArchiveConfig {
            enabled: true,
            format: ArchiveFormat::Gzip,
            ..ArchiveConfig::default()
        };

        let result = run_single_file(&candidate, &config).unwrap();
        assert!(result.created);
        assert!(result.destination.exists());
        assert!(!source.exists());

        let mtime = fs::metadata(&result.destination).unwrap().modified().unwrap();
        assert_eq!(DateTime::<Utc>::from(mtime).format("%Y-%m-%dT%H:%M:%S").to_string(), "2026-07-25T03:15:42");
    }

    #[test]
    fn run_single_file_keeps_original_when_configured() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("app.log");
        fs::write(&source, b"hello world").unwrap();
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 3, 15, 42).unwrap();
        let candidate = make_candidate(dir.path(), "t", source.clone(), basis);
        let config = ArchiveConfig {
            enabled: true,
            format: ArchiveFormat::Gzip,
            keep_original: true,
            ..ArchiveConfig::default()
        };

        let result = run_single_file(&candidate, &config).unwrap();
        assert!(result.created);
        assert!(source.exists());
    }

    #[test]
    fn run_single_file_is_idempotent_on_second_run() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("app.log");
        fs::write(&source, b"hello world").unwrap();
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 3, 15, 42).unwrap();
        let candidate = make_candidate(dir.path(), "t", source.clone(), basis);
        let config = ArchiveConfig {
            enabled: true,
            format: ArchiveFormat::Gzip,
            keep_original: true,
            ..ArchiveConfig::default()
        };

        let first = run_single_file(&candidate, &config).unwrap();
        assert!(first.created);
        let second = run_single_file(&candidate, &config).unwrap();
        assert!(!second.created);
        assert_eq!(first.destination, second.destination);
    }

    #[test]
    fn run_single_file_supports_zip_and_tar_gz_formats() {
        for format in [ArchiveFormat::Zip, ArchiveFormat::TarGz] {
            let dir = tempdir().unwrap();
            let source = dir.path().join("app.log");
            fs::write(&source, b"hello world").unwrap();
            let basis = Utc.with_ymd_and_hms(2026, 7, 25, 3, 15, 42).unwrap();
            let candidate = make_candidate(dir.path(), "t", source.clone(), basis);
            let config = ArchiveConfig {
                enabled: true,
                format,
                ..ArchiveConfig::default()
            };
            let result = run_single_file(&candidate, &config).unwrap();
            assert!(result.destination.exists(), "format {format:?} should produce an archive");
        }
    }

    #[test]
    fn run_bundle_groups_files_by_period_and_inherits_max_mtime() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        let file_a = dir.path().join("a.log");
        let file_b = sub.join("b.log");
        fs::write(&file_a, b"a").unwrap();
        fs::write(&file_b, b"b").unwrap();

        let basis_a = Utc.with_ymd_and_hms(2026, 7, 25, 1, 0, 0).unwrap();
        let basis_b = Utc.with_ymd_and_hms(2026, 7, 25, 5, 0, 0).unwrap();
        let candidates = vec![
            make_candidate(dir.path(), "t", file_a.clone(), basis_a),
            make_candidate(dir.path(), "t", file_b.clone(), basis_b),
        ];
        let target = make_target_ref(dir.path(), "t");
        let config = ArchiveConfig {
            enabled: true,
            bundle: BundleKind::Daily,
            bundle_timezone: Some("UTC".to_string()),
            ..ArchiveConfig::default()
        };

        let results = run_bundle("job", &target, &candidates, &config).unwrap();
        assert_eq!(results.len(), 1);
        let bundle = results[0].outcome.as_ref().unwrap();
        assert!(bundle.created);
        assert!(bundle.bundle_path.exists());
        assert!(!file_a.exists());
        assert!(!file_b.exists());

        let mtime = fs::metadata(&bundle.bundle_path).unwrap().modified().unwrap();
        assert_eq!(
            DateTime::<Utc>::from(mtime).format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-07-25T05:00:00"
        );
    }

    #[test]
    fn run_bundle_is_idempotent_and_flags_stale_members_as_warnings() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.log");
        fs::write(&file_a, b"a").unwrap();
        let basis_a = Utc.with_ymd_and_hms(2026, 7, 25, 1, 0, 0).unwrap();
        let target = make_target_ref(dir.path(), "t");
        let config = ArchiveConfig {
            enabled: true,
            bundle: BundleKind::Daily,
            bundle_timezone: Some("UTC".to_string()),
            keep_original: true,
            ..ArchiveConfig::default()
        };

        let first = run_bundle(
            "job",
            &target,
            &[make_candidate(dir.path(), "t", file_a.clone(), basis_a)],
            &config,
        )
        .unwrap();
        assert!(first[0].outcome.as_ref().unwrap().created);
        assert!(file_a.exists(), "keep_original: true should retain the source file");

        // 2回目: basis_datetime <= 既存バンドルのmtime のため冪等にスキップされる
        let second = run_bundle(
            "job",
            &target,
            &[make_candidate(dir.path(), "t", file_a.clone(), basis_a)],
            &config,
        )
        .unwrap();
        let second_outcome = second[0].outcome.as_ref().unwrap();
        assert!(!second_outcome.created);
        assert_eq!(second_outcome.included, vec![file_a.clone()]);
        assert!(second_outcome.stale.is_empty());

        // basis_datetimeが既存バンドルのmtimeより新しい場合はstale扱い(Warn)
        let newer_basis = Utc.with_ymd_and_hms(2026, 7, 25, 23, 0, 0).unwrap();
        let third = run_bundle(
            "job",
            &target,
            &[make_candidate(dir.path(), "t", file_a.clone(), newer_basis)],
            &config,
        )
        .unwrap();
        let third_outcome = third[0].outcome.as_ref().unwrap();
        assert!(!third_outcome.created);
        assert_eq!(third_outcome.stale, vec![file_a.clone()]);
    }

    #[test]
    fn run_bundle_returns_error_when_on_stale_bundle_member_is_error() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.log");
        fs::write(&file_a, b"a").unwrap();
        let basis_a = Utc.with_ymd_and_hms(2026, 7, 25, 1, 0, 0).unwrap();
        let target = make_target_ref(dir.path(), "t");
        let mut config = ArchiveConfig {
            enabled: true,
            bundle: BundleKind::Daily,
            bundle_timezone: Some("UTC".to_string()),
            keep_original: true,
            ..ArchiveConfig::default()
        };

        run_bundle(
            "job",
            &target,
            &[make_candidate(dir.path(), "t", file_a.clone(), basis_a)],
            &config,
        )
        .unwrap();

        config.on_stale_bundle_member = OnStaleBundleMember::Error;
        let newer_basis = Utc.with_ymd_and_hms(2026, 7, 25, 23, 0, 0).unwrap();
        let results = run_bundle(
            "job",
            &target,
            &[make_candidate(dir.path(), "t", file_a.clone(), newer_basis)],
            &config,
        )
        .unwrap();
        assert!(results[0].outcome.is_err());
    }

    proptest! {
        #[test]
        fn single_file_name_is_deterministic(basis in crate::test_support::arb_utc_datetime()) {
            let path = Path::new("/data/app.log");
            let first = ArchiveNamer::single_file_name(path, basis, ArchiveFormat::Gzip);
            let second = ArchiveNamer::single_file_name(path, basis, ArchiveFormat::Gzip);
            prop_assert_eq!(first, second);
        }

        #[test]
        fn bundle_key_compute_is_deterministic(basis in crate::test_support::arb_utc_datetime()) {
            let tz: Tz = "UTC".parse().unwrap();
            let first = BundleKey::compute(basis, BundleKind::Weekly, &tz);
            let second = BundleKey::compute(basis, BundleKind::Weekly, &tz);
            prop_assert_eq!(first, second);
        }

        /// PBT-02(往復性): 圧縮(gzip/zip/tar.gz)→解凍した内容が元のバイト列と一致する
        #[test]
        fn single_file_archive_round_trips_content(
            content in crate::test_support::arb_file_bytes(),
            basis in crate::test_support::arb_utc_datetime(),
            format in prop_oneof![
                Just(ArchiveFormat::Gzip),
                Just(ArchiveFormat::Zip),
                Just(ArchiveFormat::TarGz),
            ],
        ) {
            let dir = tempdir().unwrap();
            let source = dir.path().join("data.bin");
            fs::write(&source, &content).unwrap();
            let candidate = make_candidate(dir.path(), "t", source, basis);
            let config = ArchiveConfig {
                enabled: true,
                format,
                ..ArchiveConfig::default()
            };

            let result = run_single_file(&candidate, &config).unwrap();
            let decoded = decode_archive(&result.destination, format);
            prop_assert_eq!(decoded, content);
        }
    }

    fn decode_archive(path: &Path, format: ArchiveFormat) -> Vec<u8> {
        use std::io::Read;
        match format {
            ArchiveFormat::Gzip => {
                let file = fs::File::open(path).unwrap();
                let mut decoder = flate2::read::GzDecoder::new(file);
                let mut buf = Vec::new();
                decoder.read_to_end(&mut buf).unwrap();
                buf
            }
            ArchiveFormat::Zip => {
                let file = fs::File::open(path).unwrap();
                let mut zip = zip::ZipArchive::new(file).unwrap();
                let mut entry = zip.by_index(0).unwrap();
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf).unwrap();
                buf
            }
            ArchiveFormat::TarGz => {
                let file = fs::File::open(path).unwrap();
                let decoder = flate2::read::GzDecoder::new(file);
                let mut archive = tar::Archive::new(decoder);
                let mut entries = archive.entries().unwrap();
                let mut entry = entries.next().unwrap().unwrap();
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf).unwrap();
                buf
            }
        }
    }
}
