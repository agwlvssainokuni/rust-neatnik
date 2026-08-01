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

use crate::error::{LockError, NeatnikError};

/// ジョブ多重起動防止のアドバイザリファイルロック(FR-8, BR-16)。
///
/// `acquire`+`LockGuard`ではなく、クロージャにロック区間を委ねるスコープ方式を採用する
/// (fd-lockのガードがロックファイルの借用元に紐づくため、所有権を返す設計より単純かつ安全)。
/// ロック取得済みの場合は`Ok(None)`を返しジョブをスキップする(BR-16、エラーにはしない)
pub trait JobLock {
    fn with_lock<F, T>(&self, job_name: &str, f: F) -> Result<Option<T>, NeatnikError>
    where
        F: FnOnce() -> T;
}

/// `fd-lock`ベースの実装。ロックファイルは設定ファイルと同じディレクトリに`.<job-name>.lock`として作成する(FD-A3)
pub struct FileJobLock {
    directory: PathBuf,
}

impl FileJobLock {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn lock_path(&self, job_name: &str) -> PathBuf {
        self.directory.join(format!(".{job_name}.lock"))
    }
}

impl JobLock for FileJobLock {
    fn with_lock<F, T>(&self, job_name: &str, f: F) -> Result<Option<T>, NeatnikError>
    where
        F: FnOnce() -> T,
    {
        let path = self.lock_path(job_name);
        let file = open_lock_file(&path)?;
        let mut lock = fd_lock::RwLock::new(file);

        let result = match lock.try_write() {
            Ok(_guard) => Some(f()),
            Err(_) => {
                tracing::warn!(job = job_name, path = %path.display(), "job is already running, skipping (BR-16)");
                None
            }
        };
        Ok(result)
    }
}

fn open_lock_file(path: &Path) -> Result<fs::File, LockError> {
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|source| LockError::Acquire {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn with_lock_runs_the_closure_and_returns_its_value() {
        let dir = tempdir().unwrap();
        let lock = FileJobLock::new(dir.path());
        let result = lock.with_lock("job-a", || 42).unwrap();
        assert_eq!(result, Some(42));
    }

    #[test]
    fn with_lock_skips_when_another_holder_has_the_lock() {
        let dir = tempdir().unwrap();
        let lock = FileJobLock::new(dir.path());
        let path = lock.lock_path("job-a");

        let held_file = open_lock_file(&path).unwrap();
        let mut held_lock = fd_lock::RwLock::new(held_file);
        let _held_guard = held_lock.write().unwrap();

        let mut executed = false;
        let result = lock
            .with_lock("job-a", || {
                executed = true;
            })
            .unwrap();
        assert_eq!(result, None);
        assert!(!executed);
    }

    #[test]
    fn lock_is_released_after_with_lock_returns() {
        let dir = tempdir().unwrap();
        let lock = FileJobLock::new(dir.path());

        assert_eq!(lock.with_lock("job-a", || 1).unwrap(), Some(1));
        assert_eq!(lock.with_lock("job-a", || 2).unwrap(), Some(2));
    }

    #[test]
    fn different_jobs_use_independent_lock_files() {
        let dir = tempdir().unwrap();
        let lock = FileJobLock::new(dir.path());
        let path_a = lock.lock_path("job-a");
        let path_b = lock.lock_path("job-b");
        assert_ne!(path_a, path_b);
    }
}
