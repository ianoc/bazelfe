use crate::{
    build_events::build_event_server::bazel_event::ProgressEvt, label_utils::sanitize_label,
};
use bazelfe_protos::*;
use lazy_static::lazy_static;
use tokio::sync::{Mutex, RwLock};

use crate::{build_events::hydrated_stream, buildozer_driver::Buildozer};
use regex::Regex;
use std::{collections::HashMap, sync::Arc, time::Instant};

use super::CurrentState;
#[derive(Clone, PartialEq, Debug)]

enum BazelCorrectionCommand {
    BuildozerRemoveDep(BuildozerRemoveDepCmd),
}
#[derive(Clone, PartialEq, Debug)]
struct BuildozerRemoveDepCmd {
    pub target_to_operate_on: String,
    pub dependency_to_remove: String,
    pub why: String,
}

fn extract_target_does_not_exist(
    bazel_abort_error_info: &hydrated_stream::BazelAbortErrorInfo,
    command_stream: &mut Vec<BazelCorrectionCommand>,
) {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"^\s*in deps attribute of [A-Za-z0-9_-]* rule (.*): target '(.*)' does not exist\s*$"
        )
        .unwrap();
    }

    if let Some(build_event_stream::aborted::AbortReason::AnalysisFailure) =
        bazel_abort_error_info.reason
    {
        for ln in bazel_abort_error_info.description.lines() {
            let captures = RE.captures(ln);

            match captures {
                None => (),
                Some(captures) => {
                    let src_target = captures.get(1).unwrap().as_str();
                    let offending_dependency = captures.get(2).unwrap().as_str();

                    let correction =
                        BazelCorrectionCommand::BuildozerRemoveDep(BuildozerRemoveDepCmd {
                            target_to_operate_on: src_target.to_string(),
                            dependency_to_remove: offending_dependency.to_string(),
                            why: String::from("Dependency on does not exist"),
                        });
                    command_stream.push(correction);
                }
            }
        }
    }
}

fn extract_target_not_declared_in_package(
    bazel_progress_error_info: &ProgressEvt,
    command_stream: &mut Vec<BazelCorrectionCommand>,
) {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r".*no such target '([^']*)': target '.*' not declared in package '.*' defined by .* and referenced by '([^']*)'"
        )
        .unwrap();
    }

    for ln in bazel_progress_error_info.stderr.lines() {
        let captures = RE.captures(ln);

        match captures {
            None => (),
            Some(captures) => {
                let src_target = captures.get(2).unwrap().as_str();
                let offending_dependency = captures.get(1).unwrap().as_str();

                let correction =
                    BazelCorrectionCommand::BuildozerRemoveDep(BuildozerRemoveDepCmd {
                        target_to_operate_on: src_target.to_string(),
                        dependency_to_remove: offending_dependency.to_string(),
                        why: String::from("Dependency on does not exist"),
                    });
                command_stream.push(correction);
            }
        }
    }
}

fn extract_target_not_visible(
    bazel_abort_error_info: &hydrated_stream::BazelAbortErrorInfo,
    command_stream: &mut Vec<BazelCorrectionCommand>,
) {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r"^\s*in [A-Za-z0-9_-]* rule (.*): target '(.*)' is not visible from target '.*'. Check the visibility declaration of the former target if you think the dependency is legitimate\s*$"
        )
        .unwrap();
    }

    if let Some(build_event_stream::aborted::AbortReason::AnalysisFailure) =
        bazel_abort_error_info.reason
    {
        for ln in bazel_abort_error_info.description.lines() {
            let captures = RE.captures(ln);

            match captures {
                None => (),
                Some(captures) => {
                    let src_target = captures.get(1).unwrap().as_str();
                    let offending_dependency = captures.get(2).unwrap().as_str();

                    let correction =
                        BazelCorrectionCommand::BuildozerRemoveDep(BuildozerRemoveDepCmd {
                            target_to_operate_on: src_target.to_string(),
                            dependency_to_remove: offending_dependency.to_string(),
                            why: String::from(
                                "Target dependended on is not visible from the current target",
                            ),
                        });
                    command_stream.push(correction);
                }
            }
        }
    }
}

