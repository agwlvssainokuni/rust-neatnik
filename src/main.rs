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

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use neatnik::clock::{Clock, FixedClock, SystemClock};
use neatnik::config::{JobConfig, RootConfig, ValidationWarning};
use neatnik::lock::FileJobLock;
use neatnik::pipeline::{self, JobSummary};
use neatnik::scan::platform_write_guard_detector;

const DEFAULT_CONFIG_PATH: &str = "config.yaml";
/// `neatnik init`が出力するサンプル設定(FR-11)。Step 13でリポジトリ同梱の
/// `config.example.yaml`と共通化する予定の暫定版
const SAMPLE_CONFIG: &str = include_str!("../config.example.yaml");

#[derive(Parser)]
#[command(
    name = "neatnik",
    version,
    about = "Automated housekeeping (archive, relocate, delete) for log, working and temporary files."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// ジョブを実行する
    Run(RunArgs),
    /// 設定ファイルを検証する
    Validate(ConfigOpts),
    /// サンプル設定ファイルを生成する
    Init(InitArgs),
    /// 設定済みジョブの一覧を表示する
    List(ConfigOpts),
    /// シェル補完スクリプトを生成する
    Completions(CompletionsArgs),
}

#[derive(Args, Clone)]
struct ConfigOpts {
    /// 設定ファイルのパス(省略時は./config.yaml)
    #[arg(long, short)]
    config: Option<PathBuf>,
    /// 「現在時刻」を指定した日時に固定する(RFC3339形式、例: 2027-01-01T00:00:00Z)
    #[arg(long)]
    now: Option<String>,
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    config_opts: ConfigOpts,
    /// 実行対象のジョブ名(省略時は全ジョブ)
    #[arg(long)]
    job: Option<String>,
    /// 実際のファイル操作を行わず、対象件数・合計サイズのみ表示する
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct InitArgs {
    /// 生成する設定ファイルのパス
    #[arg(long, short, default_value = DEFAULT_CONFIG_PATH)]
    output: PathBuf,
    /// 既存ファイルを上書きする
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct CompletionsArgs {
    shell: Shell,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let Some(command) = cli.command else {
        print_welcome();
        return ExitCode::SUCCESS;
    };

    match run_command(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // ユーザー向けには一般化されたメッセージ、詳細はtracing経由で記録する(SECURITY-15)
            eprintln!("error: {err}");
            tracing::error!(error = %err, "command failed");
            ExitCode::FAILURE
        }
    }
}

fn run_command(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Run(args) => cmd_run(args),
        Command::Validate(opts) => cmd_validate(opts),
        Command::Init(args) => cmd_init(args),
        Command::List(opts) => cmd_list(opts),
        Command::Completions(args) => cmd_completions(args),
    }
}

