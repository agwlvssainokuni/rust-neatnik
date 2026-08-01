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

use chrono::{DateTime, Utc};

/// 「現在時刻」取得の抽象。テスト容易性(PBT-09)と`--now`オーバーライド(FR-13)のために導入する。
pub trait Clock {
    fn now(&self) -> DateTime<Utc>;
}

/// システムの実時刻を返す実装。CLIの通常動作(`--now`未指定時)で使う。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 固定の日時を返す実装。`--now`オプション(FR-13)およびテストで使う。
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn system_clock_returns_a_recent_time() {
        let before = Utc::now();
        let now = SystemClock.now();
        let after = Utc::now();
        assert!(before <= now && now <= after);
    }

    #[test]
    fn fixed_clock_always_returns_the_same_time() {
        let fixed = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
        let clock = FixedClock::new(fixed);
        assert_eq!(clock.now(), fixed);
        assert_eq!(clock.now(), fixed);
    }
}
