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

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::archive;
use crate::clock::Clock;
use crate::config::{BundleKind, JobConfig};
use crate::delete;
use crate::error::NeatnikError;
use crate::lock::JobLock;
use crate::relocate;
use crate::scan::{self, FileCandidate, WatchTargetRef, WriteGuardDetector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    Archive,
    Relocate,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeStatus {
    Skipped,
    Failed,
}

/// 1ファイル・1ステージの処理結果(スキップ・失敗のみ記録する。成功は`JobSummary`の集計値に反映する)
#[derive(Debug, Clone)]
pub struct StageOutcome {
    pub path: PathBuf,
    pub stage: StageKind,
    pub status: OutcomeStatus,
    pub reason: Option<String>,
}

/// ジョブ1回の実行結果サマリ(NFR-1: dry-run時も対象件数・合計サイズを表示するために使う)。
#[derive(Debug, Clone, Default)]
pub struct JobSummary {
    pub job_name: String,
    pub archived_count: u64,
    pub archived_bytes: u64,
    pub relocated_count: u64,
    pub relocated_bytes: u64,
    pub deleted_count: u64,
    pub deleted_bytes: u64,
    pub skipped: Vec<StageOutcome>,
    pub failed: Vec<StageOutcome>,
    pub safety_brake_triggered: bool,
}

fn elapsed_days(now: DateTime<Utc>, basis: DateTime<Utc>) -> i64 {
    (now - basis).num_days()
}

/// BR-16: ジョブロックを取得して`run_job_locked`を実行する。取得済みの場合は`Ok(None)`を返しジョブをスキップする
pub fn run_job<L: JobLock>(
    job: &JobConfig,
    clock: &dyn Clock,
    detector: &dyn WriteGuardDetector,
    lock: &L,
    dry_run: bool,
) -> Result<Option<JobSummary>, NeatnikError> {
    let outcome = lock.with_lock(&job.name, || run_job_locked(job, clock, detector, dry_run))?;
    outcome.transpose()
}

/// BR-15: 複数ジョブを逐次処理する。1ジョブの失敗は他のジョブの処理を止めない
pub fn run_all<L: JobLock>(
    jobs: &[JobConfig],
    clock: &dyn Clock,
    detector: &dyn WriteGuardDetector,
    lock: &L,
    dry_run: bool,
) -> Vec<(String, Result<Option<JobSummary>, NeatnikError>)> {
    jobs.iter()
        .map(|job| (job.name.clone(), run_job(job, clock, detector, lock, dry_run)))
        .collect()
}

fn run_job_locked(
    job: &JobConfig,
    clock: &dyn Clock,
    detector: &dyn WriteGuardDetector,
    dry_run: bool,
) -> Result<JobSummary, NeatnikError> {
    let now = clock.now();
    let mut summary = JobSummary {
        job_name: job.name.clone(),
        ..JobSummary::default()
    };
    let mut delete_candidates: Vec<FileCandidate> = Vec::new();

    for target in &job.targets {
        let candidates = scan::scan_target(&job.name, target, detector)?;
        let target_ref = WatchTargetRef {
            basedir: target.canonical_basedir(&job.name)?,
            name: target.resolved_name(),
        };

        let (in_use, usable): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|candidate| candidate.in_use);
        for candidate in in_use {
            summary.skipped.push(StageOutcome {
                path: candidate.path,
                stage: StageKind::Archive,
                status: OutcomeStatus::Skipped,
                reason: Some("file is in use (BR-7)".to_string()),
            });
        }

        let post_archive = run_archive_stage(job, &target_ref, usable, now, dry_run, &mut summary)?;

        for candidate in post_archive {
            let Some(relocated) = apply_relocate(job, candidate, now, dry_run, &mut summary) else {
                continue;
            };

            if !job.delete.enabled {
                continue;
            }
            if elapsed_days(now, relocated.basis_datetime) >= job.delete.after_days as i64 {
                delete_candidates.push(relocated);
            } else {
                summary.skipped.push(StageOutcome {
                    path: relocated.path,
                    stage: StageKind::Delete,
                    status: OutcomeStatus::Skipped,
                    reason: Some("delete grace period not elapsed (BR-1)".to_string()),
                });
            }
        }
    }

    if job.delete.enabled && !delete_candidates.is_empty() {
        apply_delete(&delete_candidates, job, dry_run, &mut summary);
    }

    Ok(summary)
}

