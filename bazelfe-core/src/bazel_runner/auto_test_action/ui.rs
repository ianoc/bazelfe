use super::{app::App, ctrl_char::CtrlChars};
use std::time::{Duration, Instant};

use tui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Span, Spans, Text},
    widgets::canvas::{Canvas, Line, Map, MapResolution, Rectangle},
    widgets::{
        Axis, BarChart, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph, Row,
        Table, Tabs, Wrap,
    },
    Frame,
};

pub fn draw<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(f.size());
    let titles = app
        .tabs
        .titles
        .iter()
        .map(|t| Spans::from(Span::styled(*t, Style::default().fg(Color::Green))))
        .collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(app.title))
        .highlight_style(Style::default().fg(Color::Yellow))
        .select(app.tabs.index);
    f.render_widget(tabs, chunks[0]);
    match app.tabs.index {
        0 => draw_first_tab(f, app, chunks[1]),
        1 => draw_second_tab(f, app, chunks[1]),
        _ => {}
    };
}

fn draw_first_tab<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where
    B: Backend,
{
    let chunks = Layout::default()
        .constraints([Constraint::Min(7), Constraint::Min(7), Constraint::Min(7)].as_ref())
        .split(area);
    draw_system_status(f, app, chunks[0]);
    draw_recent_file_changes(f, app, chunks[1]);
    draw_text(f, app, chunks[2]);
}

fn draw_system_status<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where
    B: Backend,
{
    let bazel_status_span = match app.bazel_status {
        super::BazelStatus::Idle => Span::styled("Idle", Style::default().bg(Color::LightBlue)),
        super::BazelStatus::Build => Span::styled("Build", Style::default().bg(Color::LightGreen)),
        super::BazelStatus::Test => Span::styled("Test", Style::default().bg(Color::LightYellow)),
        super::BazelStatus::InQuery => Span::styled(
            "System querying Dependencies",
            Style::default().bg(Color::LightMagenta),
        ),
    };

    let build_status_span = match app.build_status {
        super::BuildStatus::ActionsFailing => {
            Span::styled("Failing", Style::default().bg(Color::LightRed))
        }
        super::BuildStatus::ActionsGreen => {
            Span::styled("Success", Style::default().bg(Color::LightGreen))
        }
    };
    let text: Vec<Spans> = vec![
        Spans(vec![Span::raw("Bazel status: "), bazel_status_span]),
        Spans(vec![Span::raw("Build status: "), build_status_span]),
    ];
    let system_status = Paragraph::new(Text { lines: text })
        .block(
            Block::default()
                .title("System status")
                .borders(Borders::ALL),
        )
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    f.render_widget(system_status, area);
}

fn draw_recent_file_changes<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where
    B: Backend,
{
    use humantime::format_duration;

    let time_style = Style::default().fg(Color::Blue);
    let now_time = Instant::now();
    let logs: Vec<ListItem> = app
        .recent_files
        .iter()
        .map(|(pb, when)| {
            let mut elapsed = now_time.duration_since(*when);
            elapsed = elapsed
                .checked_sub(Duration::from_nanos(elapsed.subsec_nanos() as u64))
                .unwrap_or(elapsed);
            let content = vec![Spans::from(vec![
                Span::styled(
                    format!(
                        "{:<14}",
                        format!("{} ago", format_duration(elapsed).to_string())
                    ),
                    time_style,
                ),
                Span::raw(pb.to_string_lossy()),
            ])];
            ListItem::new(content)
        })
        .collect();
    let logs = List::new(logs).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Recently changed files"),
    );
    f.render_stateful_widget(logs, area, &mut app.action_logs.state);
}

fn draw_text<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where
    B: Backend,
{
    use humantime::format_duration;

    let action_style = Style::default().fg(Color::Blue);
    let target_style = Style::default().fg(Color::Yellow);
    let test_style = Style::default().fg(Color::Magenta);
    let time_style = Style::default().fg(Color::Blue);

    let now_time = Instant::now();
    let success_span = Span::styled(
        format!("{:<11}", "SUCCESS"),
        Style::default().fg(Color::Green),
    );
    let failed_span = Span::styled(format!("{:<11}", "FAILED"), Style::default().fg(Color::Red));
    let logs: Vec<ListItem> = app
        .action_logs
        .items
        .iter()
        .map(|action_entry| {
            let s = match action_entry.complete_type {
                super::CompleteKind::Action => action_style,
                super::CompleteKind::Target => target_style,
                super::CompleteKind::Test => test_style,
            };

            let lvl_str = match action_entry.complete_type {
                super::CompleteKind::Action => "ACTION",
                super::CompleteKind::Target => "TARGET",
                super::CompleteKind::Test => "TEST",
            };

            let mid_span = if action_entry.success {
                &success_span
            } else {
                &failed_span
            };
            let mut elapsed = now_time.duration_since(*&action_entry.when);
            elapsed = elapsed
                .checked_sub(Duration::from_nanos(elapsed.subsec_nanos() as u64))
                .unwrap_or(elapsed);
            let content = vec![Spans::from(vec![
                Span::styled(
                    format!(
                        "{:<14}",
                        format!("{} ago", format_duration(elapsed).to_string())
                    ),
                    time_style,
                ),
                Span::styled(format!("{:<9}", lvl_str), s),
                mid_span.clone(),
                Span::raw(action_entry.label.clone()),
            ])];
            ListItem::new(content)
        })
        .collect();
    let logs = List::new(logs).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Completion events"),
    );
    f.render_stateful_widget(logs, area, &mut app.action_logs.state);
}

fn draw_second_tab<B>(f: &mut Frame<B>, app: &mut App, area: Rect)
where
    B: Backend,
{
    let text: Vec<Spans> = app
        .progress_logs
        .iter()
        .map(|e| Spans(CtrlChars::parse(e.to_string()).into_text()))
        .collect();
    let paragraph = Paragraph::new(Text { lines: text })
        .block(Block::default().title("Bazel logs").borders(Borders::ALL))
        .style(Style::default().fg(Color::White).bg(Color::Black))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}
