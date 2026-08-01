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

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use walkdir::WalkDir;

use crate::config::{is_within_basedir, BasisKind, FilenameDateRule, WatchTarget};
use crate::error::{NeatnikError, ScanError};

/// スキャン元の`WatchTarget`への参照(basedir正規化後の絶対パス・解決済みターゲット名)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchTargetRef {
    pub basedir: PathBuf,
    pub name: String,
}

/// スキャンで検出されたファイル1件(domain-entities.md)。
#[derive(Debug, Clone)]
pub struct FileCandidate {
    pub path: PathBuf,
    pub target: WatchTargetRef,
    pub basis_datetime: DateTime<Utc>,
    pub size_bytes: u64,
    pub in_use: bool,
}

/// 他プロセスが書き込み中かをベストエフォートで判定する(NFR-OS)。
pub trait WriteGuardDetector {
    fn is_in_use(&self, path: &Path) -> bool;
}

/// Linux/macOS向け実装(BR-7): flockによるアドバイザリロック検知 + 直近更新時刻のヒューリスティック
#[derive(Debug, Clone, Copy)]
pub struct UnixWriteGuardDetector {
    pub recent_threshold: Duration,
}

impl Default for UnixWriteGuardDetector {
    fn default() -> Self {
        Self {
            recent_threshold: Duration::from_secs(5),
        }
    }
}

impl WriteGuardDetector for UnixWriteGuardDetector {
    fn is_in_use(&self, path: &Path) -> bool {
        Self::is_locked(path) || Self::is_recently_modified(path, self.recent_threshold)
    }
}

impl UnixWriteGuardDetector {
    fn is_locked(path: &Path) -> bool {
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut lock = fd_lock::RwLock::new(file);
        let is_locked = lock.try_write().is_err();
        is_locked
    }

    fn is_recently_modified(path: &Path, threshold: Duration) -> bool {
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };
        match metadata.modified() {
            Ok(modified) => modified.elapsed().map(|elapsed| elapsed < threshold).unwrap_or(false),
            Err(_) => false,
        }
    }
}

/// Windows向け実装(BR-7): 共有モードでのオープン試行による`ERROR_SHARING_VIOLATION`検知
#[cfg(windows)]
mod windows_impl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_SHARING_VIOLATION, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, GENERIC_READ, OPEN_EXISTING,
    };

    use super::WriteGuardDetector;

    #[derive(Debug, Default, Clone, Copy)]
    pub struct WindowsWriteGuardDetector;

    impl WriteGuardDetector for WindowsWriteGuardDetector {
        fn is_in_use(&self, path: &Path) -> bool {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            unsafe {
                // 共有モード0(排他)でのオープン試行。内容・タイムスタンプは変更しない
                let handle = CreateFileW(
                    wide.as_ptr(),
                    GENERIC_READ,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    std::ptr::null_mut(),
                );
                if handle == INVALID_HANDLE_VALUE {
                    return GetLastError() == ERROR_SHARING_VIOLATION;
                }
                CloseHandle(handle);
                false
            }
        }
    }
}

#[cfg(windows)]
pub use windows_impl::WindowsWriteGuardDetector;

#[cfg(unix)]
pub fn platform_write_guard_detector() -> impl WriteGuardDetector {
    UnixWriteGuardDetector::default()
}

#[cfg(windows)]
pub fn platform_write_guard_detector() -> impl WriteGuardDetector {
    WindowsWriteGuardDetector::default()
}

/// `WatchTarget`1件を走査し、`FileCandidate`のリストを返す。書き込み中判定・パストラバーサル対策を含む
pub fn scan_target(
    job_name: &str,
    target: &WatchTarget,
    detector: &dyn WriteGuardDetector,
) -> Result<Vec<FileCandidate>, NeatnikError> {
    let canonical_basedir = target.canonical_basedir(job_name)?;
    let target_ref = WatchTargetRef {
        basedir: canonical_basedir.clone(),
        name: target.resolved_name(),
    };

    let mut candidates = Vec::new();
    for entry in WalkDir::new(&canonical_basedir).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(error = %err, "failed to read directory entry during scan");
                continue;
            }
        };

        let file_type = entry.file_type();
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let relative = match path.strip_prefix(&canonical_basedir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !is_included(relative, &target.include, &target.exclude) {
            continue;
        }

        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(path = %path.display(), error = %err, "failed to canonicalize candidate path, skipping");
                continue;
            }
        };
        if !is_within_basedir(&canonical_basedir, &canonical_path) {
            tracing::warn!(
                path = %canonical_path.display(),
                basedir = %canonical_basedir.display(),
                "candidate escapes basedir, skipping (path traversal guard)"
            );
            continue;
        }

        let metadata = match fs::metadata(&canonical_path) {
            Ok(m) => m,
            Err(err) => {
                tracing::warn!(path = %canonical_path.display(), error = %err, "failed to read metadata, skipping");
                continue;
            }
        };

        let basis_datetime = match resolve_basis_datetime(target, &canonical_path, &metadata) {
            Ok(dt) => dt,
            Err(err) => {
                tracing::warn!(path = %canonical_path.display(), error = %err, "failed to resolve basis datetime, skipping");
                continue;
            }
        };

        let in_use = detector.is_in_use(&canonical_path);

        candidates.push(FileCandidate {
            path: canonical_path,
            target: target_ref.clone(),
            basis_datetime,
            size_bytes: metadata.len(),
            in_use,
        });
    }

    Ok(candidates)
}