async fn extract_added_cycle_in_dependency_graph(
    bazel_abort_error_info: &ProgressEvt,
    command_stream: &mut Vec<BazelCorrectionCommand>,
    previous_global_seen: &Arc<RwLock<HashMap<String, Arc<Mutex<CurrentState>>>>>,
) {
    // ERROR: .*/BUILD:\d*:\d*: in [A-Za-z0-9_-]* rule (.*): cycle in dependency graph:
    // .-> //src/main/java/com/example/foo/actions:actions
    // |   //src/main/java/com/example/foo:bar
    // `-- //src/main/java/com/example/foo/actions:actions

    lazy_static! {
        static ref RE: Regex = Regex::new(
            r".*ERROR: .*/BUILD:\d*:\d*: in [A-Za-z0-9_-]* rule (.*): cycle in dependency graph:\s*$"
        )
        .unwrap();

        static ref START_SEGMENT: Regex = Regex::new(
            r"^\s*.->\s*(.*)$"
        )
        .unwrap();
        static ref MIDDLE_SEGMENT: Regex = Regex::new(
            r"^\s*\|\s*(.*)$"
        )
        .unwrap();
        static ref END_SEGMENT: Regex = Regex::new(
            r"^\s*`--\s*(.*)$"
        )
        .unwrap();
    }

    let mut in_segment_vec: Option<Vec<String>> = None;
    for ln in bazel_abort_error_info.stderr.lines() {
        let mut end_found = false;
        if let Some(ref mut vec) = in_segment_vec.as_mut() {
            if let Some(captures) = START_SEGMENT.captures(ln) {
                vec.push(captures.get(1).unwrap().as_str().to_string());
            } else if let Some(captures) = MIDDLE_SEGMENT.captures(ln) {
                vec.push(captures.get(1).unwrap().as_str().to_string());
            } else if let Some(captures) = END_SEGMENT.captures(ln) {
                vec.push(captures.get(1).unwrap().as_str().to_string());

                for wind in vec.windows(2) {
                    let target_to_operate_on = sanitize_label(wind[0].to_string());
                    let dependency_to_remove = wind[1].to_string();

                    if let Some(hashset) =
                        previous_global_seen.read().await.get(&target_to_operate_on)
                    {
                        let data = hashset.lock().await;
                        if data.ignore_list.contains(&dependency_to_remove) {
                            let correction =
                                BazelCorrectionCommand::BuildozerRemoveDep(BuildozerRemoveDepCmd {
                                    target_to_operate_on,
                                    dependency_to_remove,
                                    why: String::from("There is a cyclic dependency, so attempting to unwind/remove dependencies")
                                });
                            command_stream.push(correction);
                        }
                    }
                }

                end_found = true;
            }
        }
        if end_found {
            in_segment_vec = None;
        }

        let captures = RE.captures(ln);
        match captures {
            None => (),
            Some(_) => {
                in_segment_vec = Some(vec![]);
            }
        }
    }
}

