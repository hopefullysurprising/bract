use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::event::{self as app_event, Action};
use crate::ui::{RunSpec, View, ViewAction};

pub struct App {
    view_stack: Vec<Box<dyn View>>,
}

pub enum AppResult {
    Exit,
    Run(RunSpec),
}

impl App {
    pub fn new(initial_view: Box<dyn View>) -> Self {
        Self {
            view_stack: vec![initial_view],
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<AppResult, Box<dyn std::error::Error>> {
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

                if let Some(view) = self.view_stack.last_mut() {
                    if let Some(action) = view.handle_key(key) {
                        match action {
                            ViewAction::Push(new_view) => {
                                self.view_stack.push(new_view);
                            }
                            ViewAction::Run(spec) => {
                                return Ok(AppResult::Run(spec));
                            }
                            ViewAction::Consumed => {}
                        }
                        continue;
                    }
                }

                if let Some(Action::Quit) = app_event::map_key(key.code) {
                    if self.view_stack.len() > 1 {
                        self.view_stack.pop();
                    } else {
                        return Ok(AppResult::Exit);
                    }
                }
            }
        }
    }
}
