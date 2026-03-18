mod app;
mod data;
mod event;
mod ui;

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
    result
}