/// BR-1/BR-8/BR-9/BR-10/BR-3: 猶予期間を満たす候補を単体/バンドルで圧縮し、以降の退避・削除段へ引き継ぐ候補を返す
fn run_archive_stage(
    job: &JobConfig,
    target_ref: &WatchTargetRef,
    candidates: Vec<FileCandidate>,
    now: DateTime<Utc>,
    dry_run: bool,
    summary: &mut JobSummary,
) -> Result<Vec<FileCandidate>, NeatnikError> {
    if !job.archive.enabled {
        return Ok(candidates);
    }

    let (eligible, not_eligible): (Vec<FileCandidate>, Vec<FileCandidate>) = candidates
        .into_iter()
        .partition(|candidate| elapsed_days(now, candidate.basis_datetime) >= job.archive.after_days as i64);

    for candidate in &not_eligible {
        summary.skipped.push(StageOutcome {
            path: candidate.path.clone(),
            stage: StageKind::Archive,
            status: OutcomeStatus::Skipped,
            reason: Some("archive grace period not elapsed (BR-1)".to_string()),
        });
    }

    if dry_run {
        summary.archived_count += eligible.len() as u64;
        summary.archived_bytes += eligible.iter().map(|candidate| candidate.size_bytes).sum::<u64>();
        let mut carried = eligible;
        carried.extend(not_eligible);
        return Ok(carried);
    }

    let mut carried = match job.archive.bundle {
        BundleKind::None => run_single_file_archive(job, eligible, summary),
        BundleKind::Daily | BundleKind::Weekly | BundleKind::Monthly => {
            run_bundle_archive(job, target_ref, eligible, summary)?
        }
    };

    carried.extend(not_eligible);
    Ok(carried)
}

fn run_single_file_archive(job: &JobConfig, eligible: Vec<FileCandidate>, summary: &mut JobSummary) -> Vec<FileCandidate> {
    let mut carried = Vec::with_capacity(eligible.len());
    for candidate in eligible {
        match archive::run_single_file(&candidate, &job.archive) {
            Ok(result) => {
                summary.archived_count += 1;
                summary.archived_bytes += candidate.size_bytes;
                carried.push(FileCandidate {
                    path: result.destination,
                    ..candidate
                });
            }
            Err(err) => {
                summary.failed.push(StageOutcome {
                    path: candidate.path,
                    stage: StageKind::Archive,
                    status: OutcomeStatus::Failed,
                    reason: Some(err.to_string()),
                });
            }
        }
    }
    carried
}

fn run_bundle_archive(
    job: &JobConfig,
    target_ref: &WatchTargetRef,
    eligible: Vec<FileCandidate>,
    summary: &mut JobSummary,
) -> Result<Vec<FileCandidate>, NeatnikError> {
    let groups = archive::run_bundle(&job.name, target_ref, &eligible, &job.archive)?;
    let by_path: HashMap<PathBuf, FileCandidate> = eligible.into_iter().map(|c| (c.path.clone(), c)).collect();

    let mut carried = Vec::new();
    for group in groups {
        match group.outcome {
            Ok(bundle) => {
                let included: Vec<&FileCandidate> = bundle.included.iter().filter_map(|path| by_path.get(path)).collect();
                if !included.is_empty() {
                    let max_basis = included
                        .iter()
                        .map(|candidate| candidate.basis_datetime)
                        .max()
                        .expect("included is non-empty");
                    let total_bytes: u64 = included.iter().map(|candidate| candidate.size_bytes).sum();
                    summary.archived_count += included.len() as u64;
                    summary.archived_bytes += total_bytes;
                    let bundle_size = fs::metadata(&bundle.bundle_path).map(|m| m.len()).unwrap_or(0);
                    carried.push(FileCandidate {
                        path: bundle.bundle_path,
                        target: target_ref.clone(),
                        basis_datetime: max_basis,
                        size_bytes: bundle_size,
                        in_use: false,
                    });
                }
                for stale_path in &bundle.stale {
                    summary.skipped.push(StageOutcome {
                        path: stale_path.clone(),
                        stage: StageKind::Archive,
                        status: OutcomeStatus::Skipped,
                        reason: Some("bundle member newer than existing bundle (BR-3)".to_string()),
                    });
                    if let Some(original) = by_path.get(stale_path) {
                        carried.push(original.clone());
                    }
                }
            }
            Err(err) => {
                for path in &group.members {
                    summary.failed.push(StageOutcome {
                        path: path.clone(),
                        stage: StageKind::Archive,
                        status: OutcomeStatus::Failed,
                        reason: Some(err.to_string()),
                    });
                }
            }
        }
    }
    Ok(carried)
}

