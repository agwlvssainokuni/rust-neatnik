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

//! CLI表示メッセージの英語/日本語対応。
//!
//! ライブラリ層(`neatnik::config`等)が生成するエラーメッセージ自体は英語のまま(技術的な
//! 詳細情報のため)であり、本モジュールが対象とするのはCLI層(`main.rs`)が組み立てる
//! ヘルプテキスト・ウェルカムガイド・サマリ出力・CLI固有のエラー案内文のみ。

use clap::builder::PossibleValue;
use clap::{Command, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Ja,
}

impl ValueEnum for Locale {
    fn value_variants<'a>() -> &'a [Self] {
        &[Locale::En, Locale::Ja]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Locale::En => PossibleValue::new("en"),
            Locale::Ja => PossibleValue::new("ja"),
        })
    }
}

impl Locale {
    /// `--lang`未指定時のフォールバック。`LC_ALL`/`LC_MESSAGES`/`LANG`の順に見て、
    /// 値が"ja"で始まれば日本語、それ以外(空でない値)は英語とする
    pub fn detect() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(var) {
                if value.is_empty() {
                    continue;
                }
                if value.to_lowercase().starts_with("ja") {
                    return Locale::Ja;
                }
                return Locale::En;
            }
        }
        Locale::En
    }

    /// `--lang`の値をclap本体でのパースより前にargvから取り出す。
    /// `--help`自体の表示言語を決めるために、正式なパースの前に必要
    pub fn from_args<I: IntoIterator<Item = String>>(args: I) -> Option<Self> {
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            if let Some(value) = arg.strip_prefix("--lang=") {
                if let Some(locale) = Self::parse(value) {
                    return Some(locale);
                }
                continue;
            }
            if arg == "--lang" {
                if let Some(value) = iter.next() {
                    if let Some(locale) = Self::parse(&value) {
                        return Some(locale);
                    }
                }
            }
        }
        None
    }

    fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "en" => Some(Locale::En),
            "ja" => Some(Locale::Ja),
            _ => None,
        }
    }
}

mod about {
    use super::Locale;

    pub fn root(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Automated housekeeping (archive, relocate, delete) for log, working and temporary files.",
            Locale::Ja => "ログ・作業ファイル・一時ファイルのハウスキーピング(アーカイブ・退避・削除)を自動化するCLIツール。",
        }
    }

    pub fn run(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Run configured jobs",
            Locale::Ja => "ジョブを実行する",
        }
    }

    pub fn validate(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Validate the configuration file",
            Locale::Ja => "設定ファイルを検証する",
        }
    }

    pub fn init(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Generate a sample configuration file",
            Locale::Ja => "サンプル設定ファイルを生成する",
        }
    }

    pub fn list(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "List configured jobs",
            Locale::Ja => "設定済みジョブの一覧を表示する",
        }
    }

    pub fn completions(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Generate shell completion scripts",
            Locale::Ja => "シェル補完スクリプトを生成する",
        }
    }
}

mod help {
    use super::Locale;

    pub fn config(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Path to the configuration file (default: ./config.yaml)",
            Locale::Ja => "設定ファイルのパス(省略時は./config.yaml)",
        }
    }

    pub fn now(locale: Locale) -> &'static str {
        match locale {
            Locale::En => {
                "Override \"now\" with a fixed RFC3339 datetime (e.g. 2027-01-01T00:00:00Z)"
            }
            Locale::Ja => {
                "「現在時刻」を指定した日時に固定する(RFC3339形式、例: 2027-01-01T00:00:00Z)"
            }
        }
    }

    pub fn job(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Name of the job to run (default: all jobs)",
            Locale::Ja => "実行対象のジョブ名(省略時は全ジョブ)",
        }
    }

    pub fn dry_run(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Preview target counts and sizes without touching any files",
            Locale::Ja => "実際のファイル操作を行わず、対象件数・合計サイズのみ表示する",
        }
    }

    pub fn output(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Path to write the generated configuration file",
            Locale::Ja => "生成する設定ファイルのパス",
        }
    }

    pub fn force(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Overwrite the file if it already exists",
            Locale::Ja => "既存ファイルを上書きする",
        }
    }

    pub fn lang(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "Output language. Defaults to the LANG/LC_ALL environment variable",
            Locale::Ja => "出力言語。省略時はLANG/LC_ALL環境変数に従う",
        }
    }
}

