pub mod browse;

use ratatui::crossterm::event::KeyCode;
use ratatui::Frame;

pub trait View {
    fn render(&mut self, frame: &mut Frame);
    fn handle_key(&mut self, code: KeyCode);
}
