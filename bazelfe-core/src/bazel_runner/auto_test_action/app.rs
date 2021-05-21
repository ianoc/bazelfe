use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use super::util::{RandomSignal, SinSignal, StatefulList, TabsState};

const TASKS: [&str; 24] = [
    "Item1", "Item2", "Item3", "Item4", "Item5", "Item6", "Item7", "Item8", "Item9", "Item10",
    "Item11", "Item12", "Item13", "Item14", "Item15", "Item16", "Item17", "Item18", "Item19",
    "Item20", "Item21", "Item22", "Item23", "Item24",
];

const LOGS: [(&str, &str); 26] = [
    ("Event1", "INFO"),
    ("Event2", "INFO"),
    ("Event3", "CRITICAL"),
    ("Event4", "ERROR"),
    ("Event5", "INFO"),
    ("Event6", "INFO"),
    ("Event7", "WARNING"),
    ("Event8", "INFO"),
    ("Event9", "INFO"),
    ("Event10", "INFO"),
    ("Event11", "CRITICAL"),
    ("Event12", "INFO"),
    ("Event13", "INFO"),
    ("Event14", "INFO"),
    ("Event15", "INFO"),
    ("Event16", "INFO"),
    ("Event17", "ERROR"),
    ("Event18", "ERROR"),
    ("Event19", "INFO"),
    ("Event20", "INFO"),
    ("Event21", "WARNING"),
    ("Event22", "INFO"),
    ("Event23", "INFO"),
    ("Event24", "WARNING"),
    ("Event25", "INFO"),
    ("Event26", "INFO"),
];

const EVENTS: [(&str, u64); 24] = [
    ("B1", 9),
    ("B2", 12),
    ("B3", 5),
    ("B4", 8),
    ("B5", 2),
    ("B6", 4),
    ("B7", 5),
    ("B8", 9),
    ("B9", 14),
    ("B10", 15),
    ("B11", 1),
    ("B12", 0),
    ("B13", 4),
    ("B14", 6),
    ("B15", 4),
    ("B16", 6),
    ("B17", 4),
    ("B18", 7),
    ("B19", 13),
    ("B20", 8),
    ("B21", 11),
    ("B22", 9),
    ("B23", 3),
    ("B24", 5),
];

const NEMONICS: [(&str, u64); 3] = [("Scalac", 9), ("Javac", 200), ("ijar", 4)];

pub struct Signal<S: Iterator> {
    source: S,
    pub points: Vec<S::Item>,
    tick_rate: usize,
}

impl<S> Signal<S>
where
    S: Iterator,
{
    fn on_tick(&mut self) {
        for _ in 0..self.tick_rate {
            self.points.remove(0);
        }
        self.points
            .extend(self.source.by_ref().take(self.tick_rate));
    }
}

pub struct Signals {
    pub sin1: Signal<SinSignal>,
    pub sin2: Signal<SinSignal>,
    pub window: [f64; 2],
}

impl Signals {
    fn on_tick(&mut self) {
        self.sin1.on_tick();
        self.sin2.on_tick();
        self.window[0] += 1.0;
        self.window[1] += 1.0;
    }
}

pub struct Server<'a> {
    pub name: &'a str,
    pub location: &'a str,
    pub coords: (f64, f64),
    pub status: &'a str,
}

