pub mod app;
mod clipboard;
pub mod data;
pub mod event;
mod shell_quote;
pub mod ui;

use std::io::IsTerminal;
use std::process::Command;
use std::time::Duration;

use ratatui::backend::Backend;
use ratatui::crossterm::event as term_event;
use ratatui::Terminal;

use app::{App, AppResult};
use data::source;
use ui::miller::MillerView;

const TICK: Duration = Duration::from_millis(80);

pub fn run_main() -> Result<(), Box<dyn std::error::Error>> {
    // bract is a full-screen TUI; it must own an interactive terminal. Bail with a
    // clear hint rather than rendering into a void if stdout/stdin isn't a TTY.
    if let Some(hint) = terminal_unavailable(std::io::stdout().is_terminal(), std::io::stdin().is_terminal()) {
        eprintln!("{hint}");
        std::process::exit(1);
    }

    let sources = source::discover_sources();
    let mut app = App::new(Box::new(MillerView::new(sources)));

    let mut terminal = ratatui::init();
    let result = run_loop(&mut app, &mut terminal);
    ratatui::restore();

    match result? {
        AppResult::Exit => {}
        AppResult::Run(spec) => {
            // The exact, re-runnable command line (properly shell-quoted) — shown,
            // copied to the clipboard, and what `args` below reproduce via argv.
            let mut tokens = spec.bin.clone();
            tokens.extend(spec.args.iter().cloned());
            let line = shell_quote::quote_command(&tokens);

            eprintln!("→ {line}");
            match clipboard::copy_command(&line) {
                clipboard::CopyOutcome::Confirmed => {
                    eprintln!("  copied to clipboard — paste to run it again");
                }
                clipboard::CopyOutcome::BestEffort => {
                    eprintln!("  sent to clipboard via OSC 52 — paste to run it again (if your terminal allows it)");
                }
                clipboard::CopyOutcome::Disabled => {}
            }

            let all_args: Vec<&str> = spec.bin.iter().skip(1)
                .map(|s| s.as_str())
                .chain(spec.args.iter().map(|s| s.as_str()))
                .collect();

            let status = Command::new(&spec.bin[0]).args(&all_args).status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
}

/// Returns a hint to print (and exit on) when bract isn't attached to an
/// interactive terminal. The common cause is a `mise` task that captures output
/// for prefixing — such tasks must be marked `raw = true`.
fn terminal_unavailable(stdout_tty: bool, stdin_tty: bool) -> Option<String> {
    if stdout_tty && stdin_tty {
        return None;
    }
    Some(
        "bract needs an interactive terminal.\n\
         If you're running it as a mise task, mark the task `raw = true` \
         (mise captures task output otherwise)."
            .to_string(),
    )
}

/// The event loop polls rather than blocks, so background-loaded columns and the
/// loading spinner keep refreshing while waiting for input.
fn run_loop<B>(app: &mut App, terminal: &mut Terminal<B>) -> Result<AppResult, Box<dyn std::error::Error>>
where
    B: Backend,
    B::Error: std::error::Error + 'static,
{
    loop {
        app.render(terminal).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        if term_event::poll(TICK)? {
            let ev = term_event::read()?;
            if let Some(result) = app.tick(ev) {
                return Ok(result);
            }
        } else {
            // No input this tick — advance background loads and the spinner.
            app.on_idle();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::terminal_unavailable;

    #[test]
    fn a_real_terminal_is_allowed() {
        assert!(terminal_unavailable(true, true).is_none());
    }

    #[test]
    fn captured_output_bails_with_a_raw_hint() {
        // e.g. a mise task without `raw = true` pipes stdout for prefixing.
        let hint = terminal_unavailable(false, true).expect("should bail without a tty");
        assert!(hint.contains("raw = true"), "hint names the fix: {hint:?}");
        assert!(hint.to_lowercase().contains("terminal"), "hint mentions the terminal: {hint:?}");
    }
}