/// BR-11/BR-12: 退避段を評価する。失敗・衝突スキップ時は`None`を返し以降のカスケードを打ち切る
/// (退避が完了していない状態で削除段に進むと、コピー元しか存在しないファイルを失う恐れがあるため安全側に倒す)
fn apply_relocate(
    job: &JobConfig,
    mut current: FileCandidate,
    now: DateTime<Utc>,
    dry_run: bool,
    summary: &mut JobSummary,
) -> Option<FileCandidate> {
    if !job.relocate.enabled {
        return Some(current);
    }
    if elapsed_days(now, current.basis_datetime) < job.relocate.after_days as i64 {
        summary.skipped.push(StageOutcome {
            path: current.path.clone(),
            stage: StageKind::Relocate,
            status: OutcomeStatus::Skipped,
            reason: Some("relocate grace period not elapsed (BR-1)".to_string()),
        });
        return Some(current);
    }
    if dry_run {
        summary.relocated_count += 1;
        summary.relocated_bytes += current.size_bytes;
        return Some(current);
    }

    match relocate::run(&current, &job.relocate) {
        Ok(result) => match result.outcome {
            relocate::RelocateOutcome::Moved => {
                summary.relocated_count += 1;
                summary.relocated_bytes += current.size_bytes;
                current.path = result.destination;
                Some(current)
            }
            relocate::RelocateOutcome::Skipped => {
                summary.skipped.push(StageOutcome {
                    path: current.path,
                    stage: StageKind::Relocate,
                    status: OutcomeStatus::Skipped,
                    reason: Some("destination conflict, on_conflict=skip (BR-12)".to_string()),
                });
                None
            }
        },
        Err(err) => {
            summary.failed.push(StageOutcome {
                path: current.path,
                stage: StageKind::Relocate,
                status: OutcomeStatus::Failed,
                reason: Some(err.to_string()),
            });
            None
        }
    }
}

