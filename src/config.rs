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

//! 設定モデル(YAML)の定義・パース・バリデーション(FR-7)。

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

use crate::error::ConfigError;

/// 設定ファイルのルート(FR-7)。`jobs`のリストを持つ。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootConfig {
    pub jobs: Vec<JobConfig>,
}

impl RootConfig {
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&content, path)
    }

    fn parse(content: &str, path: &Path) -> Result<Self, ConfigError> {
        serde_norway::from_str(content).map_err(|source| {
            unknown_field_error(&source).unwrap_or(ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            })
        })
    }

    /// 全ジョブをバリデーションし、警告(BR-2)の一覧を返す。エラー(BR-1等)があれば最初の1件を返す。
    pub fn validate(&self) -> Result<Vec<ValidationWarning>, ConfigError> {
        let mut warnings = Vec::new();
        for job in &self.jobs {
            warnings.extend(validate_job(job)?);
        }
        Ok(warnings)
    }
}

/// ジョブ単位(監視対象×ルールの組)の設定。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobConfig {
    pub name: String,
    pub targets: Vec<WatchTarget>,
    #[serde(default)]
    pub archive: ArchiveConfig,
    #[serde(default)]
    pub relocate: RelocateConfig,
    #[serde(default)]
    pub delete: DeleteConfig,
}

/// 1つの監視対象ディレクトリとそのパターン(FR-1)。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchTarget {
    pub basedir: PathBuf,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub basis: BasisKind,
    #[serde(default)]
    pub filename_date_rules: Vec<FilenameDateRule>,
}

impl WatchTarget {
    /// `name`省略時、`basedir`から識別子を自動導出する(例: `/var/log/app` -> `var-log-app`)。バンドル命名の衝突回避に使う
    pub fn resolved_name(&self) -> String {
        match &self.name {
            Some(name) => name.clone(),
            None => derive_target_name(&self.basedir),
        }
    }

    /// `basedir`を正規化する(NFR-Design: パストラバーサル対策の起点)。
    pub fn canonical_basedir(&self, job_name: &str) -> Result<PathBuf, ConfigError> {
        self.basedir
            .canonicalize()
            .map_err(|source| ConfigError::Invalid {
                job: job_name.to_string(),
                reason: format!(
                    "basedir \"{}\" is not accessible: {source}",
                    self.basedir.display()
                ),
            })
    }
}

fn derive_target_name(basedir: &Path) -> String {
    let normalized = basedir.to_string_lossy().replace('\\', "/");
    let trimmed = normalized.trim_start_matches('/').trim_end_matches('/');
    if trimmed.is_empty() {
        "root".to_string()
    } else {
        trimmed.replace('/', "-")
    }
}

/// 正規化済み`basedir`の配下に正規化済み`candidate`が収まっているかを検証する(パストラバーサル対策)。
pub fn is_within_basedir(basedir: &Path, candidate: &Path) -> bool {
    candidate.starts_with(basedir)
}

/// `basis: FilenameDate`における日付抽出ルール1件(BR-7.1)。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilenameDateRule {
    pub regex: String,
    pub format: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasisKind {
    #[default]
    Mtime,
    Ctime,
    FilenameDate,
}