/// `Cli::command()`が生成した`Command`の about/help テキストを指定言語に差し替える。
/// clap自体が生成する構造テキスト("Usage:", "Options:", "Print help"等)は対象外(clapの制約)
pub fn localize_command(cmd: Command, locale: Locale) -> Command {
    let cmd = cmd.about(about::root(locale));

    let cmd = cmd.mut_subcommand("run", |sub| {
        localize_config_opts(sub.about(about::run(locale)), locale)
            .mut_arg("job", |a| a.help(help::job(locale)))
            .mut_arg("dry_run", |a| a.help(help::dry_run(locale)))
    });
    let cmd = cmd.mut_subcommand("validate", |sub| {
        localize_config_opts(sub.about(about::validate(locale)), locale)
    });
    let cmd = cmd.mut_subcommand("init", |sub| {
        sub.about(about::init(locale))
            .mut_arg("output", |a| a.help(help::output(locale)))
            .mut_arg("force", |a| a.help(help::force(locale)))
    });
    let cmd = cmd.mut_subcommand("list", |sub| {
        localize_config_opts(sub.about(about::list(locale)), locale)
    });
    let cmd = cmd.mut_subcommand("completions", |sub| sub.about(about::completions(locale)));

    cmd.mut_arg("lang", |a| a.help(help::lang(locale)))
}

fn localize_config_opts(cmd: Command, locale: Locale) -> Command {
    cmd.mut_arg("config", |a| a.help(help::config(locale)))
        .mut_arg("now", |a| a.help(help::now(locale)))
}

/// CLI層が組み立てる実行時メッセージ(ウェルカムガイド・サマリ・CLI固有のエラー案内)。
pub mod msg {
    use super::Locale;
    use std::path::Path;

