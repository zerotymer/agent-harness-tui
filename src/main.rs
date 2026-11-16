//! Entry point: runs the wizard in the current directory (the project root).

use agent_harness_tui::paths::ProjectRoot;
use agent_harness_tui::tui::App;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use std::io::IsTerminal;
use std::process::ExitCode;

const USAGE: &str = "usage: agent-harness-tui [--version | --help]\n\
Run inside the project root. It never searches parent directories, \
initializes Git, commits or pushes.";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some("--version" | "-V") => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("unknown argument `{other}`\n{USAGE}");
            return ExitCode::from(2);
        }
    }
    // Fail fast instead of waiting when there is no interactive terminal.
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!(
            "agent-harness-tui: an interactive terminal is required (stdin and stdout must be a TTY)"
        );
        return ExitCode::from(2);
    }
    let root = match ProjectRoot::from_cwd() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("agent-harness-tui: cannot use the current directory: {e}");
            return ExitCode::from(1);
        }
    };
    let mut app = App::new(root);
    // Raw mode, alternate screen, and a panic hook that restores the terminal.
    let mut terminal = ratatui::init();
    let result = (|| -> std::io::Result<()> {
        while !app.quit {
            terminal.draw(|f| app.render(f))?;
            // Resize events simply trigger the next redraw.
            if let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press
            {
                app.handle_key(k);
            }
        }
        Ok(())
    })();
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("agent-harness-tui: terminal error: {e}");
            ExitCode::from(1)
        }
    }
}