/// BR-13/BR-14: ジョブ内の削除対象をまとめてセーフティブレーキ評価にかけ、削除を実行する
fn apply_delete(candidates: &[FileCandidate], job: &JobConfig, dry_run: bool, summary: &mut JobSummary) {
    let report = delete::run(candidates, &job.delete.safety_brake, dry_run);
    summary.safety_brake_triggered = report.evaluation.triggered && job.delete.safety_brake.enforce;

    for (result, candidate) in report.results.into_iter().zip(candidates.iter()) {
        match result.outcome {
            delete::DeleteOutcome::Deleted | delete::DeleteOutcome::DryRun => {
                summary.deleted_count += 1;
                summary.deleted_bytes += candidate.size_bytes;
            }
            delete::DeleteOutcome::Blocked => {
                summary.skipped.push(StageOutcome {
                    path: result.path,
                    stage: StageKind::Delete,
                    status: OutcomeStatus::Skipped,
                    reason: Some("safety brake triggered (BR-13)".to_string()),
                });
            }
            delete::DeleteOutcome::Failed => {
                summary.failed.push(StageOutcome {
                    path: result.path,
                    stage: StageKind::Delete,
                    status: OutcomeStatus::Failed,
                    reason: Some("failed to delete file".to_string()),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    use crate::clock::FixedClock;
    use crate::config::{
        ArchiveConfig, ArchiveFormat, ConflictPolicy, DeleteConfig, LayoutKind, RelocateConfig, SafetyBrakeConfig, WatchTarget,
    };
    use crate::lock::FileJobLock;

    struct NeverInUse;
    impl WriteGuardDetector for NeverInUse {
        fn is_in_use(&self, _path: &std::path::Path) -> bool {
            false
        }
    }

    struct AlwaysInUse;
    impl WriteGuardDetector for AlwaysInUse {
        fn is_in_use(&self, _path: &std::path::Path) -> bool {
            true
        }
    }

    fn set_old_mtime(path: &std::path::Path, when: DateTime<Utc>) {
        crate::archive::set_mtime(path, when).unwrap();
    }

    fn base_job(name: &str, basedir: &std::path::Path, destination: Option<PathBuf>) -> JobConfig {
        JobConfig {
            name: name.to_string(),
            targets: vec![WatchTarget {
                basedir: basedir.to_path_buf(),
                name: None,
                include: vec!["*.log".to_string()],
                exclude: vec![],
                basis: crate::config::BasisKind::Mtime,
                filename_date_rules: vec![],
            }],
            archive: ArchiveConfig {
                enabled: true,
                after_days: 7,
                format: ArchiveFormat::Gzip,
                ..ArchiveConfig::default()
            },
            relocate: RelocateConfig {
                enabled: true,
                after_days: 30,
                destination,
                layout: LayoutKind::Preserve,
                on_conflict: ConflictPolicy::Rename,
            },
            delete: DeleteConfig {
                enabled: true,
                after_days: 365,
                safety_brake: SafetyBrakeConfig::default(),
            },
        }
    }

    #[test]
    fn cascades_archive_relocate_delete_when_all_thresholds_are_zero() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        set_old_mtime(&file, old);

        let mut job = base_job("job", source_dir.path(), Some(dest_dir.path().to_path_buf()));
        job.archive.after_days = 0;
        job.relocate.after_days = 0;
        job.delete.after_days = 0;

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false).unwrap().unwrap();

        assert_eq!(summary.archived_count, 1);
        assert_eq!(summary.relocated_count, 1);
        assert_eq!(summary.deleted_count, 1);
        assert!(summary.failed.is_empty());

        // 最終的にアーカイブが作成された上で退避・削除まで完了し、どこにも実体が残らない
        let remaining: Vec<_> = walkdir::WalkDir::new(dest_dir.path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();
        assert!(remaining.is_empty(), "the archived+relocated file should have been deleted in the same run");
    }

    #[test]
    fn skips_files_that_are_in_use() {
        let source_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        set_old_mtime(&file, Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());

        let job = base_job("job", source_dir.path(), None);
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &AlwaysInUse, &lock, false).unwrap().unwrap();

        assert_eq!(summary.archived_count, 0);
        assert!(file.exists());
        assert!(summary
            .skipped
            .iter()
            .any(|outcome| outcome.stage == StageKind::Archive && outcome.reason.as_deref() == Some("file is in use (BR-7)")));
    }

    #[test]
    fn leaves_files_untouched_before_their_grace_period_elapses() {
        let source_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        // mtimeは「今」のまま(N1=7日の猶予に満たない)

        let job = base_job("job", source_dir.path(), None);
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false).unwrap().unwrap();

        assert_eq!(summary.archived_count, 0);
        assert!(file.exists());
    }

    #[test]
    fn dry_run_reports_counts_without_touching_files() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        set_old_mtime(&file, Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());

        let mut job = base_job("job", source_dir.path(), Some(dest_dir.path().to_path_buf()));
        job.archive.after_days = 0;
        job.relocate.after_days = 0;
        job.delete.after_days = 0;

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, true).unwrap().unwrap();

        assert_eq!(summary.archived_count, 1);
        assert_eq!(summary.relocated_count, 1);
        assert_eq!(summary.deleted_count, 1);
        assert!(file.exists(), "dry-run must not touch the original file");
    }

    #[test]
    fn skips_the_job_when_already_locked() {
        let source_dir = tempdir().unwrap();
        let job = base_job("job", source_dir.path(), None);
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());

        let path = source_dir.path().join(format!(".{}.lock", job.name));
        let held_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .unwrap();
        let mut held_lock = fd_lock::RwLock::new(held_file);
        let _guard = held_lock.write().unwrap();

        let outcome = run_job(&job, &clock, &NeverInUse, &lock, false).unwrap();
        assert!(outcome.is_none());
    }

    #[test]
    fn blocks_delete_when_safety_brake_enforces() {
        let source_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        set_old_mtime(&file, Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());

        let mut job = base_job("job", source_dir.path(), None);
        job.archive.enabled = false;
        job.relocate.enabled = false;
        job.delete.after_days = 0;
        job.delete.safety_brake = SafetyBrakeConfig {
            enforce: true,
            count_threshold: Some(0),
            size_threshold_gb: None,
        };

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false).unwrap().unwrap();

        assert_eq!(summary.deleted_count, 0);
        assert!(summary.safety_brake_triggered);
        assert!(file.exists());
    }

    #[test]
    fn bundles_multiple_files_into_a_single_carried_candidate_for_relocation() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file_a = source_dir.path().join("a.log");
        let file_b = source_dir.path().join("b.log");
        fs::write(&file_a, b"a").unwrap();
        fs::write(&file_b, b"b").unwrap();
        let basis = Utc.with_ymd_and_hms(2020, 1, 1, 1, 0, 0).unwrap();
        set_old_mtime(&file_a, basis);
        set_old_mtime(&file_b, basis);

        let mut job = base_job("job", source_dir.path(), Some(dest_dir.path().to_path_buf()));
        job.archive.bundle = BundleKind::Daily;
        job.archive.bundle_timezone = Some("UTC".to_string());
        job.relocate.after_days = 0;
        job.delete.enabled = false;

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false).unwrap().unwrap();

        assert_eq!(summary.archived_count, 2);
        assert_eq!(summary.relocated_count, 1, "the bundle should be relocated as a single unit");
        assert!(!file_a.exists());
        assert!(!file_b.exists());
    }

    #[test]
    fn run_all_processes_every_job_independently() {
        let source_dir = tempdir().unwrap();
        let job_a = base_job("job-a", source_dir.path(), None);
        let job_b = base_job("job-b", source_dir.path(), None);
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());

        let results = run_all(&[job_a, job_b], &clock, &NeverInUse, &lock, false);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
    }
}
