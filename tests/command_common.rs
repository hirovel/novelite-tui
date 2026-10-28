use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, Command, Workspace};
use ratatui::{Terminal, backend::TestBackend};
fn press(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}

#[test]
fn common_commands_are_compact_and_search_finds_registered_and_advanced_commands() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.register_command(Command {
        id: "custom-heading".into(),
        title: "插入标题".into(),
        run: |w| w.dispatch(Action::Insert("# ".into())),
    })
    .unwrap();
    press(&mut w, KeyCode::F(1));
    let common = w.palette().unwrap();
    assert_eq!(
        common
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        [
            "open-project",
            "save",
            "find",
            "design-settings",
            "recent-projects",
            "save-as",
            "chapters",
            "quit",
            "browse-commands"
        ]
    );
    let common_count = common.entries.len();
    w.handle_event(Event::Paste("custom-heading".into()))
        .unwrap();
    assert_eq!(w.palette().unwrap().entries[0].id, "custom-heading");
    press(&mut w, KeyCode::Enter);
    assert_eq!(w.text(), "# ");
    press(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("reset-keys".into())).unwrap();
    assert_eq!(w.palette().unwrap().entries[0].id, "reset-keys");
    for _ in "reset-keys".chars() {
        press(&mut w, KeyCode::Backspace);
    }
    assert_eq!(w.palette().unwrap().entries.len(), common_count);
}

#[test]
fn browse_all_is_reachable_by_keyboard_and_reopening_returns_to_common() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    press(&mut w, KeyCode::F(1));
    for _ in 0..8 {
        press(&mut w, KeyCode::Down);
    }
    assert_eq!(
        w.palette().unwrap().entries[w.palette().unwrap().selected].id,
        "browse-commands"
    );
    press(&mut w, KeyCode::Enter);
    let all = w.palette().unwrap();
    assert!(all.entries.len() > 9);
    assert!(all.entries.iter().any(|e| e.id == "reset-keys"));
    assert!(all.entries.iter().all(|e| e.id != "browse-commands"));
    press(&mut w, KeyCode::Esc);
    press(&mut w, KeyCode::F(1));
    assert_eq!(w.palette().unwrap().entries.len(), 9);
    assert_eq!(w.palette().unwrap().selected, 0);
    press(&mut w, KeyCode::Esc);
    press(&mut w, KeyCode::Char('/'));
    assert_eq!(w.text(), "/");
}

#[test]
fn browse_all_uses_ordinary_rendered_mouse_targets_including_small_terminals() {
    for (width, height) in [(32, 10), (80, 24), (120, 40)] {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
        w.configure_mouse(true);
        w.set_design_motion(false);
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        press(&mut w, KeyCode::F(1));
        for _ in 0..8 {
            press(&mut w, KeyCode::Down);
        }
        t.draw(|f| w.render(f, "")).unwrap();
        let selected_row = t
            .backend()
            .buffer()
            .content
            .iter()
            .position(|c| c.symbol() == "▎")
            .unwrap()
            / width as usize;
        let i = t
            .backend()
            .buffer()
            .content
            .iter()
            .enumerate()
            .find(|(i, c)| *i / width as usize == selected_row && c.symbol() == "浏")
            .map(|(i, _)| i)
            .unwrap();
        w.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: (i % width as usize) as u16,
            row: (i / width as usize) as u16,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
        assert!(w.palette().unwrap().entries.len() > 9);
        assert_eq!(w.text(), "");
    }
}