async fn apply_candidates<T: Buildozer + Clone + Send + Sync + 'static>(
    candidate_correction_commands: Vec<BazelCorrectionCommand>,
    buildozer: T,
) -> super::Response {
    let mut target_stories = Vec::default();
    if candidate_correction_commands.is_empty() {
        return super::Response::new(Vec::default());
    }
    for correction_command in candidate_correction_commands.into_iter() {
        match correction_command {
            BazelCorrectionCommand::BuildozerRemoveDep(buildozer_remove_dep) => {
                let dependency_to_remove = buildozer_remove_dep.dependency_to_remove;
                let target_to_operate_on = buildozer_remove_dep.target_to_operate_on;
                // otherwise... add the dependency with buildozer here
                // then add it ot the local seen dependencies
                debug!(
                    "Buildozer action: remove dependency {:?} from {:?}",
                    dependency_to_remove, target_to_operate_on
                );
                let buildozer_res = buildozer
                    .remove_dependency(&target_to_operate_on, &dependency_to_remove)
                    .await;
                match buildozer_res {
                    Ok(_) => {
                        target_stories.push(super::TargetStory {
                            target: target_to_operate_on.clone(),
                            action: super::TargetStoryAction::RemovedDependency {
                                removed_what: dependency_to_remove.clone(),
                                why: buildozer_remove_dep.why.clone(),
                            },
                            when: Instant::now(),
                        });
                    }
                    Err(_) => info!("Buildozer command failed"),
                }
            }
        }
    }
    super::Response::new(target_stories)
}
pub async fn process_progress<T: Buildozer + Clone + Send + Sync + 'static>(
    buildozer: T,
    bazel_progress_error_info: &ProgressEvt,
    previous_global_seen: Arc<RwLock<HashMap<String, Arc<Mutex<CurrentState>>>>>,
) -> super::Response {
    let mut candidate_correction_commands: Vec<BazelCorrectionCommand> = vec![];

    extract_added_cycle_in_dependency_graph(
        bazel_progress_error_info,
        &mut candidate_correction_commands,
        &previous_global_seen,
    )
    .await;

    extract_target_not_declared_in_package(
        bazel_progress_error_info,
        &mut candidate_correction_commands,
    );

    apply_candidates(candidate_correction_commands, buildozer).await
}

