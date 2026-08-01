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
use std::path::Path;
use std::time::SystemTime;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn neatnik() -> Command {
    Command::cargo_bin("neatnik").unwrap()
}

fn set_old_mtime(path: &Path) {
    let old = SystemTime::now() - std::time::Duration::from_secs(3600 * 24 * 3650);
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(old)).unwrap();
}

fn write_job_config(path: &Path, basedir: &Path, destination: &Path) {
    let content = format!(
        r#"
jobs:
  - name: test-job
    targets:
      - basedir: "{}"
        include: ["*.log"]
    archive:
      enabled: true
      after_days: 0
      format: gzip
      bundle: none
    relocate:
      enabled: true
      after_days: 0
      destination: "{}"
      layout: preserve
      on_conflict: rename
    delete:
      enabled: true
      after_days: 0
"#,
        basedir.display(),
        destination.display()
    );
    fs::write(path, content).unwrap();
}

#[test]
fn no_args_shows_the_welcome_guide() {
    neatnik()
        .assert()
        .success()
        .stdout(predicate::str::contains("Getting started"))
        .stdout(predicate::str::contains("neatnik init"));
}

#[test]
fn init_creates_a_sample_config_file() {
    let dir = tempdir().unwrap();
    let output = dir.path().join("config.yaml");

    neatnik()
        .args(["init", "--output"])
        .arg(&output)
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote a sample configuration"));

    assert!(output.exists());
    let content = fs::read_to_string(&output).unwrap();
    assert!(content.contains("jobs:"));
}

#[test]
fn init_refuses_to_overwrite_without_force() {
    let dir = tempdir().unwrap();
    let output = dir.path().join("config.yaml");
    fs::write(&output, "existing content").unwrap();

    neatnik()
        .args(["init", "--output"])
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--force"));

    assert_eq!(fs::read_to_string(&output).unwrap(), "existing content");
}

#[test]
fn init_overwrites_when_forced() {
    let dir = tempdir().unwrap();
    let output = dir.path().join("config.yaml");
    fs::write(&output, "existing content").unwrap();

    neatnik()
        .args(["init", "--output"])
        .arg(&output)
        .arg("--force")
        .assert()
        .success();

    let content = fs::read_to_string(&output).unwrap();
    assert!(content.contains("jobs:"));
}

#[test]
fn validate_reports_a_helpful_message_when_config_is_missing() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("no-such-config.yaml");

    neatnik()
        .args(["validate", "--config"])
        .arg(&missing)
        .assert()
        .failure()
        .stderr(predicate::str::contains("neatnik init"));
}

#[test]
fn validate_succeeds_for_a_well_formed_config() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["validate", "--config"])
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("configuration is valid"));
}

#[test]
fn validate_rejects_an_unknown_now_format() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["validate", "--config"])
        .arg(&config_path)
        .args(["--now", "not-a-date"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--now"));
}

#[test]
fn list_shows_configured_jobs() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["list", "--config"])
        .arg(&config_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("test-job"));
}

#[test]
fn run_dry_run_reports_counts_without_touching_files() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let log_file = source_dir.path().join("app.log");
    fs::write(&log_file, b"hello").unwrap();
    set_old_mtime(&log_file);

    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["run", "--config"])
        .arg(&config_path)
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("archived 1"))
        .stdout(predicate::str::contains("relocated 1"))
        .stdout(predicate::str::contains("deleted 1"));

    assert!(
        log_file.exists(),
        "dry-run must not touch the original file"
    );
}

#[test]
fn run_executes_the_full_pipeline_in_one_pass() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let log_file = source_dir.path().join("app.log");
    fs::write(&log_file, b"hello").unwrap();
    set_old_mtime(&log_file);

    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["run", "--config"])
        .arg(&config_path)
        .assert()
        .success();

    assert!(
        !log_file.exists(),
        "the file should have been archived, relocated and deleted"
    );
}

#[test]
fn run_accepts_a_now_override_in_rfc3339_format() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["run", "--config"])
        .arg(&config_path)
        .args(["--now", "2027-01-01T00:00:00Z"])
        .arg("--dry-run")
        .assert()
        .success();
}

#[test]
fn run_rejects_an_invalid_now_value() {
    let source_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let config_path = source_dir.path().join("config.yaml");
    write_job_config(&config_path, source_dir.path(), dest_dir.path());

    neatnik()
        .args(["run", "--config"])
        .arg(&config_path)
        .args(["--now", "not-a-date"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--now"));
}

#[test]
fn completions_generates_a_non_empty_script() {
    neatnik()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("neatnik"));
}

#[test]
fn version_flag_prints_the_crate_version() {
    neatnik()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
