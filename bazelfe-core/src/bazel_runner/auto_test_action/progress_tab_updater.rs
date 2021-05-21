use std::time::Instant;

use crate::hydrated_stream_processors::BazelEventHandler;

#[derive(Debug)]
pub struct ProgressTabUpdater {
    progress_pump: flume::Sender<String>,
    action_event_tx: flume::Sender<super::ActionTargetStateScrollEntry>,
}

impl ProgressTabUpdater {
    pub fn new(
        progress_pump: flume::Sender<String>,
        action_event_tx: flume::Sender<super::ActionTargetStateScrollEntry>,
    ) -> Self {
        Self {
            progress_pump,
            action_event_tx,
        }
    }
}

#[async_trait::async_trait]
impl BazelEventHandler for ProgressTabUpdater {
    async fn process_event(
        &self,
        bazel_run_id: usize,
        event: &crate::build_events::hydrated_stream::HydratedInfo,
    ) -> Vec<crate::hydrated_stream_processors::BuildEventResponse> {
        match event {
            crate::build_events::hydrated_stream::HydratedInfo::Progress(p) => {
                if !p.stderr.is_empty() {
                    let _ = self.progress_pump.send_async(p.stderr.clone()).await;
                }
                if !p.stdout.is_empty() {
                    let _ = self.progress_pump.send_async(p.stdout.clone()).await;
                }
            }

            crate::build_events::hydrated_stream::HydratedInfo::BazelAbort(_ba) => {}
            crate::build_events::hydrated_stream::HydratedInfo::ActionFailed(af) => {
                let _ = self
                    .action_event_tx
                    .send_async(super::ActionTargetStateScrollEntry {
                        complete_type: super::CompleteKind::Action,
                        success: false,
                        label: af.label.clone(),
                        when: Instant::now(),
                        bazel_run_id,
                    })
                    .await;
            }
            crate::build_events::hydrated_stream::HydratedInfo::ActionSuccess(action_success) => {
                let _ = self
                    .action_event_tx
                    .send_async(super::ActionTargetStateScrollEntry {
                        complete_type: super::CompleteKind::Action,
                        success: true,
                        label: action_success.label.clone(),
                        when: Instant::now(),
                        bazel_run_id,
                    })
                    .await;
            }
            crate::build_events::hydrated_stream::HydratedInfo::TargetComplete(tc) => {
                let _ = self
                    .action_event_tx
                    .send_async(super::ActionTargetStateScrollEntry {
                        complete_type: super::CompleteKind::Target,
                        success: tc.success,
                        label: tc.label.clone(),
                        when: Instant::now(),
                        bazel_run_id,
                    })
                    .await;
            }
            crate::build_events::hydrated_stream::HydratedInfo::TestResult(tst) => {
                let is_success = match tst.test_summary_event.test_status {
                    crate::build_events::build_event_server::bazel_event::TestStatus::Passed => true,
                    crate::build_events::build_event_server::bazel_event::TestStatus::Flaky => false,
                    crate::build_events::build_event_server::bazel_event::TestStatus::Timeout => false,
                    crate::build_events::build_event_server::bazel_event::TestStatus::Failed => false,
                    crate::build_events::build_event_server::bazel_event::TestStatus::Incomplete => false,
                    crate::build_events::build_event_server::bazel_event::TestStatus::RemoteFailure => false,
                    crate::build_events::build_event_server::bazel_event::TestStatus::FailedToBuild => false,
                    crate::build_events::build_event_server::bazel_event::TestStatus::ToolHaltedBeforeTesting => false,
                };
                let _ = self
                    .action_event_tx
                    .send_async(super::ActionTargetStateScrollEntry {
                        complete_type: super::CompleteKind::Test,
                        success: is_success,
                        label: tst.test_summary_event.label.clone(),
                        when: Instant::now(),
                        bazel_run_id,
                    })
                    .await;
            }
        }
        Vec::default()
    }
}