/// BR-6: `exclude`は`include`より優先する。両パターンとも`basedir`からの相対パスに対して評価する
fn is_included(relative: &Path, include: &[String], exclude: &[String]) -> bool {
    let matches_any = |patterns: &[String]| {
        patterns.iter().any(|pattern| {
            glob::Pattern::new(pattern)
                .map(|p| p.matches_path(relative))
                .unwrap_or(false)
        })
    };
    if include.is_empty() {
        return false;
    }
    matches_any(include) && !matches_any(exclude)
}

fn resolve_basis_datetime(
    target: &WatchTarget,
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<DateTime<Utc>, ScanError> {
    match target.basis {
        BasisKind::Mtime => metadata_time_to_utc(metadata.modified(), path),
        BasisKind::Ctime => ctime_to_utc(metadata, path),
        BasisKind::FilenameDate => {
            let file_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                ScanError::NoDateRuleMatched {
                    path: path.to_path_buf(),
                }
            })?;
            extract_filename_date(&target.filename_date_rules, file_name).ok_or_else(|| {
                ScanError::NoDateRuleMatched {
                    path: path.to_path_buf(),
                }
            })
        }
    }
}

fn metadata_time_to_utc(
    result: std::io::Result<std::time::SystemTime>,
    path: &Path,
) -> Result<DateTime<Utc>, ScanError> {
    let system_time = result.map_err(|source| ScanError::Metadata {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(DateTime::<Utc>::from(system_time))
}

#[cfg(unix)]
fn ctime_to_utc(metadata: &fs::Metadata, path: &Path) -> Result<DateTime<Utc>, ScanError> {
    use std::os::unix::fs::MetadataExt;
    let seconds = metadata.ctime();
    let nanos = metadata.ctime_nsec() as u32;
    Utc.timestamp_opt(seconds, nanos)
        .single()
        .ok_or_else(|| ScanError::Metadata {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid ctime value"),
        })
}

#[cfg(not(unix))]
fn ctime_to_utc(metadata: &fs::Metadata, path: &Path) -> Result<DateTime<Utc>, ScanError> {
    metadata_time_to_utc(metadata.created(), path)
}

/// BR-7.1: `FilenameDateRule`を上から順に照合し、最初にマッチ+パース成功したものを採用する
pub fn extract_filename_date(rules: &[FilenameDateRule], file_name: &str) -> Option<DateTime<Utc>> {
    rules.iter().find_map(|rule| try_match_rule(rule, file_name))
}

fn try_match_rule(rule: &FilenameDateRule, file_name: &str) -> Option<DateTime<Utc>> {
    let re = Regex::new(&rule.regex).ok()?;
    let captures = re.captures(file_name)?;
    let date_str = captures.name("date")?.as_str();
    parse_captured_date(date_str, &rule.format)
}

fn parse_captured_date(date_str: &str, format: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(date_str, format) {
        return Some(Utc.from_utc_datetime(&dt));
    }
    let date = NaiveDate::parse_from_str(date_str, format).ok()?;
    let dt = date.and_hms_opt(0, 0, 0)?;
    Some(Utc.from_utc_datetime(&dt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    use tempfile::tempdir;

    #[test]
    fn extract_filename_date_uses_first_matching_rule() {
        let rules = vec![
            FilenameDateRule {
                regex: r"^app_log\.(?P<date>\d{4}-\d{2}-\d{2})\.txt$".to_string(),
                format: "%Y-%m-%d".to_string(),
            },
            FilenameDateRule {
                regex: r"^access-(?P<date>\d{8})\.log$".to_string(),
                format: "%Y%m%d".to_string(),
            },
        ];
        let dt = extract_filename_date(&rules, "access-20260701.log").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-07-01");
    }

    #[test]
    fn extract_filename_date_returns_none_when_no_rule_matches() {
        let rules = vec![FilenameDateRule {
            regex: r"^app_log\.(?P<date>\d{4}-\d{2}-\d{2})\.txt$".to_string(),
            format: "%Y-%m-%d".to_string(),
        }];
        assert!(extract_filename_date(&rules, "unrelated.txt").is_none());
    }

    #[test]
    fn extract_filename_date_ignores_directory_components() {
        let rules = vec![FilenameDateRule {
            regex: r"^app_log\.(?P<date>\d{4}-\d{2}-\d{2})\.txt$".to_string(),
            format: "%Y-%m-%d".to_string(),
        }];
        // basename のみに照合するため、ディレクトリ名に日付相当の文字列があってもマッチしない
        assert!(extract_filename_date(&rules, "2026-07-01").is_none());
    }

    #[test]
    fn is_included_respects_exclude_precedence() {
        let include = vec!["**/*.log".to_string()];
        let exclude = vec!["current.log".to_string()];
        assert!(is_included(Path::new("app.log"), &include, &exclude));
        assert!(!is_included(Path::new("current.log"), &include, &exclude));
        assert!(!is_included(Path::new("app.txt"), &include, &exclude));
    }

    #[test]
    fn is_included_returns_false_when_include_is_empty() {
        assert!(!is_included(Path::new("app.log"), &[], &[]));
    }

    #[test]
    fn scan_target_finds_matching_files_and_skips_symlinks_and_excludes() {
        let dir = tempdir().unwrap();
        let basedir = dir.path();
        File::create(basedir.join("app.log")).unwrap();
        File::create(basedir.join("current.log")).unwrap();
        File::create(basedir.join("notes.txt")).unwrap();

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(basedir.join("app.log"), basedir.join("linked.log")).unwrap();
        }

        let target = WatchTarget {
            basedir: basedir.to_path_buf(),
            name: None,
            include: vec!["*.log".to_string()],
            exclude: vec!["current.log".to_string()],
            basis: BasisKind::Mtime,
            filename_date_rules: vec![],
        };

        struct NeverInUse;
        impl WriteGuardDetector for NeverInUse {
            fn is_in_use(&self, _path: &Path) -> bool {
                false
            }
        }

        let candidates = scan_target("test-job", &target, &NeverInUse).unwrap();
        let names: Vec<String> = candidates
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["app.log".to_string()]);
    }

    #[test]
    fn scan_target_resolves_basis_datetime_from_filename_when_configured() {
        let dir = tempdir().unwrap();
        let basedir = dir.path();
        let mut f = File::create(basedir.join("access-20260701.log")).unwrap();
        writeln!(f, "line").unwrap();

        let target = WatchTarget {
            basedir: basedir.to_path_buf(),
            name: None,
            include: vec!["*.log".to_string()],
            exclude: vec![],
            basis: BasisKind::FilenameDate,
            filename_date_rules: vec![FilenameDateRule {
                regex: r"^access-(?P<date>\d{8})\.log$".to_string(),
                format: "%Y%m%d".to_string(),
            }],
        };

        struct NeverInUse;
        impl WriteGuardDetector for NeverInUse {
            fn is_in_use(&self, _path: &Path) -> bool {
                false
            }
        }

        let candidates = scan_target("test-job", &target, &NeverInUse).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].basis_datetime.format("%Y-%m-%d").to_string(),
            "2026-07-01"
        );
    }

    #[test]
    fn scan_target_skips_file_with_no_matching_date_rule() {
        let dir = tempdir().unwrap();
        let basedir = dir.path();
        File::create(basedir.join("unrelated.log")).unwrap();

        let target = WatchTarget {
            basedir: basedir.to_path_buf(),
            name: None,
            include: vec!["*.log".to_string()],
            exclude: vec![],
            basis: BasisKind::FilenameDate,
            filename_date_rules: vec![FilenameDateRule {
                regex: r"^access-(?P<date>\d{8})\.log$".to_string(),
                format: "%Y%m%d".to_string(),
            }],
        };

        struct NeverInUse;
        impl WriteGuardDetector for NeverInUse {
            fn is_in_use(&self, _path: &Path) -> bool {
                false
            }
        }

        let candidates = scan_target("test-job", &target, &NeverInUse).unwrap();
        assert!(candidates.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn unix_detector_flags_recently_modified_files_as_in_use() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let detector = UnixWriteGuardDetector {
            recent_threshold: Duration::from_secs(5),
        };
        assert!(detector.is_in_use(file.path()));
    }

    #[cfg(unix)]
    #[test]
    fn unix_detector_ignores_old_unlocked_files() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let detector = UnixWriteGuardDetector {
            recent_threshold: Duration::from_millis(20),
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(!detector.is_in_use(file.path()));
    }

    #[cfg(unix)]
    #[test]
    fn unix_detector_flags_locked_files_even_with_old_mtime() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let detector = UnixWriteGuardDetector {
            recent_threshold: Duration::from_millis(1),
        };
        std::thread::sleep(Duration::from_millis(20));

        let opened = File::open(file.path()).unwrap();
        let mut lock = fd_lock::RwLock::new(opened);
        let _guard = lock.write().unwrap();

        assert!(detector.is_in_use(file.path()));
    }
}
