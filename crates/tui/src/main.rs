mod app;
mod data;
mod event;
mod ui;

use data::source;
use data::source::mise_tasks::MiseTasksSource;
use ui::browse::BrowseView;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sources: Vec<Box<dyn source::Source>> = vec![Box::new(MiseTasksSource)];
    let tools = source::assemble_tools(sources)?;
    let browse = BrowseView::new(&tools)?;

    let terminal = ratatui::init();
    let result = app::App::new(Box::new(browse)).run(terminal);
    ratatui::restore();
    result
}
