use crate::{Action, Direction};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};

pub(crate) fn decode(event: Event) -> Option<Action> {
    if let Event::Paste(text) = event {
        return Some(Action::Insert(text));
    }
    let Event::Key(key) = event else {
        return None;
    };
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
    {
        return None;
    }
    let action = match key.code {
        KeyCode::Char(ch) => Some(Action::Insert(ch.to_string())),
        KeyCode::Enter => Some(Action::Newline),
        KeyCode::Tab => Some(Action::Insert("\t".into())),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Delete => Some(Action::Delete),
        KeyCode::Left => Some(Action::Move(Direction::Left)),
        KeyCode::Right => Some(Action::Move(Direction::Right)),
        KeyCode::Up => Some(Action::Move(Direction::Up)),
        KeyCode::Down => Some(Action::Move(Direction::Down)),
        KeyCode::Home => Some(Action::Move(Direction::Home)),
        KeyCode::End => Some(Action::Move(Direction::End)),

        _ => None,
    };
    action.map(|action| match action {
        Action::Move(direction) if key.modifiers.contains(KeyModifiers::SHIFT) => {
            Action::Select(direction)
        }
        action => action,
    })
}
