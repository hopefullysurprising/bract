pub mod form;
pub mod miller;

use std::any::Any;

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

pub trait View: 'static {
    fn render(&mut self, frame: &mut Frame);
    fn handle_key(&mut self, key: KeyEvent) -> Option<ViewAction>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Called once per event-loop tick (including idle ticks) so a view can
    /// integrate background work such as lazily-loaded children.
    fn on_idle(&mut self) {}
}