pub struct App<'a> {
    pub title: &'a str,
    pub should_quit: bool,
    pub tabs: TabsState<'a>,
    pub show_chart: bool,
    pub progress: f64,
    pub sparkline: Signal<RandomSignal>,
    pub tasks: StatefulList<&'a str>,
    pub action_logs: StatefulList<super::ActionTargetStateScrollEntry>,
    pub progress_receiver: flume::Receiver<String>,
    pub file_change_receiver: flume::Receiver<PathBuf>,
    pub recent_files: HashMap<PathBuf, Instant>,
    pub bazel_status_rx: flume::Receiver<super::BazelStatus>,
    pub bazel_status: super::BazelStatus,
    pub build_status_rx: flume::Receiver<super::BuildStatus>,
    pub build_status: super::BuildStatus,
    pub progress_logs: Vec<String>,
    pub logs: StatefulList<(&'a str, &'a str)>,
    pub signals: Signals,
    pub barchart: Vec<(&'a str, u64)>,
    pub completed_actions: Vec<(&'a str, u64)>,
    pub nemonics: Vec<(&'a str, u64)>,
    pub servers: Vec<Server<'a>>,
    pub enhanced_graphics: bool,
    pub action_event_rx: flume::Receiver<super::ActionTargetStateScrollEntry>,
}

impl<'a> App<'a> {
    pub fn new(
        title: &'a str,
        enhanced_graphics: bool,
        progress_receiver: flume::Receiver<String>,
        file_change_receiver: flume::Receiver<PathBuf>,
        action_event_rx: flume::Receiver<super::ActionTargetStateScrollEntry>,
        bazel_status_rx: flume::Receiver<super::BazelStatus>,
        build_status_rx: flume::Receiver<super::BuildStatus>,
    ) -> App<'a> {
        let mut rand_signal = RandomSignal::new(0, 100);
        let sparkline_points = rand_signal.by_ref().take(300).collect();
        let mut sin_signal = SinSignal::new(0.2, 3.0, 18.0);
        let sin1_points = sin_signal.by_ref().take(100).collect();
        let mut sin_signal2 = SinSignal::new(0.1, 2.0, 10.0);
        let sin2_points = sin_signal2.by_ref().take(200).collect();
        App {
            title,
            should_quit: false,
            tabs: TabsState::new(vec!["Build Activity", "Bazel logs"]),
            show_chart: true,
            progress: 0.0,
            sparkline: Signal {
                source: rand_signal,
                points: sparkline_points,
                tick_rate: 1,
            },
            tasks: StatefulList::with_items(TASKS.to_vec()),
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
            logs: StatefulList::with_items(LOGS.to_vec()),
            signals: Signals {
                sin1: Signal {
                    source: sin_signal,
                    points: sin1_points,
                    tick_rate: 5,
                },
                sin2: Signal {
                    source: sin_signal2,
                    points: sin2_points,
                    tick_rate: 10,
                },
                window: [0.0, 20.0],
            },
            barchart: EVENTS.to_vec(),
            completed_actions: NEMONICS.to_vec(),
            nemonics: NEMONICS.to_vec(),
            servers: vec![
                Server {
                    name: "NorthAmerica-1",
                    location: "New York City",
                    coords: (40.71, -74.00),
                    status: "Up",
                },
                Server {
                    name: "Europe-1",
                    location: "Paris",
                    coords: (48.85, 2.35),
                    status: "Failure",
                },
                Server {
                    name: "SouthAmerica-1",
                    location: "São Paulo",
                    coords: (-23.54, -46.62),
                    status: "Up",
                },
                Server {
                    name: "Asia-1",
                    location: "Singapore",
                    coords: (1.35, 103.86),
                    status: "Up",
                },
            ],
            enhanced_graphics,
        }
    }

    pub fn on_up(&mut self) {
        self.tasks.previous();
    }

    pub fn on_down(&mut self) {
        self.tasks.next();
    }

    pub fn on_right(&mut self) {
        self.tabs.next();
    }

    pub fn on_left(&mut self) {
        self.tabs.previous();
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

        self.sparkline.on_tick();
        self.signals.on_tick();

        while let Ok(r) = self.bazel_status_rx.try_recv() {
            self.bazel_status = r;
        }

        while let Ok(r) = self.build_status_rx.try_recv() {
            self.build_status = r;
        }

        while let Ok(r) = self.action_event_rx.try_recv() {
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

        let log = self.logs.items.pop().unwrap();
        self.logs.items.insert(0, log);

        while let Ok(r) = self.progress_receiver.try_recv() {
            r.lines()
                .for_each(|e| self.progress_logs.push(e.to_string()));
        }
        if self.progress_logs.len() > 20000 {
            let too_big = self.progress_logs.len() - 20000;
            self.progress_logs.drain(0..too_big);
        }

        let event = self.barchart.pop().unwrap();
        self.barchart.insert(0, event);

        self.completed_actions.iter_mut().for_each(|(_, v)| {
            if rand::random::<bool>() {
                *v = *v + 1
            }
        });
        self.nemonics.iter_mut().for_each(|(_, v)| {
            if *v == 0 {
                *v = 50 as u64;
            }
            if rand::random::<bool>() {
                *v = *v + 1
            } else {
                *v = *v - 1
            }
        })
    }
}
