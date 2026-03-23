mod app;
mod data;
mod event;
mod ui;

use std::process::Command;

use app::AppResult;
use data::source;
use data::source::mise_tasks::MiseTasksSource;
use data::source::mise_tools;
use ui::browse::BrowseView;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sources: Vec<Box<dyn source::Source>> = vec![Box::new(MiseTasksSource)];
    sources.extend(mise_tools::discover_sources());
    let tools = source::assemble_tools(sources)?;
    let browse = BrowseView::new(&tools)?;

    let terminal = ratatui::init();
    let result = app::App::new(Box::new(browse)).run(terminal);
    ratatui::restore();

    match result? {
        AppResult::Exit => {}
        AppResult::Run(spec) => {
            let all_args: Vec<&str> = spec.bin.iter().skip(1)
                .map(|s| s.as_str())
                .chain(spec.args.iter().map(|s| s.as_str()))
                .collect();

            eprintln!("→ {} {}", spec.bin[0], all_args.join(" "));

            let status = Command::new(&spec.bin[0])
                .args(&all_args)
                .status()?;

            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
}
