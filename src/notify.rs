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

//! 通知抽象(FR-10)。MVPではトレイトの定義のみ行い、具体実装(メール/Slack等)は行わない。

use std::path::PathBuf;

/// エラー・セーフティブレーキ発動等の通知イベント(FR-10)。
#[derive(Debug, Clone, PartialEq)]
pub enum NotificationEvent {
    JobFailed {
        job_name: String,
        reason: String,
    },
    SafetyBrakeTriggered {
        job_name: String,
        count: usize,
        total_bytes: u64,
    },
    StageError {
        job_name: String,
        path: PathBuf,
        reason: String,
    },
}

/// 通知先の抽象(FR-10)。MVPでは本トレイトの定義のみ行い、具体実装(メール/Slack等)は行わない
pub trait Notifier {
    fn notify(&self, event: NotificationEvent);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingNotifier {
        events: std::cell::RefCell<Vec<NotificationEvent>>,
    }

    impl Notifier for RecordingNotifier {
        fn notify(&self, event: NotificationEvent) {
            self.events.borrow_mut().push(event);
        }
    }

    #[test]
    fn notifier_trait_can_be_implemented_and_dispatched() {
        let notifier = RecordingNotifier {
            events: std::cell::RefCell::new(Vec::new()),
        };
        notifier.notify(NotificationEvent::JobFailed {
            job_name: "daily-logs".to_string(),
            reason: "disk full".to_string(),
        });
        assert_eq!(notifier.events.borrow().len(), 1);
    }
}
