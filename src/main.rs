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

mod i18n;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Shell};

use i18n::{msg, Locale};
use neatnik::clock::{Clock, FixedClock, SystemClock};
use neatnik::config::{JobConfig, RootConfig, StageConfig, ValidationWarning};
use neatnik::lock::FileJobLock;
use neatnik::pipeline::{self, JobSummary};
use neatnik::scan::platform_write_guard_detector;

const DEFAULT_CONFIG_PATH: &str = "config.yaml";

#[derive(Parser)]
#[command(name = "neatnik", version)]
struct Cli {
    /// Output language (en/ja)
    #[arg(long, global = true, value_enum)]
    lang: Option<Locale>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Run(RunArgs),
    Validate(ConfigOpts),
    Init(InitArgs),
    List(ConfigOpts),
    Completions(CompletionsArgs),
}

#[derive(Args, Clone)]
struct ConfigOpts {
    #[arg(long, short)]
    config: Option<PathBuf>,
    #[arg(long)]
    now: Option<String>,
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    config_opts: ConfigOpts,
    #[arg(long)]
    job: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct InitArgs {
    #[arg(long, short, default_value = DEFAULT_CONFIG_PATH)]
    output: PathBuf,
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct CompletionsArgs {
    shell: Shell,
}

fn resolve_locale() -> Locale {
    Locale::from_args(std::env::args().skip(1)).unwrap_or_else(Locale::detect)
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let locale = resolve_locale();
    let command = i18n::localize_command(Cli::command(), locale);
    let matches = command.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    let locale = cli.lang.unwrap_or(locale);

    let Some(command) = cli.command else {
        println!("{}", msg::welcome(locale));
        return ExitCode::SUCCESS;
    };

    match run_command(command, locale) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // ユーザー向けには一般化されたメッセージ、詳細はtracing経由で記録する(SECURITY-15)
            eprintln!("error: {err}");
            tracing::error!(error = %err, "command failed");
            ExitCode::FAILURE
        }
    }
}

fn run_command(command: Command, locale: Locale) -> anyhow::Result<()> {
    match command {
        Command::Run(args) => cmd_run(args, locale),
        Command::Validate(opts) => cmd_validate(opts, locale),
        Command::Init(args) => cmd_init(args, locale),
        Command::List(opts) => cmd_list(opts, locale),
        Command::Completions(args) => cmd_completions(args, locale),
    }
}

fn resolve_config_path(explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH))
}

/// BR-5: 設定ファイル不在時は`neatnik init`の実行を提案する
fn load_config(explicit: Option<PathBuf>, locale: Locale) -> anyhow::Result<RootConfig> {
    let path = resolve_config_path(explicit);
    if !path.exists() {
        anyhow::bail!(msg::config_not_found(locale, &path));
    }
    Ok(RootConfig::load_from_path(&path)?)
}

/// FR-13: `--now`(RFC3339)が指定されていればそれを固定時刻として使う。省略時はシステム時刻を使う
fn resolve_clock(now: Option<&str>, locale: Locale) -> anyhow::Result<Box<dyn Clock>> {
    match now {
        None => Ok(Box::new(SystemClock)),
        Some(value) => {
            let parsed = chrono::DateTime::parse_from_rfc3339(value).map_err(|err| {
                anyhow::anyhow!(msg::invalid_now(locale, value, &err.to_string()))
            })?;
            Ok(Box::new(FixedClock::new(
                parsed.with_timezone(&chrono::Utc),
            )))
        }
    }
}

fn filter_jobs(
    jobs: &[JobConfig],
    job_name: Option<&str>,
    locale: Locale,
) -> anyhow::Result<Vec<JobConfig>> {
    match job_name {
        None => Ok(jobs.to_vec()),
        Some(name) => {
            let filtered: Vec<JobConfig> = jobs
                .iter()
                .filter(|job| job.name == name)
                .cloned()
                .collect();
            if filtered.is_empty() {
                anyhow::bail!(msg::job_not_found(locale, name));
            }
            Ok(filtered)
        }
    }
}

