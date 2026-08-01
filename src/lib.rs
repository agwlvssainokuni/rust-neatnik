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

//! neatnik: ログ・作業ファイル・一時ファイルの自動ハウスキーピング
//! (アーカイブ→退避→削除)を行うCLIツールのライブラリクレート。
//!
//! `pipeline`モジュールがエントリポイントであり、`config`で読み込んだジョブ設定を
//! `scan`/`archive`/`relocate`/`delete`/`lock`の各モジュールを組み合わせて実行する。
//! `clock`/`scan::WriteGuardDetector`/`lock::JobLock`/`notify::Notifier`はテスト容易性・
//! 移植性のためにトレイトとして抽象化している(詳細はリポジトリのREADME.mdを参照)。

pub mod archive;
pub mod clock;
pub mod config;
pub mod delete;
pub mod error;
pub mod lock;
pub mod notify;
pub mod pipeline;
pub mod relocate;
pub mod scan;

#[cfg(test)]
mod test_support;
