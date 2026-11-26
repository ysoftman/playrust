mod app;

use std::io::{self, IsTerminal};
use std::path::Path;
use std::process::ExitCode;

use app::App;
use playrust::{LESSONS_DIR, load_lessons, run_code};
use ratatui::DefaultTerminal;
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    let mut cursor = SetCursorStyle::DefaultUserShape;
    while !app.quitting() {
        terminal.draw(|frame| app::ui(frame, app))?;
        // vim 처럼 Normal 은 블록, Insert 는 막대 커서
        if app.cursor_style() != cursor {
            cursor = app.cursor_style();
            execute!(terminal.backend_mut(), cursor)?;
        }
        // 실행은 "Running…" 을 그린 뒤에 한다
        if let Some(code) = app.pending_run() {
            let output = run_code(code);
            app.finish_run(output);
        } else if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let lessons = match load_lessons(Path::new(LESSONS_DIR)) {
        Ok(lessons) if !lessons.is_empty() => lessons,
        Ok(_) => {
            eprintln!("no lesson files in {LESSONS_DIR}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("failed to load lessons: {e}");
            return ExitCode::FAILURE;
        }
    };
    let start = match std::env::args().nth(1) {
        None => None,
        Some(arg) => match arg.parse::<usize>() {
            Ok(n) if (1..=lessons.len()).contains(&n) => Some(n - 1),
            _ => {
                eprintln!("usage: cargo run -- [lesson number 1-{}]", lessons.len());
                return ExitCode::FAILURE;
            }
        },
    };
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        eprintln!("playrust must run in a terminal (tty).");
        return ExitCode::FAILURE;
    }

    let mut app = App::new(lessons);
    app.start(start);
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    execute!(io::stdout(), SetCursorStyle::DefaultUserShape).ok();
    ratatui::restore();
    if let Err(e) = result {
        eprintln!("terminal error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