fn cmd_run(args: RunArgs, locale: Locale) -> anyhow::Result<()> {
    let config = load_config(args.config_opts.config.clone(), locale)?;
    print_warnings(&config.validate()?, locale);

    let clock = resolve_clock(args.config_opts.now.as_deref(), locale)?;
    let detector = platform_write_guard_detector();
    let lock_dir = resolve_config_path(args.config_opts.config)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let lock = FileJobLock::new(lock_dir);

    let jobs = filter_jobs(&config.jobs, args.job.as_deref(), locale)?;
    let results = pipeline::run_all(&jobs, clock.as_ref(), &detector, &lock, args.dry_run);

    let mut had_failure = false;
    for (job_name, result) in results {
        match result {
            Ok(Some(summary)) => print_summary(&summary, locale),
            Ok(None) => println!("{}", msg::job_skipped_locked(locale, &job_name)),
            Err(err) => {
                had_failure = true;
                eprintln!("{}", msg::job_failed(locale, &job_name, &err.to_string()));
                tracing::error!(job = %job_name, error = %err, "job failed");
            }
        }
    }

    if had_failure {
        anyhow::bail!(msg::one_or_more_jobs_failed(locale));
    }
    Ok(())
}

fn cmd_validate(opts: ConfigOpts, locale: Locale) -> anyhow::Result<()> {
    // --nowはrun/list/validateで共通のオプションとして受け付ける(FR-13)が、validateは
    // 日時に基づくファイル走査を行わないため、ここでは形式検証にのみ使う
    let _clock = resolve_clock(opts.now.as_deref(), locale)?;
    let config = load_config(opts.config, locale)?;
    print_warnings(&config.validate()?, locale);
    println!("{}", msg::configuration_is_valid(locale, config.jobs.len()));
    Ok(())
}

fn cmd_init(args: InitArgs, locale: Locale) -> anyhow::Result<()> {
    if args.output.exists() && !args.force {
        anyhow::bail!(msg::already_exists(locale, &args.output));
    }
    std::fs::write(&args.output, i18n::sample_config(locale))?;
    println!("{}", msg::wrote_sample_config(locale, &args.output));
    Ok(())
}

fn cmd_list(opts: ConfigOpts, locale: Locale) -> anyhow::Result<()> {
    let _clock = resolve_clock(opts.now.as_deref(), locale)?;
    let config = load_config(opts.config, locale)?;
    if config.jobs.is_empty() {
        println!("{}", msg::no_jobs_configured(locale));
        return Ok(());
    }
    for job in &config.jobs {
        let archive_count = job
            .stages
            .iter()
            .filter(|s| matches!(s, StageConfig::Archive(_)))
            .count();
        let relocate_count = job
            .stages
            .iter()
            .filter(|s| matches!(s, StageConfig::Relocate(_)))
            .count();
        let delete_count = job
            .stages
            .iter()
            .filter(|s| matches!(s, StageConfig::Delete(_)))
            .count();
        println!(
            "{} (archive: {archive_count}, relocate: {relocate_count}, delete: {delete_count})",
            job.name
        );
    }
    Ok(())
}

fn cmd_completions(args: CompletionsArgs, locale: Locale) -> anyhow::Result<()> {
    let mut command = i18n::localize_command(Cli::command(), locale);
    let name = command.get_name().to_string();
    generate(args.shell, &mut command, name, &mut std::io::stdout());
    Ok(())
}

fn print_warnings(warnings: &[ValidationWarning], locale: Locale) {
    for warning in warnings {
        eprintln!("{}: {}", msg::warning_prefix(locale), warning.0);
    }
}

fn print_summary(summary: &JobSummary, locale: Locale) {
    println!(
        "{}",
        msg::job_summary(
            locale,
            &summary.job_name,
            summary.archived_count,
            summary.archived_bytes,
            summary.relocated_count,
            summary.relocated_bytes,
            summary.deleted_count,
            summary.deleted_bytes,
        )
    );
    if summary.safety_brake_triggered {
        println!("{}", msg::safety_brake_triggered(locale, &summary.job_name));
    }
    for outcome in &summary.failed {
        eprintln!(
            "{}",
            msg::failed_outcome(
                locale,
                &outcome.path.display().to_string(),
                &format!("{:?}", outcome.stage),
                outcome
                    .reason
                    .as_deref()
                    .unwrap_or(msg::unknown_reason(locale)),
            )
        );
    }
}
