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

//! 全体オーケストレーション。1ジョブの`stages`を先頭から順に1エントリずつ処理し(BR-9)、
//! 複数ジョブの逐次処理(BR-15)を実装する。

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::archive;
use crate::clock::Clock;
use crate::config::{
    ArchiveConfig, BundleKind, DeleteConfig, JobConfig, RelocateConfig, StageConfig, WatchTarget,
};
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

/// 1ファイル・1ステージエントリの処理結果(スキップ・失敗のみ記録する。成功は`JobSummary`の集計値に反映する)
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
        .map(|job| {
            (
                job.name.clone(),
                run_job(job, clock, detector, lock, dry_run),
            )
        })
        .collect()
}

/// BR-9: `stages`を書かれた順序どおりに1エントリずつ処理する。並べ替えは行わない
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

    for stage in &job.stages {
        match stage {
            StageConfig::Archive(archive_config) => {
                run_archive_entry(
                    &job.name,
                    archive_config,
                    now,
                    dry_run,
                    detector,
                    &mut summary,
                )?;
            }
            StageConfig::Relocate(relocate_config) => {
                run_relocate_entry(
                    &job.name,
                    relocate_config,
                    now,
                    dry_run,
                    detector,
                    &mut summary,
                )?;
            }
            StageConfig::Delete(delete_config) => {
                run_delete_entry(
                    &job.name,
                    delete_config,
                    now,
                    dry_run,
                    detector,
                    &mut summary,
                )?;
            }
        }
    }

    Ok(summary)
}

/// 指定された`targets`をスキャンし、書き込み中のファイル(BR-7)をスキップした候補を返す
fn scan_targets(
    job_name: &str,
    targets: &[WatchTarget],
    detector: &dyn WriteGuardDetector,
    stage: StageKind,
    summary: &mut JobSummary,
) -> Result<Vec<FileCandidate>, NeatnikError> {
    let mut usable = Vec::new();
    for target in targets {
        let candidates = scan::scan_target(job_name, target, detector)?;
        for candidate in candidates {
            if candidate.in_use {
                summary.skipped.push(StageOutcome {
                    path: candidate.path,
                    stage,
                    status: OutcomeStatus::Skipped,
                    reason: Some("file is in use (BR-7)".to_string()),
                });
            } else {
                usable.push(candidate);
            }
        }
    }
    Ok(usable)
}

/// BR-8/BR-9/BR-10/BR-3: archiveエントリを実行する。ターゲット単位でスキャン・判定・圧縮する
/// (バンドル命名(BR-8)がターゲット単位のグルーピングを前提とするため、ターゲットをまたいで
/// 候補をまとめない)
fn run_archive_entry(
    job_name: &str,
    archive_config: &ArchiveConfig,
    now: DateTime<Utc>,
    dry_run: bool,
    detector: &dyn WriteGuardDetector,
    summary: &mut JobSummary,
) -> Result<(), NeatnikError> {
    for target in &archive_config.targets {
        let target_ref = WatchTargetRef {
            basedir: target.canonical_basedir(job_name)?,
            name: target.resolved_name(),
        };
        let candidates = scan::scan_target(job_name, target, detector)?;

        let (in_use, usable): (Vec<_>, Vec<_>) = candidates.into_iter().partition(|c| c.in_use);
        for candidate in in_use {
            summary.skipped.push(StageOutcome {
                path: candidate.path,
                stage: StageKind::Archive,
                status: OutcomeStatus::Skipped,
                reason: Some("file is in use (BR-7)".to_string()),
            });
        }

        let (eligible, not_eligible): (Vec<FileCandidate>, Vec<FileCandidate>) = usable
            .into_iter()
            .partition(|c| elapsed_days(now, c.basis_datetime) >= archive_config.after_days as i64);

        for candidate in &not_eligible {
            summary.skipped.push(StageOutcome {
                path: candidate.path.clone(),
                stage: StageKind::Archive,
                status: OutcomeStatus::Skipped,
                reason: Some("archive grace period not elapsed".to_string()),
            });
        }

        if dry_run {
            for candidate in &eligible {
                tracing::info!(
                    job = job_name,
                    stage = "archive",
                    path = %candidate.path.display(),
                    bytes = candidate.size_bytes,
                    dry_run = true,
                    "would archive file"
                );
            }
            summary.archived_count += eligible.len() as u64;
            summary.archived_bytes += eligible.iter().map(|c| c.size_bytes).sum::<u64>();
            continue;
        }

        match archive_config.bundle {
            BundleKind::None => {
                run_single_file_archive(job_name, eligible, archive_config, summary)
            }
            BundleKind::Daily | BundleKind::Weekly | BundleKind::Monthly => {
                run_bundle_archive(job_name, archive_config, &target_ref, eligible, summary)?;
            }
        }
    }
    Ok(())
}

