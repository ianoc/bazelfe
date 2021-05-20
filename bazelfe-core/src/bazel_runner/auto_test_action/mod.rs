mod app;
mod command_line_driver;
mod ctrl_char;
mod progress_tab_updater;
mod ui;
mod util;

use std::sync::Arc;

use crate::bazel_command_line_parser::BuiltInAction;
use crate::{
    bazel_command_line_parser::CustomAction, bazel_runner_daemon::daemon_service::FileStatus,
    buildozer_driver,
};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AutoTestActionError {
    #[error("Requested Autotest, but the daemon isn't running")]
    NoDaemon,
}

use super::configured_bazel_runner::ConfiguredBazelRunner;

pub async fn maybe_auto_test_mode<
    T: buildozer_driver::Buildozer,
    U: crate::hydrated_stream_processors::process_bazel_failures::CommandLineRunner,
>(
    configured_bazel_runner: &mut ConfiguredBazelRunner<T, U>,
) -> Result<bool, Box<dyn std::error::Error>> {
    if configured_bazel_runner.bazel_command_line.action
        == Some(crate::bazel_command_line_parser::Action::Custom(
            CustomAction::AutoTest,
        ))
    {
        configured_bazel_runner.bazel_command_line.action = Some(
            crate::bazel_command_line_parser::Action::BuiltIn(BuiltInAction::Test),
        );

        let daemon_cli = if let Some(daemon_cli) = configured_bazel_runner.runner_daemon.as_ref() {
            Ok(daemon_cli)
        } else {
            Err(AutoTestActionError::NoDaemon)
        }?;
        let (progress_pump_sender, progress_receiver) = flume::unbounded::<String>();

        let progress_tab_updater =
            progress_tab_updater::ProgressTabUpdater::new(progress_pump_sender);

        configured_bazel_runner
            .configured_bazel
            .aes
            .add_event_handler(Arc::new(progress_tab_updater));

        let mut invalid_since_when: u128 = 0;
        let mut cur_distance = 1;
        let max_distance = 3;
        let mut dirty_files: Vec<FileStatus> = Vec::default();

        let main_running = command_line_driver::main(progress_receiver)?;
        'outer_loop: loop {
            match main_running.try_recv() {
                Ok(inner_result) => {
                    if let Err(e) = inner_result {
                        eprintln!("UX system failed with: {}", e);
                        break 'outer_loop;
                    }
                }
                Err(e) => match e {
                    flume::TryRecvError::Empty => (),
                    flume::TryRecvError::Disconnected => {
                        break 'outer_loop;
                    }
                },
            }
            let recent_changed_files: Vec<FileStatus> = daemon_cli
                .wait_for_files(tarpc::context::current(), invalid_since_when)
                .await?;
            if !recent_changed_files.is_empty() {
                dirty_files.extend(recent_changed_files);

                'inner_loop: loop {
                    let changed_targets = daemon_cli
                        .targets_from_files(
                            tarpc::context::current(),
                            dirty_files.clone(),
                            cur_distance,
                        )
                        .await?;

                    if !changed_targets.is_empty() {
                        configured_bazel_runner.bazel_command_line.action = Some(
                            crate::bazel_command_line_parser::Action::BuiltIn(BuiltInAction::Build),
                        );
                        configured_bazel_runner
                            .bazel_command_line
                            .remaining_args
                            .clear();

                        for t in changed_targets.iter() {
                            configured_bazel_runner
                                .bazel_command_line
                                .remaining_args
                                .push(t.target_label().clone());
                        }

                        if cur_distance == 1 {
                            invalid_since_when = daemon_cli
                                .request_instant(tarpc::context::current())
                                .await?;
                        }
                        eprintln!(
                            "Building... {}",
                            configured_bazel_runner
                                .bazel_command_line
                                .remaining_args
                                .join(", ")
                        );
                        let result = configured_bazel_runner.run_command_line(false).await?;
                        if result.final_exit_code != 0 {
                            continue 'outer_loop;
                        }

                        // Now try tests

                        configured_bazel_runner
                            .bazel_command_line
                            .remaining_args
                            .clear();

                        for t in changed_targets.iter() {
                            if t.is_test() {
                                configured_bazel_runner
                                    .bazel_command_line
                                    .remaining_args
                                    .push(t.target_label().clone());
                            }
                        }

                        if !configured_bazel_runner
                            .bazel_command_line
                            .remaining_args
                            .is_empty()
                        {
                            configured_bazel_runner.bazel_command_line.action =
                                Some(crate::bazel_command_line_parser::Action::BuiltIn(
                                    BuiltInAction::Test,
                                ));

                            eprintln!(
                                "Testing... {}",
                                configured_bazel_runner
                                    .bazel_command_line
                                    .remaining_args
                                    .join(", ")
                            );

                            let result = configured_bazel_runner.run_command_line(false).await?;
                            if result.final_exit_code != 0 {
                                continue 'outer_loop;
                            }
                        }

                        eprintln!(
                        "Operating at distance {}, all targets built and tested that were eligble.",
                        cur_distance
                    );
                    }
                    if cur_distance >= max_distance {
                        cur_distance = 1;
                        dirty_files.clear();
                        break 'inner_loop;
                    } else {
                        cur_distance += 1;
                    }
                }
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {}
