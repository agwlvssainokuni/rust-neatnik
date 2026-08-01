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

//! 退避(保管フォルダへの移動)処理(FR-3、BR-11、BR-12)。

use std::fs;
use std::path::{Path, PathBuf};

use crate::archive::set_mtime;
use crate::config::{ConflictPolicy, LayoutKind, RelocateConfig};
use crate::error::{NeatnikError, RelocateError};
use crate::scan::FileCandidate;

/// 退避処理1件の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelocateResult {
    pub destination: PathBuf,
    pub outcome: RelocateOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocateOutcome {
    Moved,
    /// `on_conflict: skip`により何もせず対象ファイルをそのまま残した(BR-12)
    Skipped,
}

/// FR-3/BR-11/BR-12: ファイルを保管先へコピー(mtime・パーミッション保持)後、元ファイルを削除する
pub fn run(candidate: &FileCandidate, config: &RelocateConfig) -> Result<RelocateResult, NeatnikError> {
    let destination_base = compute_destination(candidate, config)?;
    let destination = match resolve_conflict(destination_base, config.on_conflict)? {
        Some(path) => path,
        None => {
            return Ok(RelocateResult {
                destination: candidate.path.clone(),
                outcome: RelocateOutcome::Skipped,
            })
        }
    };

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| RelocateError::Copy {
            from: candidate.path.clone(),
            to: destination.clone(),
            source,
        })?;
    }

    // std::fs::copyはUnix系ではパーミッション(モードビット)も複製する(BR-11)
    fs::copy(&candidate.path, &destination).map_err(|source| RelocateError::Copy {
        from: candidate.path.clone(),
        to: destination.clone(),
        source,
    })?;

    set_mtime(&destination, candidate.basis_datetime).map_err(|source| RelocateError::Copy {
        from: candidate.path.clone(),
        to: destination.clone(),
        source,
    })?;

    fs::remove_file(&candidate.path).map_err(|source| RelocateError::Copy {
        from: candidate.path.clone(),
        to: destination.clone(),
        source,
    })?;

    Ok(RelocateResult {
        destination,
        outcome: RelocateOutcome::Moved,
    })
}

fn compute_destination(candidate: &FileCandidate, config: &RelocateConfig) -> Result<PathBuf, RelocateError> {
    let base = config.destination.clone().ok_or_else(|| RelocateError::Copy {
        from: candidate.path.clone(),
        to: PathBuf::new(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "relocate.destination is not configured"),
    })?;

    let relative = match config.layout {
        LayoutKind::Preserve => candidate
            .path
            .strip_prefix(&candidate.target.basedir)
            .unwrap_or(&candidate.path)
            .to_path_buf(),
        LayoutKind::YearMonth => {
            let file_name = candidate.path.file_name().map(PathBuf::from).unwrap_or_default();
            PathBuf::from(candidate.basis_datetime.format("%Y/%m").to_string()).join(file_name)
        }
    };

    Ok(base.join(relative))
}

