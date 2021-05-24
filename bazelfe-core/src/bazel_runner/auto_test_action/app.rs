use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use super::util::{StatefulList, TabsState};
use bazelfe_protos::*;

#[derive(Debug, PartialEq)]
pub struct FailureState {
    pub output_files: Vec<build_event_stream::file::File>,
    pub target_kind: String,
    pub bazel_run_id: usize,
    pub label: String,
    pub when: Instant,
}

pub struct App<'a> {
    pub title: &'a str,
    pub should_quit: bool,
    pub tabs: TabsState<'a>,
    pub error_tab_position: isize,
    pub show_chart: bool,
    pub progress: f64,
    pub action_logs: StatefulList<super::ActionTargetStateScrollEntry>,
    pub progress_receiver: flume::Receiver<String>,
    pub file_change_receiver: flume::Receiver<PathBuf>,
    pub recent_files: HashMap<PathBuf, Instant>,
    pub bazel_status_rx: flume::Receiver<super::BazelStatus>,
    pub bazel_status: super::BazelStatus,
    pub build_status_rx: flume::Receiver<super::BuildStatus>,
    pub build_status: super::BuildStatus,
    pub progress_logs: Vec<String>,
    pub scroll_h: u16,
    pub scroll_w: u16,
    pub action_event_rx: flume::Receiver<super::ActionTargetStateScrollEntry>,
    pub failure_state: HashMap<String, FailureState>,
}

impl<'a> App<'a> {
    pub fn new(
        title: &'a str,
        progress_receiver: flume::Receiver<String>,
        file_change_receiver: flume::Receiver<PathBuf>,
        action_event_rx: flume::Receiver<super::ActionTargetStateScrollEntry>,
        bazel_status_rx: flume::Receiver<super::BazelStatus>,
        build_status_rx: flume::Receiver<super::BuildStatus>,
    ) -> App<'a> {
        App {
            title,
            should_quit: false,
            tabs: TabsState::new(vec!["Build Activity", "Bazel logs"]),
            error_tab_position: 0,
            show_chart: true,
            progress: 0.0,
            action_logs: StatefulList::new(),
            progress_receiver,
            file_change_receiver,
            action_event_rx,
            bazel_status_rx,
            bazel_status: super::BazelStatus::Idle,
            build_status_rx,

            build_status: super::BuildStatus::ActionsGreen,
            recent_files: HashMap::default(),
            progress_logs: Vec::default(),
            scroll_h: 0,
            scroll_w: 0,
            failure_state: HashMap::default(),
        }
    }

    pub fn on_up(&mut self) {
        // self.tasks.previous();
    }

    pub fn on_down(&mut self) {
        // self.tasks.next();
    }

    pub fn on_page_down(&mut self) {
        if self.scroll_h > 20 {
            self.scroll_h -= 20;
        } else {
            self.scroll_h = 0;
        }
    }

    pub fn on_page_up(&mut self) {
        self.scroll_h += 20;
    }

    pub fn on_right(&mut self) {
        self.error_tab_position += 1;
    }

    pub fn on_left(&mut self) {
        self.error_tab_position -= 1;
    }

    pub fn on_tab(&mut self) {
        self.scroll_h = 0;
        self.scroll_w = 0;
        self.tabs.next();
        self.error_tab_position = 0;
    }

    pub fn on_back_tab(&mut self) {
        self.scroll_h = 0;
        self.scroll_w = 0;
        self.tabs.previous();
        self.error_tab_position = 0;
    }

    pub fn scroll(&mut self) -> (u16, u16) {
        (self.scroll_h, self.scroll_w)
    }

    pub fn on_key(&mut self, c: char) {
        match c {
            'q' => {
                self.should_quit = true;
            }
            't' => {
                self.show_chart = !self.show_chart;
            }
            _ => {}
        }
    }

    pub fn on_tick(&mut self) {
        // Update progress
        self.progress += 0.001;
        if self.progress > 1.0 {
            self.progress = 0.0;
        }

        while let Ok(r) = self.bazel_status_rx.try_recv() {
            self.bazel_status = r;
        }

        while let Ok(r) = self.build_status_rx.try_recv() {
            self.build_status = r;
        }

        while let Ok(r) = self.action_event_rx.try_recv() {
            if r.success {
                let _ = self.failure_state.remove(&r.label);
            } else {
                let f = FailureState {
                    output_files: r.files.clone(),
                    target_kind: "Dunno.".to_string(),
                    bazel_run_id: r.bazel_run_id,
                    when: r.when.clone(),
                    label: r.label.clone(),
                };
                self.failure_state.insert(r.label.clone(), f);
            }

            let mut prev_idx = None;
            for (idx, item) in self.action_logs.items.iter().enumerate() {
                // starts at the left which is the newest
                if item.bazel_run_id != r.bazel_run_id {
                    break;
                }
                if item.label == r.label {
                    prev_idx = Some(idx);
                }
            }
            if let Some(prev_idx) = prev_idx {
                self.action_logs.items[prev_idx] = r;
            } else {
                self.action_logs.items.insert(0, r);
            }
        }

        let len = self.action_logs.items.len();
        let max_len = 2000;
        if len > max_len {
            let to_remove = len - max_len;
            for _ in 0..to_remove {
                self.action_logs.items.pop();
            }
        }

        let now_time = Instant::now();
        while let Ok(r) = self.file_change_receiver.try_recv() {
            self.recent_files.insert(r, now_time);
        }

        // 5 minutes
        let len = self.recent_files.len();
        let max_time = if len > 10 {
            Duration::from_secs(90)
        } else {
            Duration::from_secs(300)
        };
        self.recent_files
            .retain(|_, i| now_time.duration_since(*i) < max_time);

        while let Ok(r) = self.progress_receiver.try_recv() {
            r.lines()
                .for_each(|e| self.progress_logs.push(e.to_string()));
        }
        if self.progress_logs.len() > 20000 {
            let too_big = self.progress_logs.len() - 20000;
            self.progress_logs.drain(0..too_big);
        }
    }
}