    pub fn welcome(locale: Locale) -> &'static str {
        match locale {
            Locale::En => concat!(
                "neatnik - automated housekeeping for log, working and temporary files\n",
                "\n",
                "Getting started:\n",
                "  1. neatnik init             Generate a sample configuration file (config.yaml)\n",
                "  2. neatnik validate         Check the configuration for errors\n",
                "  3. neatnik run --dry-run    Preview what would happen without touching any files\n",
                "  4. neatnik run              Execute the configured jobs\n",
                "\n",
                "Run `neatnik --help` for the full list of commands and options."
            ),
            Locale::Ja => concat!(
                "neatnik - ログ・作業ファイル・一時ファイルのハウスキーピングを自動化するツール\n",
                "\n",
                "はじめに:\n",
                "  1. neatnik init             サンプル設定ファイル(config.yaml)を生成する\n",
                "  2. neatnik validate         設定内容にエラーがないか確認する\n",
                "  3. neatnik run --dry-run    ファイルを変更せずに何が起きるかを確認する\n",
                "  4. neatnik run              設定済みのジョブを実行する\n",
                "\n",
                "コマンドとオプションの一覧は `neatnik --help` を参照してください。"
            ),
        }
    }

    pub fn config_not_found(locale: Locale, path: &Path) -> String {
        match locale {
            Locale::En => format!(
                "config file \"{}\" was not found. Run `neatnik init --output {}` to generate a sample configuration file.",
                path.display(),
                path.display()
            ),
            Locale::Ja => format!(
                "設定ファイル\"{}\"が見つかりません。`neatnik init --output {}` を実行してサンプル設定ファイルを生成してください。",
                path.display(),
                path.display()
            ),
        }
    }

    pub fn invalid_now(locale: Locale, value: &str, err: &str) -> String {
        match locale {
            Locale::En => {
                format!("invalid --now value \"{value}\": {err} (expected RFC3339, e.g. 2027-01-01T00:00:00Z)")
            }
            Locale::Ja => {
                format!("--nowの値\"{value}\"が不正です: {err}(RFC3339形式で指定してください。例: 2027-01-01T00:00:00Z)")
            }
        }
    }

    pub fn job_not_found(locale: Locale, name: &str) -> String {
        match locale {
            Locale::En => format!("job \"{name}\" was not found in the configuration"),
            Locale::Ja => format!("ジョブ\"{name}\"は設定内に見つかりません"),
        }
    }

    pub fn already_exists(locale: Locale, path: &Path) -> String {
        match locale {
            Locale::En => format!(
                "\"{}\" already exists. Use --force to overwrite it.",
                path.display()
            ),
            Locale::Ja => format!(
                "\"{}\"は既に存在します。上書きするには--forceを指定してください。",
                path.display()
            ),
        }
    }

    pub fn wrote_sample_config(locale: Locale, path: &Path) -> String {
        match locale {
            Locale::En => format!("wrote a sample configuration to \"{}\"", path.display()),
            Locale::Ja => format!("サンプル設定を\"{}\"に書き込みました", path.display()),
        }
    }

    pub fn configuration_is_valid(locale: Locale, job_count: usize) -> String {
        match locale {
            Locale::En => format!("configuration is valid ({job_count} job(s))"),
            Locale::Ja => format!("設定は正常です(ジョブ数: {job_count})"),
        }
    }

    pub fn no_jobs_configured(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "no jobs are configured",
            Locale::Ja => "ジョブが設定されていません",
        }
    }

    pub fn job_skipped_locked(locale: Locale, job_name: &str) -> String {
        match locale {
            Locale::En => format!("job \"{job_name}\" skipped: already running (lock held, BR-16)"),
            Locale::Ja => format!(
                "ジョブ\"{job_name}\"はスキップされました: 多重起動を検知(ロック取得済み、BR-16)"
            ),
        }
    }

    pub fn job_failed(locale: Locale, job_name: &str, err: &str) -> String {
        match locale {
            Locale::En => format!("job \"{job_name}\" failed: {err}"),
            Locale::Ja => format!("ジョブ\"{job_name}\"が失敗しました: {err}"),
        }
    }

    pub fn one_or_more_jobs_failed(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "one or more jobs failed",
            Locale::Ja => "1件以上のジョブが失敗しました",
        }
    }

    pub fn warning_prefix(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "warning",
            Locale::Ja => "警告",
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn job_summary(
        locale: Locale,
        job_name: &str,
        archived_count: u64,
        archived_bytes: u64,
        relocated_count: u64,
        relocated_bytes: u64,
        deleted_count: u64,
        deleted_bytes: u64,
    ) -> String {
        match locale {
            Locale::En => format!(
                "job \"{job_name}\": archived {archived_count} ({archived_bytes} bytes), relocated {relocated_count} ({relocated_bytes} bytes), deleted {deleted_count} ({deleted_bytes} bytes)"
            ),
            Locale::Ja => format!(
                "ジョブ\"{job_name}\": アーカイブ {archived_count}件({archived_bytes}バイト)、退避 {relocated_count}件({relocated_bytes}バイト)、削除 {deleted_count}件({deleted_bytes}バイト)"
            ),
        }
    }

    pub fn safety_brake_triggered(locale: Locale, job_name: &str) -> String {
        match locale {
            Locale::En => format!("  safety brake triggered for job \"{job_name}\" (BR-13)"),
            Locale::Ja => {
                format!("  ジョブ\"{job_name}\"でセーフティブレーキが発動しました(BR-13)")
            }
        }
    }

    pub fn failed_outcome(locale: Locale, path: &str, stage: &str, reason: &str) -> String {
        match locale {
            Locale::En => format!("  failed: {path} ({stage}): {reason}"),
            Locale::Ja => format!("  失敗: {path} ({stage}): {reason}"),
        }
    }

    pub fn unknown_reason(locale: Locale) -> &'static str {
        match locale {
            Locale::En => "unknown reason",
            Locale::Ja => "不明な理由",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_args_recognizes_space_separated_form() {
        let args = vec!["run".to_string(), "--lang".to_string(), "ja".to_string()];
        assert_eq!(Locale::from_args(args), Some(Locale::Ja));
    }

    #[test]
    fn from_args_recognizes_equals_form() {
        let args = vec!["run".to_string(), "--lang=en".to_string()];
        assert_eq!(Locale::from_args(args), Some(Locale::En));
    }

    #[test]
    fn from_args_ignores_unknown_values() {
        let args = vec!["--lang".to_string(), "fr".to_string()];
        assert_eq!(Locale::from_args(args), None);
    }

    #[test]
    fn from_args_returns_none_when_absent() {
        let args = vec!["run".to_string(), "--dry-run".to_string()];
        assert_eq!(Locale::from_args(args), None);
    }
}
