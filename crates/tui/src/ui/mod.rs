pub mod browse;
pub mod form;

use ratatui::crossterm::event::KeyEvent;
use ratatui::Frame;

pub struct RunSpec {
    pub bin: Vec<String>,
    pub args: Vec<String>,
}

pub enum ViewAction {
    Push(Box<dyn View>),
    Run(RunSpec),
    Consumed,
}

pub trait View {
    fn render(&mut self, frame: &mut Frame);
    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction>;
}
