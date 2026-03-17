use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::event::{self as app_event, Action};
use crate::ui::View;

pub struct App {
    view_stack: Vec<Box<dyn View>>,
}

impl App {
    pub fn new(initial_view: Box<dyn View>) -> Self {
        Self {
            view_stack: vec![initial_view],
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|frame| {
                if let Some(view) = self.view_stack.last_mut() {
                    view.render(frame);
                }
            })?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some(Action::Quit) = app_event::map_key(key.code) {
                    if self.view_stack.len() > 1 {
                        self.view_stack.pop();
                    } else {
                        return Ok(());
                    }
                    continue;
                }
                if let Some(view) = self.view_stack.last_mut() {
                    view.handle_key(key.code);
                }
            }
        }
    }
}
