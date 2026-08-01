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

//! 削除処理・セーフティブレーキ(FR-4、BR-13、BR-14)。

use std::fs;
use std::path::PathBuf;

use crate::config::SafetyBrakeConfig;
use crate::scan::FileCandidate;

const BYTES_PER_GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// BR-13の閾値評価結果。
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyBrakeEvaluation {
    pub triggered: bool,
    pub count: usize,
    pub total_bytes: u64,
}

/// BR-13: 削除対象の件数・合計容量を閾値と比較する(純粋関数)。
pub fn evaluate_safety_brake(
    candidates: &[FileCandidate],
    config: &SafetyBrakeConfig,
) -> SafetyBrakeEvaluation {
    let count = candidates.len();
    let total_bytes: u64 = candidates
        .iter()
        .map(|candidate| candidate.size_bytes)
        .sum();
    let count_exceeded = config
        .count_threshold
        .is_some_and(|threshold| count as u64 > threshold);
    let size_exceeded = config
        .size_threshold_gb
        .is_some_and(|threshold| (total_bytes as f64) / BYTES_PER_GB > threshold);
    SafetyBrakeEvaluation {
        triggered: count_exceeded || size_exceeded,
        count,
        total_bytes,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteOutcome {
    Deleted,
    /// `--dry-run`により実削除を行わなかった(BR-14)
    DryRun,
    /// セーフティブレーキ(`enforce: true`)発動により削除をブロックした(BR-13)
    Blocked,
    Failed,
}

#[derive(Debug, Clone)]
pub struct DeleteResult {
    pub path: PathBuf,
    pub outcome: DeleteOutcome,
}

#[derive(Debug, Clone)]
pub struct DeleteRunReport {
    pub evaluation: SafetyBrakeEvaluation,
    pub results: Vec<DeleteResult>,
}

/// BR-13/BR-14: セーフティブレーキを評価し、ブロックされていなければ削除を実行する。
///
/// **既知の制約**: BR-13後半の「`enforce: true`発動後、人手でロック解除するまで次回実行も
/// 自動的に止め続ける」永続的な状態保持は、具体的なロックファイル形式・解除コマンドが
/// Functional/NFR Designで未確定のため本ステップでは未実装。現状は実行のたびに閾値を再評価する
pub fn run(
    candidates: &[FileCandidate],
    config: &SafetyBrakeConfig,
    dry_run: bool,
) -> DeleteRunReport {
    let evaluation = evaluate_safety_brake(candidates, config);
    let blocked = evaluation.triggered && config.enforce;

    if blocked {
        tracing::warn!(
            count = evaluation.count,
            total_bytes = evaluation.total_bytes,
            "safety brake triggered, blocking delete stage for this run (BR-13)"
        );
    } else if evaluation.triggered {
        tracing::warn!(
            count = evaluation.count,
            total_bytes = evaluation.total_bytes,
            "safety brake threshold exceeded but enforce is false, continuing (BR-13)"
        );
    }

    let results = candidates
        .iter()
        .map(|candidate| {
            let outcome = if blocked {
                DeleteOutcome::Blocked
            } else if dry_run {
                DeleteOutcome::DryRun
            } else {
                match fs::remove_file(&candidate.path) {
                    Ok(()) => DeleteOutcome::Deleted,
                    Err(source) => {
                        tracing::warn!(path = %candidate.path.display(), error = %source, "failed to delete file");
                        DeleteOutcome::Failed
                    }
                }
            };
            DeleteResult {
                path: candidate.path.clone(),
                outcome,
            }
        })
        .collect();

    DeleteRunReport {
        evaluation,
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use proptest::prelude::*;

    use crate::scan::WatchTargetRef;

    fn candidate_with_size(size_bytes: u64) -> FileCandidate {
        FileCandidate {
            path: PathBuf::from(format!("/tmp/{size_bytes}.log")),
            target: WatchTargetRef {
                basedir: PathBuf::from("/tmp"),
                name: "t".to_string(),
            },
            basis_datetime: Utc::now(),
            size_bytes,
            in_use: false,
        }
    }

    #[test]
    fn triggers_on_count_threshold() {
        let candidates = vec![
            candidate_with_size(1),
            candidate_with_size(1),
            candidate_with_size(1),
        ];
        let config = SafetyBrakeConfig {
            enforce: true,
            count_threshold: Some(2),
            size_threshold_gb: None,
        };
        let evaluation = evaluate_safety_brake(&candidates, &config);
        assert!(evaluation.triggered);
        assert_eq!(evaluation.count, 3);
    }

    #[test]
    fn triggers_on_size_threshold() {
        let candidates = vec![candidate_with_size(2 * 1024 * 1024 * 1024)];
        let config = SafetyBrakeConfig {
            enforce: true,
            count_threshold: None,
            size_threshold_gb: Some(1.0),
        };
        assert!(evaluate_safety_brake(&candidates, &config).triggered);
    }

    #[test]
    fn does_not_trigger_when_under_thresholds() {
        let candidates = vec![candidate_with_size(1)];
        let config = SafetyBrakeConfig {
            enforce: true,
            count_threshold: Some(10),
            size_threshold_gb: Some(1.0),
        };
        assert!(!evaluate_safety_brake(&candidates, &config).triggered);
    }

    #[test]
    fn run_blocks_deletion_when_enforce_is_true_and_triggered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.log");
        fs::write(&path, b"x").unwrap();
        let mut candidate = candidate_with_size(1);
        candidate.path = path.clone();

        let config = SafetyBrakeConfig {
            enforce: true,
            count_threshold: Some(0),
            size_threshold_gb: None,
        };
        let report = run(&[candidate], &config, false);
        assert_eq!(report.results[0].outcome, DeleteOutcome::Blocked);
        assert!(path.exists());
    }

    #[test]
    fn run_deletes_when_enforce_is_false_even_if_triggered() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.log");
        fs::write(&path, b"x").unwrap();
        let mut candidate = candidate_with_size(1);
        candidate.path = path.clone();

        let config = SafetyBrakeConfig {
            enforce: false,
            count_threshold: Some(0),
            size_threshold_gb: None,
        };
        let report = run(&[candidate], &config, false);
        assert_eq!(report.results[0].outcome, DeleteOutcome::Deleted);
        assert!(!path.exists());
    }

    #[test]
    fn run_does_not_delete_in_dry_run_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.log");
        fs::write(&path, b"x").unwrap();
        let mut candidate = candidate_with_size(1);
        candidate.path = path.clone();

        let config = SafetyBrakeConfig::default();
        let report = run(&[candidate], &config, true);
        assert_eq!(report.results[0].outcome, DeleteOutcome::DryRun);
        assert!(path.exists());
    }

    proptest! {
        #[test]
        fn count_never_exceeds_threshold_unless_triggered(
            count in 0usize..50,
            threshold in 0u64..50,
        ) {
            let candidates: Vec<FileCandidate> = (0..count).map(|_| candidate_with_size(1)).collect();
            let config = SafetyBrakeConfig {
                enforce: true,
                count_threshold: Some(threshold),
                size_threshold_gb: None,
            };
            let evaluation = evaluate_safety_brake(&candidates, &config);
            if !evaluation.triggered {
                prop_assert!((evaluation.count as u64) <= threshold);
            }
        }
    }
}
