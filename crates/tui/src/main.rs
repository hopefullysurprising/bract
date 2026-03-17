mod app;
mod data;
mod event;
mod ui;

use data::commands;
use ui::browse::BrowseView;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tools = commands::sample_tools();
    let browse = BrowseView::new(&tools)?;

    let terminal = ratatui::init();
    let result = app::App::new(browse).run(terminal);
    ratatui::restore();
    result
}