/// BR-12: `on_conflict`設定に従う。`Some(path)`はコピー先、`None`はスキップを表す
fn resolve_conflict(destination: PathBuf, policy: ConflictPolicy) -> Result<Option<PathBuf>, RelocateError> {
    if !destination.exists() {
        return Ok(Some(destination));
    }

    match policy {
        ConflictPolicy::Skip => Ok(None),
        ConflictPolicy::Error => Err(RelocateError::Conflict { path: destination }),
        ConflictPolicy::Rename => {
            let parent = destination.parent().unwrap_or_else(|| Path::new("."));
            let stem = destination.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            let extension = destination.extension().and_then(|s| s.to_str());
            let mut counter: u32 = 1;
            loop {
                let candidate_name = match extension {
                    Some(ext) => format!("{stem}_{counter}.{ext}"),
                    None => format!("{stem}_{counter}"),
                };
                let candidate_path = parent.join(candidate_name);
                if !candidate_path.exists() {
                    return Ok(Some(candidate_path));
                }
                counter += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;
    use tempfile::tempdir;

    use crate::scan::WatchTargetRef;

    fn make_candidate(basedir: &Path, path: PathBuf, basis: chrono::DateTime<Utc>) -> FileCandidate {
        FileCandidate {
            size_bytes: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            path,
            target: WatchTargetRef {
                basedir: basedir.to_path_buf(),
                name: "t".to_string(),
            },
            basis_datetime: basis,
            in_use: false,
        }
    }

    #[test]
    fn compute_destination_preserves_relative_layout_by_default() {
        let basedir = Path::new("/data/app");
        let candidate = make_candidate(basedir, PathBuf::from("/data/app/sub/a.log"), Utc::now());
        let config = RelocateConfig {
            destination: Some(PathBuf::from("/mnt/storage")),
            layout: LayoutKind::Preserve,
            ..RelocateConfig::default()
        };
        let destination = compute_destination(&candidate, &config).unwrap();
        assert_eq!(destination, PathBuf::from("/mnt/storage/sub/a.log"));
    }

    #[test]
    fn compute_destination_uses_year_month_layout_from_basis_datetime() {
        let basedir = Path::new("/data/app");
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 0, 0, 0).unwrap();
        let candidate = make_candidate(basedir, PathBuf::from("/data/app/sub/a.log"), basis);
        let config = RelocateConfig {
            destination: Some(PathBuf::from("/mnt/storage")),
            layout: LayoutKind::YearMonth,
            ..RelocateConfig::default()
        };
        let destination = compute_destination(&candidate, &config).unwrap();
        assert_eq!(destination, PathBuf::from("/mnt/storage/2026/07/a.log"));
    }

    #[test]
    fn resolve_conflict_returns_the_path_unchanged_when_no_conflict() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.log");
        let resolved = resolve_conflict(target.clone(), ConflictPolicy::Error).unwrap();
        assert_eq!(resolved, Some(target));
    }

    #[test]
    fn resolve_conflict_skip_returns_none_when_destination_exists() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.log");
        fs::write(&target, b"x").unwrap();
        let resolved = resolve_conflict(target, ConflictPolicy::Skip).unwrap();
        assert_eq!(resolved, None);
    }

    #[test]
    fn resolve_conflict_error_fails_when_destination_exists() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.log");
        fs::write(&target, b"x").unwrap();
        assert!(resolve_conflict(target, ConflictPolicy::Error).is_err());
    }

    #[test]
    fn resolve_conflict_rename_finds_the_next_free_suffix() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("a.log");
        fs::write(&target, b"x").unwrap();
        fs::write(dir.path().join("a_1.log"), b"x").unwrap();
        let resolved = resolve_conflict(target, ConflictPolicy::Rename).unwrap();
        assert_eq!(resolved, Some(dir.path().join("a_2.log")));
    }

    #[test]
    fn run_moves_file_and_preserves_basis_datetime_as_mtime() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let source = source_dir.path().join("a.log");
        fs::write(&source, b"hello").unwrap();
        let basis = Utc.with_ymd_and_hms(2026, 7, 25, 3, 15, 42).unwrap();
        let candidate = make_candidate(source_dir.path(), source.clone(), basis);
        let config = RelocateConfig {
            enabled: true,
            destination: Some(dest_dir.path().to_path_buf()),
            layout: LayoutKind::Preserve,
            on_conflict: ConflictPolicy::Rename,
            ..RelocateConfig::default()
        };

        let result = run(&candidate, &config).unwrap();
        assert_eq!(result.outcome, RelocateOutcome::Moved);
        assert!(!source.exists());
        assert!(result.destination.exists());

        let mtime = fs::metadata(&result.destination).unwrap().modified().unwrap();
        assert_eq!(
            chrono::DateTime::<Utc>::from(mtime).format("%Y-%m-%dT%H:%M:%S").to_string(),
            "2026-07-25T03:15:42"
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_preserves_permission_bits_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let source = source_dir.path().join("a.log");
        fs::write(&source, b"hello").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();

        let candidate = make_candidate(source_dir.path(), source.clone(), Utc::now());
        let config = RelocateConfig {
            enabled: true,
            destination: Some(dest_dir.path().to_path_buf()),
            layout: LayoutKind::Preserve,
            on_conflict: ConflictPolicy::Rename,
            ..RelocateConfig::default()
        };

        let result = run(&candidate, &config).unwrap();
        let mode = fs::metadata(&result.destination).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn run_skips_without_touching_source_when_conflict_policy_is_skip() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let source = source_dir.path().join("a.log");
        fs::write(&source, b"hello").unwrap();
        fs::write(dest_dir.path().join("a.log"), b"existing").unwrap();

        let candidate = make_candidate(source_dir.path(), source.clone(), Utc::now());
        let config = RelocateConfig {
            enabled: true,
            destination: Some(dest_dir.path().to_path_buf()),
            layout: LayoutKind::Preserve,
            on_conflict: ConflictPolicy::Skip,
            ..RelocateConfig::default()
        };

        let result = run(&candidate, &config).unwrap();
        assert_eq!(result.outcome, RelocateOutcome::Skipped);
        assert!(source.exists());
        assert_eq!(fs::read(dest_dir.path().join("a.log")).unwrap(), b"existing");
    }

    proptest! {
        #[test]
        fn relocated_file_mtime_always_matches_basis_datetime(basis in crate::test_support::arb_utc_datetime()) {
            let source_dir = tempdir().unwrap();
            let dest_dir = tempdir().unwrap();
            let source = source_dir.path().join("a.log");
            fs::write(&source, b"hello").unwrap();
            let candidate = make_candidate(source_dir.path(), source.clone(), basis);
            let config = RelocateConfig {
                enabled: true,
                destination: Some(dest_dir.path().to_path_buf()),
                layout: LayoutKind::Preserve,
                on_conflict: ConflictPolicy::Rename,
                ..RelocateConfig::default()
            };

            let result = run(&candidate, &config).unwrap();
            let mtime = fs::metadata(&result.destination).unwrap().modified().unwrap();
            let actual = chrono::DateTime::<Utc>::from(mtime).format("%Y-%m-%dT%H:%M:%S").to_string();
            let expected = basis.format("%Y-%m-%dT%H:%M:%S").to_string();
            prop_assert_eq!(actual, expected);
        }
    }
}