fn run_single_file_archive(
    job_name: &str,
    eligible: Vec<FileCandidate>,
    archive_config: &ArchiveConfig,
    summary: &mut JobSummary,
) {
    for candidate in eligible {
        match archive::run_single_file(&candidate, archive_config) {
            Ok(result) => {
                summary.archived_count += 1;
                summary.archived_bytes += candidate.size_bytes;
                tracing::info!(
                    job = job_name,
                    stage = "archive",
                    path = %candidate.path.display(),
                    destination = %result.destination.display(),
                    bytes = candidate.size_bytes,
                    format = ?archive_config.format,
                    "archived file"
                );
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
}

fn run_bundle_archive(
    job_name: &str,
    archive_config: &ArchiveConfig,
    target_ref: &WatchTargetRef,
    eligible: Vec<FileCandidate>,
    summary: &mut JobSummary,
) -> Result<(), NeatnikError> {
    let groups = archive::run_bundle(&archive_config.name, target_ref, &eligible, archive_config)?;
    let by_path: HashMap<PathBuf, FileCandidate> =
        eligible.into_iter().map(|c| (c.path.clone(), c)).collect();

    for group in groups {
        match group.outcome {
            Ok(bundle) => {
                let included: Vec<&FileCandidate> = bundle
                    .included
                    .iter()
                    .filter_map(|path| by_path.get(path))
                    .collect();
                if !included.is_empty() {
                    let total_bytes: u64 =
                        included.iter().map(|candidate| candidate.size_bytes).sum();
                    summary.archived_count += included.len() as u64;
                    summary.archived_bytes += total_bytes;
                    tracing::info!(
                        job = job_name,
                        stage = "archive",
                        bundle = %bundle.bundle_path.display(),
                        member_count = included.len(),
                        members = ?included.iter().map(|c| c.path.display().to_string()).collect::<Vec<_>>(),
                        bytes = total_bytes,
                        "archived bundle"
                    );
                }
                for stale_path in &bundle.stale {
                    summary.skipped.push(StageOutcome {
                        path: stale_path.clone(),
                        stage: StageKind::Archive,
                        status: OutcomeStatus::Skipped,
                        reason: Some("bundle member newer than existing bundle (BR-3)".to_string()),
                    });
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
    Ok(())
}

/// BR-11/BR-12: relocateエントリを実行する
fn run_relocate_entry(
    job_name: &str,
    relocate_config: &RelocateConfig,
    now: DateTime<Utc>,
    dry_run: bool,
    detector: &dyn WriteGuardDetector,
    summary: &mut JobSummary,
) -> Result<(), NeatnikError> {
    let candidates = scan_targets(
        job_name,
        &relocate_config.targets,
        detector,
        StageKind::Relocate,
        summary,
    )?;

    for candidate in candidates {
        if elapsed_days(now, candidate.basis_datetime) < relocate_config.after_days as i64 {
            summary.skipped.push(StageOutcome {
                path: candidate.path,
                stage: StageKind::Relocate,
                status: OutcomeStatus::Skipped,
                reason: Some("relocate grace period not elapsed".to_string()),
            });
            continue;
        }

        if dry_run {
            tracing::info!(
                job = job_name,
                stage = "relocate",
                path = %candidate.path.display(),
                bytes = candidate.size_bytes,
                dry_run = true,
                "would relocate file"
            );
            summary.relocated_count += 1;
            summary.relocated_bytes += candidate.size_bytes;
            continue;
        }

        match relocate::run(&candidate, relocate_config) {
            Ok(result) => match result.outcome {
                relocate::RelocateOutcome::Moved => {
                    summary.relocated_count += 1;
                    summary.relocated_bytes += candidate.size_bytes;
                    tracing::info!(
                        job = job_name,
                        stage = "relocate",
                        path = %candidate.path.display(),
                        destination = %result.destination.display(),
                        bytes = candidate.size_bytes,
                        "relocated file"
                    );
                }
                relocate::RelocateOutcome::Skipped => {
                    summary.skipped.push(StageOutcome {
                        path: candidate.path,
                        stage: StageKind::Relocate,
                        status: OutcomeStatus::Skipped,
                        reason: Some("destination conflict, on_conflict=skip (BR-12)".to_string()),
                    });
                }
            },
            Err(err) => {
                summary.failed.push(StageOutcome {
                    path: candidate.path,
                    stage: StageKind::Relocate,
                    status: OutcomeStatus::Failed,
                    reason: Some(err.to_string()),
                });
            }
        }
    }
    Ok(())
}

/// BR-13/BR-14: deleteエントリを実行する。セーフティブレーキはこのエントリ単位で評価する
/// (2026-08-02改訂: ジョブ全体ではなくdeleteエントリ単位)
fn run_delete_entry(
    job_name: &str,
    delete_config: &DeleteConfig,
    now: DateTime<Utc>,
    dry_run: bool,
    detector: &dyn WriteGuardDetector,
    summary: &mut JobSummary,
) -> Result<(), NeatnikError> {
    let candidates = scan_targets(
        job_name,
        &delete_config.targets,
        detector,
        StageKind::Delete,
        summary,
    )?;

    let (eligible, not_eligible): (Vec<FileCandidate>, Vec<FileCandidate>) = candidates
        .into_iter()
        .partition(|c| elapsed_days(now, c.basis_datetime) >= delete_config.after_days as i64);

    for candidate in &not_eligible {
        summary.skipped.push(StageOutcome {
            path: candidate.path.clone(),
            stage: StageKind::Delete,
            status: OutcomeStatus::Skipped,
            reason: Some("delete grace period not elapsed".to_string()),
        });
    }

    if eligible.is_empty() {
        return Ok(());
    }

    let report = delete::run(&eligible, &delete_config.safety_brake, dry_run);
    summary.safety_brake_triggered |=
        report.evaluation.triggered && delete_config.safety_brake.enforce;

    for (result, candidate) in report.results.into_iter().zip(eligible.iter()) {
        match result.outcome {
            delete::DeleteOutcome::Deleted => {
                summary.deleted_count += 1;
                summary.deleted_bytes += candidate.size_bytes;
                tracing::info!(
                    job = job_name,
                    stage = "delete",
                    path = %result.path.display(),
                    bytes = candidate.size_bytes,
                    "deleted file"
                );
            }
            delete::DeleteOutcome::DryRun => {
                summary.deleted_count += 1;
                summary.deleted_bytes += candidate.size_bytes;
                tracing::info!(
                    job = job_name,
                    stage = "delete",
                    path = %result.path.display(),
                    bytes = candidate.size_bytes,
                    dry_run = true,
                    "would delete file"
                );
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use chrono::TimeZone;
    use tempfile::tempdir;

    use crate::clock::FixedClock;
    use crate::config::{ArchiveFormat, BasisKind, ConflictPolicy, LayoutKind, SafetyBrakeConfig};
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

    fn target(basedir: &std::path::Path, include: &str) -> WatchTarget {
        WatchTarget {
            basedir: basedir.to_path_buf(),
            name: None,
            include: vec![include.to_string()],
            exclude: vec![],
            basis: BasisKind::Mtime,
            filename_date_rules: vec![],
        }
    }

    fn archive_stage(basedir: &std::path::Path, after_days: u32) -> StageConfig {
        StageConfig::Archive(ArchiveConfig {
            name: "a".to_string(),
            targets: vec![target(basedir, "*.log")],
            after_days,
            format: ArchiveFormat::Gzip,
            ..ArchiveConfig::default()
        })
    }

    fn relocate_stage(
        basedir: &std::path::Path,
        after_days: u32,
        destination: PathBuf,
    ) -> StageConfig {
        StageConfig::Relocate(RelocateConfig {
            targets: vec![target(basedir, "*.gz")],
            after_days,
            destination: Some(destination),
            layout: LayoutKind::Preserve,
            on_conflict: ConflictPolicy::Rename,
        })
    }

    fn delete_stage(basedir: &std::path::Path, after_days: u32) -> StageConfig {
        StageConfig::Delete(DeleteConfig {
            targets: vec![target(basedir, "**/*")],
            after_days,
            safety_brake: SafetyBrakeConfig::default(),
        })
    }

    #[test]
    fn cascades_archive_relocate_delete_when_all_thresholds_are_zero() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        let old = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        set_old_mtime(&file, old);

        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![
                archive_stage(source_dir.path(), 0),
                relocate_stage(source_dir.path(), 0, dest_dir.path().to_path_buf()),
                delete_stage(dest_dir.path(), 0),
            ],
        };

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false)
            .unwrap()
            .unwrap();

        assert_eq!(summary.archived_count, 1);
        assert_eq!(summary.relocated_count, 1);
        assert_eq!(summary.deleted_count, 1);
        assert!(summary.failed.is_empty());

        let remaining: Vec<_> = walkdir::WalkDir::new(dest_dir.path())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();
        assert!(
            remaining.is_empty(),
            "the archived+relocated file should have been deleted in the same run"
        );
    }

    #[test]
    fn skips_files_that_are_in_use() {
        let source_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        set_old_mtime(&file, Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());

        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![archive_stage(source_dir.path(), 7)],
        };
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &AlwaysInUse, &lock, false)
            .unwrap()
            .unwrap();

        assert_eq!(summary.archived_count, 0);
        assert!(file.exists());
        assert!(summary
            .skipped
            .iter()
            .any(|outcome| outcome.stage == StageKind::Archive
                && outcome.reason.as_deref() == Some("file is in use (BR-7)")));
    }

    #[test]
    fn leaves_files_untouched_before_their_grace_period_elapses() {
        let source_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        // mtimeは「今」のまま(猶予に満たない)

        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![archive_stage(source_dir.path(), 7)],
        };
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false)
            .unwrap()
            .unwrap();

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

        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![
                archive_stage(source_dir.path(), 0),
                relocate_stage(source_dir.path(), 0, dest_dir.path().to_path_buf()),
                delete_stage(dest_dir.path(), 0),
            ],
        };

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, true)
            .unwrap()
            .unwrap();

        // dry-runは各エントリが「現時点でディスク上に存在するもの」だけを独立に報告する。
        // archiveの出力はdry-runでは実際には作られないため、relocate/delete(archiveの出力
        // パターンを監視するtargets)の対象はまだ存在せず0件になる。これは「メモリ上で候補を
        // 持ち越す」旧設計とは異なる、stagesの独立スキャン方式(2026-08-02改訂)の帰結
        assert_eq!(summary.archived_count, 1);
        assert_eq!(summary.relocated_count, 0);
        assert_eq!(summary.deleted_count, 0);
        assert!(file.exists(), "dry-run must not touch the original file");
    }

    #[test]
    fn skips_the_job_when_already_locked() {
        let source_dir = tempdir().unwrap();
        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![archive_stage(source_dir.path(), 7)],
        };
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

        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![StageConfig::Delete(DeleteConfig {
                targets: vec![target(source_dir.path(), "*.log")],
                after_days: 0,
                safety_brake: SafetyBrakeConfig {
                    enforce: true,
                    count_threshold: Some(0),
                    size_threshold_gb: None,
                },
            })],
        };

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false)
            .unwrap()
            .unwrap();

        assert_eq!(summary.deleted_count, 0);
        assert!(summary.safety_brake_triggered);
        assert!(file.exists());
    }

    #[test]
    fn bundles_multiple_files_and_relocates_the_bundle_as_a_single_unit() {
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file_a = source_dir.path().join("a.log");
        let file_b = source_dir.path().join("b.log");
        fs::write(&file_a, b"a").unwrap();
        fs::write(&file_b, b"b").unwrap();
        let basis = Utc.with_ymd_and_hms(2020, 1, 1, 1, 0, 0).unwrap();
        set_old_mtime(&file_a, basis);
        set_old_mtime(&file_b, basis);

        let job = JobConfig {
            name: "job".to_string(),
            stages: vec![
                StageConfig::Archive(ArchiveConfig {
                    name: "a".to_string(),
                    targets: vec![target(source_dir.path(), "*.log")],
                    after_days: 7,
                    bundle: BundleKind::Daily,
                    bundle_timezone: Some("UTC".to_string()),
                    ..ArchiveConfig::default()
                }),
                relocate_stage(source_dir.path(), 0, dest_dir.path().to_path_buf()),
            ],
        };

        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());
        let summary = run_job(&job, &clock, &NeverInUse, &lock, false)
            .unwrap()
            .unwrap();

        assert_eq!(summary.archived_count, 2);
        assert_eq!(
            summary.relocated_count, 1,
            "the bundle should be relocated as a single unit"
        );
        assert!(!file_a.exists());
        assert!(!file_b.exists());
    }

    #[test]
    fn run_all_processes_every_job_independently() {
        let source_dir = tempdir().unwrap();
        let job_a = JobConfig {
            name: "job-a".to_string(),
            stages: vec![archive_stage(source_dir.path(), 7)],
        };
        let job_b = JobConfig {
            name: "job-b".to_string(),
            stages: vec![archive_stage(source_dir.path(), 7)],
        };
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(source_dir.path());

        let results = run_all(&[job_a, job_b], &clock, &NeverInUse, &lock, false);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
    }

    #[test]
    fn delete_finds_relocated_files_independently_in_a_later_run() {
        // これが今回のFunctional Design改訂の主な動機: relocateとdeleteが別々の実行で
        // 完結する場合でも、deleteが自身のtargetsで退避先を独立にスキャンして発見できること
        let source_dir = tempdir().unwrap();
        let dest_dir = tempdir().unwrap();
        let file = source_dir.path().join("app.log");
        fs::write(&file, b"hello").unwrap();
        set_old_mtime(&file, Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());

        // ロックファイルの置き場所(設定ファイルのディレクトリ相当、BR-16)は監視対象とは
        // 別にする(実運用に忠実。deleteのtargetsがdest_dirを"**/*"で監視するため、同じ
        // ディレクトリにロックファイルを置くとロックファイル自体を候補に含めてしまう)
        let lock_dir = tempdir().unwrap();

        // 1回目の実行: archive + relocate のみ(deleteエントリなし)
        let relocate_job = JobConfig {
            name: "job".to_string(),
            stages: vec![
                archive_stage(source_dir.path(), 0),
                relocate_stage(source_dir.path(), 0, dest_dir.path().to_path_buf()),
            ],
        };
        let clock = FixedClock::new(Utc::now());
        let lock = FileJobLock::new(lock_dir.path());
        let summary = run_job(&relocate_job, &clock, &NeverInUse, &lock, false)
            .unwrap()
            .unwrap();
        assert_eq!(summary.relocated_count, 1);

        // 2回目の実行(別ジョブ・別ロック相当): deleteのみが退避先を独立にスキャンする
        let delete_job = JobConfig {
            name: "cleanup".to_string(),
            stages: vec![delete_stage(dest_dir.path(), 0)],
        };
        let lock2 = FileJobLock::new(lock_dir.path());
        let summary2 = run_job(&delete_job, &clock, &NeverInUse, &lock2, false)
            .unwrap()
            .unwrap();
        assert_eq!(
            summary2.deleted_count, 1,
            "delete should independently discover the relocated file"
        );
    }
}
