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

use std::path::PathBuf;

use thiserror::Error;

/// config モジュールのエラー(パース・バリデーション・パストラバーサル検出)
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_norway::Error,
    },
    #[error("invalid job \"{job}\": {reason}")]
    Invalid { job: String, reason: String },
    #[error("path \"{path}\" escapes basedir \"{basedir}\"")]
    PathTraversal { path: PathBuf, basedir: PathBuf },
}

/// scan モジュールのエラー(走査・基準日時決定)
#[derive(Debug, Error)]
pub enum ScanError {
    #[error("failed to read metadata for {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no filename date rule matched for {path}")]
    NoDateRuleMatched { path: PathBuf },
}

/// archive モジュールのエラー(圧縮・命名)
#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("failed to create archive {path}: {source}")]
    Create {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write entry {entry} into archive {archive}: {source}")]
    WriteEntry {
        archive: PathBuf,
        entry: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("insufficient disk space while archiving to {path}")]
    InsufficientSpace { path: PathBuf },
}

/// relocate モジュールのエラー(退避コピー)
#[derive(Debug, Error)]
pub enum RelocateError {
    #[error("failed to copy {from} to {to}: {source}")]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("destination already exists: {path}")]
    Conflict { path: PathBuf },
}

/// delete モジュールのエラー(削除・セーフティブレーキ)
#[derive(Debug, Error)]
pub enum DeleteError {
    #[error("failed to delete {path}: {source}")]
    Delete {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("safety brake triggered: {count} files exceed threshold {threshold}")]
    SafetyBrakeTriggered { count: usize, threshold: usize },
}

/// lock モジュールのエラー(多重起動防止)
#[derive(Debug, Error)]
pub enum LockError {
    #[error("failed to acquire job lock at {path}: {source}")]
    Acquire {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("job is already running (lock held at {path})")]
    AlreadyLocked { path: PathBuf },
}

/// 各モジュール共通のエラー型。`?`演算子でモジュール別エラーから変換できる。
#[derive(Debug, Error)]
pub enum NeatnikError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Scan(#[from] ScanError),
    #[error(transparent)]
    Archive(#[from] ArchiveError),
    #[error(transparent)]
    Relocate(#[from] RelocateError),
    #[error(transparent)]
    Delete(#[from] DeleteError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, NeatnikError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_error_converts_into_neatnik_error() {
        let err: NeatnikError = ConfigError::Invalid {
            job: "daily-logs".to_string(),
            reason: "N1 must be <= N2".to_string(),
        }
        .into();
        assert!(err.to_string().contains("daily-logs"));
    }

    #[test]
    fn path_traversal_error_message_includes_both_paths() {
        let err = ConfigError::PathTraversal {
            path: PathBuf::from("/data/../etc"),
            basedir: PathBuf::from("/data"),
        };
        let message = err.to_string();
        assert!(message.contains("/data/../etc"));
        assert!(message.contains("/data"));
    }
}
