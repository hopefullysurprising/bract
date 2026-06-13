use ratatui::backend::Backend;
use ratatui::crossterm::event::{Event, KeyEventKind};
use ratatui::Terminal;

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

    pub fn current_view_mut(&mut self) -> Option<&mut dyn View> {
        self.view_stack.last_mut().map(|v| v.as_mut())
    }

    pub fn render<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<(), B::Error> {
        terminal.draw(|frame| {
            if let Some(view) = self.view_stack.last_mut() {
                view.render(frame);
            }
        })?;
        Ok(())
    }

    pub fn tick(&mut self, event: Event) -> Option<AppResult> {
        let Event::Key(key) = event else {
            return None;
        };
        if key.kind != KeyEventKind::Press {
            return None;
        }

        if let Some(view) = self.view_stack.last_mut()
            && let Some(action) = view.handle_key(key) {
                match action {
                    ViewAction::Push(new_view) => {
                        self.view_stack.push(new_view);
                        return None;
                    }
                    ViewAction::Run(spec) => return Some(AppResult::Run(spec)),
                    ViewAction::Consumed => return None,
                }
            }

        if let Some(Action::Quit) = app_event::map_key(key.code) {
            if self.view_stack.len() > 1 {
                self.view_stack.pop();
                None
            } else {
                Some(AppResult::Exit)
            }
        } else {
            None
        }
    }

    /// Advance background work on the current view (lazy loads, spinners).
    pub fn on_idle(&mut self) {
        if let Some(view) = self.view_stack.last_mut() {
            view.on_idle();
        }
    }
}