pub async fn process_build_abort_errors<T: Buildozer + Clone + Send + Sync + 'static>(
    buildozer: T,
    bazel_abort_error_info: &hydrated_stream::BazelAbortErrorInfo,
) -> super::Response {
    let mut candidate_correction_commands: Vec<BazelCorrectionCommand> = vec![];

    extract_target_does_not_exist(bazel_abort_error_info, &mut candidate_correction_commands);
    extract_target_not_visible(bazel_abort_error_info, &mut candidate_correction_commands);
    apply_candidates(candidate_correction_commands, buildozer).await
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn test_extract_target_does_not_exist() {
        // This was referring to a random string put into the dependencies list of the target
        let sample_output = hydrated_stream::BazelAbortErrorInfo {
            description: String::from("in deps attribute of java_library rule //src/main/java/com/example:Example: target '//src/main/java/com/example:asdfasdf' does not exist"),
            reason: Some(build_event_stream::aborted::AbortReason::AnalysisFailure),
            label: None
        };

        let mut results = vec![];
        extract_target_does_not_exist(&sample_output, &mut results);
        assert_eq!(
            results,
            vec![BazelCorrectionCommand::BuildozerRemoveDep(
                BuildozerRemoveDepCmd {
                    target_to_operate_on: String::from("//src/main/java/com/example:Example"),
                    dependency_to_remove: String::from("//src/main/java/com/example:asdfasdf"),
                    why: String::from("Dependency on does not exist"),
                }
            )]
        );
    }

    #[test]
    fn test_extract_target_not_declared_in_package() {
        // This was referring to a random string put into the dependencies list of the target
        let sample_output = ProgressEvt {
            stderr: String::from("no such target '//src/main/java/com/example/foo:foo': target 'foo' not declared in package 'src/main/java/com/example/foo' defined by /User/jim/github/example_bazel_project/src/main/java/com/example/foo/BUILD and referenced by '//src/main/java/com/example/c:c'"),
            stdout: String::from("")
        };

        let mut results = vec![];
        extract_target_not_declared_in_package(&sample_output, &mut results);
        assert_eq!(
            results,
            vec![BazelCorrectionCommand::BuildozerRemoveDep(
                BuildozerRemoveDepCmd {
                    target_to_operate_on: String::from("//src/main/java/com/example/c:c"),
                    dependency_to_remove: String::from("//src/main/java/com/example/foo:foo"),
                    why: String::from("Dependency on does not exist"),
                }
            )]
        );
    }

    #[test]
    fn test_extract_target_not_visible() {
        // This was referring to a random string put into the dependencies list of the target
        let sample_output = hydrated_stream::BazelAbortErrorInfo {
            description: String::from("in java_library rule //src/main/java/com/com/example:Example: target '@third_party_jvm//3rdparty/jvm/com/google/api/grpc:proto_google_common_protos' is not visible from target '//src/main/java/com/com/example:Example'. Check the visibility declaration of the former target if you think the dependency is legitimate"),
            reason: Some(build_event_stream::aborted::AbortReason::AnalysisFailure),
            label: None
        };

        let mut results = vec![];
        extract_target_not_visible(&sample_output, &mut results);
        assert_eq!(
            results,
            vec![BazelCorrectionCommand::BuildozerRemoveDep(
                BuildozerRemoveDepCmd {
                    target_to_operate_on: String::from("//src/main/java/com/com/example:Example"),
                    dependency_to_remove: String::from("@third_party_jvm//3rdparty/jvm/com/google/api/grpc:proto_google_common_protos"),
                    why: String::from("Target dependended on is not visible from the current target"),
                }
            )]
        );
    }

    #[tokio::test]
    async fn test_extract_added_cycle_in_dependency_graph() {
        // This was referring to a random string put into the dependencies list of the target
        let sample_output = ProgressEvt {
                stderr: String::from("ERROR: /Users/exampleuser/example_path/example_repo/src/main/java/com/example/foo/actions/BUILD:1:13: in java_library rule //src/main/java/com/example/foo/actions:actions: cycle in dependency graph:
    .-> //src/main/java/com/example/foo/actions:actions
    |   //src/main/java/com/example/foo:bar
    `-- //src/main/java/com/example/foo/actions:actions"),
                stdout: String::from("")
            };

        let mut results = vec![];
        let previous_global_seen = Arc::new(RwLock::new(HashMap::new()));
        extract_added_cycle_in_dependency_graph(
            &sample_output,
            &mut results,
            &previous_global_seen,
        )
        .await;
        assert_eq!(results, vec![]);
    }

    #[tokio::test]
    async fn test_extract_added_cycle_in_dependency_graph_with_state() {
        // This was referring to a random string put into the dependencies list of the target
        let sample_output = ProgressEvt {
                stderr: String::from("ERROR: /Users/exampleuser/example_path/example_repo/src/main/java/com/example/foo/actions/BUILD:1:13: in java_library rule //src/main/java/com/example/foo/actions:actions: cycle in dependency graph:
    .-> //src/main/java/com/example/foo/actions:actions
    |   //src/main/java/com/example/foo:bar
    `-- //src/main/java/com/example/foo/actions:actions"),
                stdout: String::from("")
            };

        let mut results = vec![];
        let previous_global_seen = Arc::new(RwLock::new(HashMap::new()));
        let mut current_state = CurrentState::default();
        current_state.ignore_list.insert(String::from(
            "//src/main/java/com/example/foo/actions:actions",
        ));
        previous_global_seen.write().await.insert(
            String::from("//src/main/java/com/example/foo:bar"),
            Arc::new(Mutex::new(current_state)),
        );
        extract_added_cycle_in_dependency_graph(
            &sample_output,
            &mut results,
            &previous_global_seen,
        )
        .await;
        assert_eq!(
            results,
            vec![BazelCorrectionCommand::BuildozerRemoveDep(
                BuildozerRemoveDepCmd {
                    target_to_operate_on: String::from("//src/main/java/com/example/foo:bar"),
                    dependency_to_remove: String::from(
                        "//src/main/java/com/example/foo/actions:actions"
                    ),
                    why: String::from(
                        "There is a cyclic dependency, so attempting to unwind/remove dependencies"
                    ),
                }
            ),]
        );
    }
}
