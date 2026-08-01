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

//! PBT-07: 各モジュールのプロパティテストで共通に使うジェネレータ。テストビルドでのみ有効

#![cfg(test)]

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;

/// 秒精度のUTC日時を2000-01-01〜2099-12-28の範囲で生成する(グレゴリオ暦上、月に依らず
/// 常に妥当な日付になるよう日は28日までに制限する)
pub fn arb_utc_datetime() -> impl Strategy<Value = DateTime<Utc>> {
    (2000i32..2100, 1u32..13, 1u32..29, 0u32..24, 0u32..60, 0u32..60)
        .prop_map(|(year, month, day, hour, minute, second)| Utc.with_ymd_and_hms(year, month, day, hour, minute, second).unwrap())
}

/// ファイルの内容として使う、非空のランダムバイト列を生成する
pub fn arb_file_bytes() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 1..256)
}
