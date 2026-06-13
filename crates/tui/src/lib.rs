pub mod app;
pub mod data;
pub mod event;
pub mod ui;

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
    let sources = source::discover_sources();
    let mut app = App::new(Box::new(MillerView::new(sources)));

    let mut terminal = ratatui::init();
    let result = run_loop(&mut app, &mut terminal);
    ratatui::restore();

    match result? {
        AppResult::Exit => {}
        AppResult::Run(spec) => {
            let all_args: Vec<&str> = spec.bin.iter().skip(1)
                .map(|s| s.as_str())
                .chain(spec.args.iter().map(|s| s.as_str()))
                .collect();

            eprintln!("→ {} {}", spec.bin[0], all_args.join(" "));

            let status = Command::new(&spec.bin[0]).args(&all_args).status()?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
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
