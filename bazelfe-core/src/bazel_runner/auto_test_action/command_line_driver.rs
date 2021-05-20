use super::{app::App, ui};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    error::Error,
    io::stdout,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tui::{backend::CrosstermBackend, Terminal};

enum Event<I> {
    Input(I),
    Tick,
}

fn main_loop(
    progress_receiver: flume::Receiver<String>,
    rx: flume::Receiver<Event<KeyEvent>>,
) -> Result<(), Box<dyn Error>> {
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut app = App::new("BazelFE AutoTest Dashboard", true, progress_receiver);

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        match rx.recv()? {
            Event::Input(event) => match event.code {
                KeyCode::Char('q') => {
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        DisableMouseCapture
                    )?;
                    terminal.show_cursor()?;
                    return Ok(());
                }
                KeyCode::Char(c) => app.on_key(c),
                KeyCode::Left => app.on_left(),
                KeyCode::Up => app.on_up(),
                KeyCode::Right => app.on_right(),
                KeyCode::Tab => app.on_right(),
                KeyCode::BackTab => app.on_left(),
                KeyCode::Down => app.on_down(),
                _ => {}
            },
            Event::Tick => {
                app.on_tick();
            }
        }
        if app.should_quit {
            return Ok(());
        }
    }
}
pub fn main(
    progress_receiver: flume::Receiver<String>,
) -> Result<flume::Receiver<Result<(), String>>, Box<dyn Error>> {
    enable_raw_mode()?;

    // Setup input handling
    let (tx, rx) = flume::unbounded();

    let tick_rate = Duration::from_millis(250);
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            // poll for tick rate duration, if no events, sent tick event.
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if event::poll(timeout).unwrap() {
                if let CEvent::Key(key) = event::read().unwrap() {
                    tx.send(Event::Input(key)).unwrap();
                }
            }
            if last_tick.elapsed() >= tick_rate {
                tx.send(Event::Tick).unwrap();
                last_tick = Instant::now();
            }
        }
    });

    let (loop_dead_tx, loop_dead_rx) = flume::unbounded();

    thread::spawn(move || {
        let r = if let Err(e) = main_loop(progress_receiver, rx) {
            Err(format!("{:#?}", e))
        } else {
            Ok(())
        };
        let _ = loop_dead_tx.send(r);
    });
    Ok(loop_dead_rx)
}