/// アーカイブ段階の設定(FR-2)。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ArchiveConfig {
    pub enabled: bool,
    pub after_days: u32,
    pub format: ArchiveFormat,
    pub bundle: BundleKind,
    /// バンドル期間境界の計算に使うタイムゾーン(BR-10)。省略時はローカルタイムゾーンを使う
    pub bundle_timezone: Option<String>,
    pub keep_original: bool,
    pub on_stale_bundle_member: OnStaleBundleMember,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    #[default]
    Gzip,
    Zip,
    #[serde(rename = "tar.gz")]
    TarGz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleKind {
    #[default]
    None,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnStaleBundleMember {
    #[default]
    Warn,
    Error,
}

/// 退避段階の設定(FR-3)。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RelocateConfig {
    pub enabled: bool,
    pub after_days: u32,
    pub destination: Option<PathBuf>,
    pub layout: LayoutKind,
    pub on_conflict: ConflictPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutKind {
    #[default]
    Preserve,
    YearMonth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    #[default]
    Rename,
    Skip,
    Error,
}

/// 削除段階の設定(FR-4)。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DeleteConfig {
    pub enabled: bool,
    pub after_days: u32,
    pub safety_brake: SafetyBrakeConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SafetyBrakeConfig {
    pub enforce: bool,
    pub count_threshold: Option<u64>,
    pub size_threshold_gb: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationWarning(pub String);

/// ジョブ1件をバリデーションする(BR-1, BR-2, RelocateConfig.destination必須チェック、basedir到達可能性)。
pub fn validate_job(job: &JobConfig) -> Result<Vec<ValidationWarning>, ConfigError> {
    let mut warnings = Vec::new();

    if !job.archive.enabled && !job.relocate.enabled && !job.delete.enabled {
        warnings.push(ValidationWarning(format!(
            "job \"{}\": all stages (archive/relocate/delete) are disabled",
            job.name
        )));
    }

    check_threshold_order(
        &job.name,
        &[
            (
                "archive.after_days",
                job.archive.enabled,
                job.archive.after_days,
            ),
            (
                "relocate.after_days",
                job.relocate.enabled,
                job.relocate.after_days,
            ),
            (
                "delete.after_days",
                job.delete.enabled,
                job.delete.after_days,
            ),
        ],
    )?;

    if job.relocate.enabled && job.relocate.destination.is_none() {
        return Err(ConfigError::Invalid {
            job: job.name.clone(),
            reason: "relocate.destination is required when relocate.enabled is true".to_string(),
        });
    }

    for target in &job.targets {
        target.canonical_basedir(&job.name)?;
    }

    Ok(warnings)
}

/// BR-1: 有効なステージ同士でのみ`N1 <= N2 <= N3`を検証する(等号は許容)。無効なステージの値は比較対象から除外する
fn check_threshold_order(job_name: &str, stages: &[(&str, bool, u32)]) -> Result<(), ConfigError> {
    let enabled: Vec<(&str, u32)> = stages
        .iter()
        .filter(|(_, enabled, _)| *enabled)
        .map(|(name, _, days)| (*name, *days))
        .collect();
    for pair in enabled.windows(2) {
        let (prev_name, prev_days) = pair[0];
        let (next_name, next_days) = pair[1];
        if prev_days > next_days {
            return Err(ConfigError::Invalid {
                job: job_name.to_string(),
                reason: format!("{prev_name} ({prev_days}) must be <= {next_name} ({next_days})"),
            });
        }
    }
    Ok(())
}

/// BR-4: 未知フィールドをレーベンシュタイン距離で既知候補と比較し、類似フィールドを提示する
fn unknown_field_error(source: &serde_norway::Error) -> Option<ConfigError> {
    let message = source.to_string();
    let field_re = Regex::new(r"unknown field `([^`]+)`, expected (?:one of )?(.+)").ok()?;
    let captures = field_re.captures(&message)?;
    let field = captures.get(1)?.as_str().to_string();
    let rest = captures.get(2)?.as_str();
    let candidate_re = Regex::new(r"`([^`]+)`").ok()?;
    let candidates: Vec<String> = candidate_re
        .captures_iter(rest)
        .map(|c| c[1].to_string())
        .collect();
    let suggestion = candidates
        .iter()
        .map(|candidate| (candidate, levenshtein(&field, candidate)))
        .min_by_key(|(_, distance)| *distance)
        .filter(|(_, distance)| *distance <= 3)
        .map(|(candidate, _)| candidate.clone())
        .unwrap_or_else(|| {
            "one of the documented fields (see `neatnik init` for a sample)".to_string()
        });
    Some(ConfigError::UnknownField { field, suggestion })
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn sample_yaml() -> &'static str {
        r#"
jobs:
  - name: app-server-logs
    targets:
      - basedir: "/var/log/app"
        include: ["*.log"]
        exclude: ["current.log"]
        basis: mtime
    archive:
      enabled: true
      after_days: 7
      format: gzip
      bundle: daily
      keep_original: false
    relocate:
      enabled: true
      after_days: 30
      destination: "/mnt/storage/app-logs"
      layout: year_month
      on_conflict: rename
    delete:
      enabled: true
      after_days: 365
"#
    }

    #[test]
    fn parses_a_full_job_config() {
        let config: RootConfig = serde_norway::from_str(sample_yaml()).unwrap();
        assert_eq!(config.jobs.len(), 1);
        let job = &config.jobs[0];
        assert_eq!(job.name, "app-server-logs");
        assert_eq!(job.archive.format, ArchiveFormat::Gzip);
        assert_eq!(job.archive.bundle, BundleKind::Daily);
        assert_eq!(job.relocate.layout, LayoutKind::YearMonth);
        assert_eq!(job.delete.after_days, 365);
    }

    #[test]
    fn resolved_name_is_derived_from_basedir_when_omitted() {
        let target = WatchTarget {
            basedir: PathBuf::from("/var/log/app"),
            name: None,
            include: vec![],
            exclude: vec![],
            basis: BasisKind::Mtime,
            filename_date_rules: vec![],
        };
        assert_eq!(target.resolved_name(), "var-log-app");
    }

    #[test]
    fn resolved_name_uses_explicit_name_when_present() {
        let target = WatchTarget {
            basedir: PathBuf::from("/var/log/app"),
            name: Some("custom".to_string()),
            include: vec![],
            exclude: vec![],
            basis: BasisKind::Mtime,
            filename_date_rules: vec![],
        };
        assert_eq!(target.resolved_name(), "custom");
    }

    #[test]
    fn all_stages_disabled_produces_a_warning_not_an_error() {
        let job = JobConfig {
            name: "idle-job".to_string(),
            targets: vec![],
            archive: ArchiveConfig::default(),
            relocate: RelocateConfig::default(),
            delete: DeleteConfig::default(),
        };
        let warnings = validate_job(&job).unwrap();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn relocate_enabled_without_destination_is_rejected() {
        let job = JobConfig {
            name: "broken-job".to_string(),
            targets: vec![],
            archive: ArchiveConfig::default(),
            relocate: RelocateConfig {
                enabled: true,
                ..RelocateConfig::default()
            },
            delete: DeleteConfig::default(),
        };
        assert!(validate_job(&job).is_err());
    }

    #[test]
    fn is_within_basedir_detects_traversal() {
        let basedir = Path::new("/data/app");
        assert!(is_within_basedir(
            basedir,
            Path::new("/data/app/logs/a.log")
        ));
        assert!(!is_within_basedir(basedir, Path::new("/data/other/a.log")));
        assert!(!is_within_basedir(basedir, Path::new("/etc/passwd")));
    }

    #[test]
    fn unknown_field_suggests_the_closest_known_field() {
        let source =
            serde_norway::from_str::<JobConfig>("name: x\ntargets: []\narchve:\n  enabled: true\n")
                .unwrap_err();
        let err = unknown_field_error(&source);
        match err {
            Some(ConfigError::UnknownField { field, suggestion }) => {
                assert_eq!(field, "archve");
                assert_eq!(suggestion, "archive");
            }
            other => panic!("expected UnknownField, got {other:?}"),
        }
    }

    proptest! {
        #[test]
        fn disabled_stage_threshold_is_excluded_from_br1_ordering_check(
            archive_days in 0u32..100,
            relocate_days in 0u32..100_000,
            delete_days in 0u32..100,
        ) {
            prop_assume!(archive_days <= delete_days);
            let result = check_threshold_order(
                "t",
                &[
                    ("archive.after_days", true, archive_days),
                    ("relocate.after_days", false, relocate_days),
                    ("delete.after_days", true, delete_days),
                ],
            );
            prop_assert!(result.is_ok());
        }

        #[test]
        fn enabled_stages_out_of_order_are_rejected(
            a in 1u32..100,
            b in 0u32..100,
        ) {
            prop_assume!(a > b);
            let result = check_threshold_order(
                "t",
                &[("archive.after_days", true, a), ("relocate.after_days", true, b)],
            );
            prop_assert!(result.is_err());
        }

        #[test]
        fn enabled_stages_in_order_are_accepted(
            a in 0u32..100,
            b in 0u32..100,
        ) {
            prop_assume!(a <= b);
            let result = check_threshold_order(
                "t",
                &[("archive.after_days", true, a), ("relocate.after_days", true, b)],
            );
            prop_assert!(result.is_ok());
        }
    }
}