fn resolve_config_path(explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

/// BR-5: 設定ファイル不在時は`neatnik init`の実行を提案する
fn load_config(explicit: Option<PathBuf>) -> anyhow::Result<RootConfig> {
    let path = resolve_config_path(explicit);
    if !path.exists() {
        anyhow::bail!(
            "config file \"{}\" was not found. Run `neatnik init --output {}` to generate a sample configuration file.",
            path.display(),
            path.display()
        );
    }
    Ok(RootConfig::load_from_path(&path)?)
}

/// FR-13: `--now`(RFC3339)が指定されていればそれを固定時刻として使う。省略時はシステム時刻を使う
fn resolve_clock(now: Option<&str>) -> anyhow::Result<Box<dyn Clock>> {
    match now {
        None => Ok(Box::new(SystemClock)),
        Some(value) => {
            let parsed = chrono::DateTime::parse_from_rfc3339(value).map_err(|err| {
                anyhow::anyhow!("invalid --now value \"{value}\": {err} (expected RFC3339, e.g. 2027-01-01T00:00:00Z)")
            })?;
            Ok(Box::new(FixedClock::new(parsed.with_timezone(&chrono::Utc))))
        }
    }
}

fn filter_jobs(jobs: &[JobConfig], job_name: Option<&str>) -> anyhow::Result<Vec<JobConfig>> {
    match job_name {
        None => Ok(jobs.to_vec()),
        Some(name) => {
            let filtered: Vec<JobConfig> = jobs.iter().filter(|job| job.name == name).cloned().collect();
            if filtered.is_empty() {
                anyhow::bail!("job \"{name}\" was not found in the configuration");
            }
            Ok(filtered)
        }
    }
}

fn cmd_run(args: RunArgs) -> anyhow::Result<()> {
    let config = load_config(args.config_opts.config.clone())?;
    print_warnings(&config.validate()?);

    let clock = resolve_clock(args.config_opts.now.as_deref())?;
    let detector = platform_write_guard_detector();
    let lock_dir = resolve_config_path(args.config_opts.config)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let lock = FileJobLock::new(lock_dir);

    let jobs = filter_jobs(&config.jobs, args.job.as_deref())?;
    let results = pipeline::run_all(&jobs, clock.as_ref(), &detector, &lock, args.dry_run);

    let mut had_failure = false;
    for (job_name, result) in results {
        match result {
            Ok(Some(summary)) => print_summary(&summary),
            Ok(None) => println!("job \"{job_name}\" skipped: already running (lock held, BR-16)"),
            Err(err) => {
                had_failure = true;
                eprintln!("job \"{job_name}\" failed: {err}");
                tracing::error!(job = %job_name, error = %err, "job failed");
            }
        }
    }

    if had_failure {
        anyhow::bail!("one or more jobs failed");
    }
    Ok(())
}

fn cmd_validate(opts: ConfigOpts) -> anyhow::Result<()> {
    // --nowはrun/list/validateで共通のオプションとして受け付ける(FR-13)が、validateは
    // 日時に基づくファイル走査を行わないため、ここでは形式検証にのみ使う
    let _clock = resolve_clock(opts.now.as_deref())?;
    let config = load_config(opts.config)?;
    print_warnings(&config.validate()?);
    println!("configuration is valid ({} job(s))", config.jobs.len());
    Ok(())
}

fn cmd_init(args: InitArgs) -> anyhow::Result<()> {
    if args.output.exists() && !args.force {
        anyhow::bail!("\"{}\" already exists. Use --force to overwrite it.", args.output.display());
    }
    std::fs::write(&args.output, SAMPLE_CONFIG)?;
    println!("wrote a sample configuration to \"{}\"", args.output.display());
    Ok(())
}

fn cmd_list(opts: ConfigOpts) -> anyhow::Result<()> {
    let _clock = resolve_clock(opts.now.as_deref())?;
    let config = load_config(opts.config)?;
    if config.jobs.is_empty() {
        println!("no jobs are configured");
        return Ok(());
    }
    for job in &config.jobs {
        println!(
            "{} (archive: {}, relocate: {}, delete: {})",
            job.name, job.archive.enabled, job.relocate.enabled, job.delete.enabled
        );
    }
    Ok(())
}

fn cmd_completions(args: CompletionsArgs) -> anyhow::Result<()> {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    generate(args.shell, &mut command, name, &mut std::io::stdout());
    Ok(())
}

fn print_warnings(warnings: &[ValidationWarning]) {
    for warning in warnings {
        eprintln!("warning: {}", warning.0);
    }
}

fn print_summary(summary: &JobSummary) {
    println!(
        "job \"{}\": archived {} ({} bytes), relocated {} ({} bytes), deleted {} ({} bytes)",
        summary.job_name,
        summary.archived_count,
        summary.archived_bytes,
        summary.relocated_count,
        summary.relocated_bytes,
        summary.deleted_count,
        summary.deleted_bytes,
    );
    if summary.safety_brake_triggered {
        println!("  safety brake triggered for job \"{}\" (BR-13)", summary.job_name);
    }
    for outcome in &summary.failed {
        eprintln!(
            "  failed: {} ({:?}): {}",
            outcome.path.display(),
            outcome.stage,
            outcome.reason.as_deref().unwrap_or("unknown reason")
        );
    }
}

/// 引数なし実行時のウェルカムガイド(FR-11)
fn print_welcome() {
    println!("neatnik - automated housekeeping for log, working and temporary files\n");
    println!("Getting started:");
    println!("  1. neatnik init             Generate a sample configuration file (config.yaml)");
    println!("  2. neatnik validate         Check the configuration for errors");
    println!("  3. neatnik run --dry-run    Preview what would happen without touching any files");
    println!("  4. neatnik run              Execute the configured jobs");
    println!();
    println!("Run `neatnik --help` for the full list of commands and options.");
}
