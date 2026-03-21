pub mod browse;
pub mod form;

use ratatui::crossterm::event::KeyEvent;
use ratatui::Frame;

pub enum ViewAction {
    Push(Box<dyn View>),
    Consumed,
}

pub trait View {
    fn render(&mut self, frame: &mut Frame);
    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction>;
}
