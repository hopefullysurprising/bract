use ratatui::crossterm::event::KeyCode;

pub enum Action {
    Quit,
}

pub fn map_key(code: KeyCode) -> Option<Action> {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => Some(Action::Quit),
        _ => None,
    }
}
