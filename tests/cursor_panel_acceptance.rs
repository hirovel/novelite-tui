use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::Workspace;
use ratatui::{Terminal, backend::TestBackend};

fn key(w: &mut Workspace, code: KeyCode, modifiers: KeyModifiers, kind: KeyEventKind) {
    w.handle_event(Event::Key(KeyEvent::new_with_kind(code, modifiers, kind)))
        .unwrap();
}
fn press(w: &mut Workspace, code: KeyCode) {
    key(w, code, KeyModifiers::NONE, KeyEventKind::Press);
}
fn draw(w: &mut Workspace, t: &mut Terminal<TestBackend>) {
    t.draw(|f| w.render(f, "")).unwrap();
}
fn fixture(text: &str) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.txt");
    std::fs::write(&path, text).unwrap();
    let mut w = Workspace::open(path).unwrap();
    w.configure_mouse(true);
    w.set_design_motion(false);
    (dir, w)
}

#[test]
fn drawer_round_trip_preserves_selection_and_the_vertical_column_goal() {
    for settings in [false, true] {
        let text = "0123456789\nx\nabcdefghij\nABCDEFGHIJ";
        let (_dir, mut w) = fixture(text);
        let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
        draw(&mut w, &mut t);
        for _ in 0..7 {
            press(&mut w, KeyCode::Right);
        }
        key(
            &mut w,
            KeyCode::Down,
            KeyModifiers::SHIFT,
            KeyEventKind::Press,
        );
        draw(&mut w, &mut t);
        let selection = w.selected_text().to_owned();
        let cursor = w.cursor();
        if settings {
            w.open_design_settings().unwrap();
        } else {
            press(&mut w, KeyCode::F(1));
        }
        draw(&mut w, &mut t);
        press(&mut w, KeyCode::Down);
        press(&mut w, KeyCode::Up);
        draw(&mut w, &mut t);
        assert_eq!(w.cursor(), cursor);
        assert_eq!(w.selected_text(), selection);
        press(&mut w, KeyCode::Esc);
        draw(&mut w, &mut t);
        assert!(!w.design_settings_open());
        assert!(w.palette().is_none());
        assert_eq!(w.selected_text(), selection);
        key(
            &mut w,
            KeyCode::Down,
            KeyModifiers::SHIFT,
            KeyEventKind::Press,
        );
        draw(&mut w, &mut t);
        assert_eq!(
            w.cursor(),
            20,
            "drawer must not reset the column remembered on a short line"
        );
        key(
            &mut w,
            KeyCode::Up,
            KeyModifiers::SHIFT,
            KeyEventKind::Press,
        );
        key(
            &mut w,
            KeyCode::Up,
            KeyModifiers::SHIFT,
            KeyEventKind::Press,
        );
        draw(&mut w, &mut t);
        assert_eq!(w.cursor(), 7);
        assert_eq!(w.selected_text(), "");
        press(&mut w, KeyCode::Char('/'));
        assert_eq!(w.text(), "0123456/789\nx\nabcdefghij\nABCDEFGHIJ");
    }
}

#[test]
fn repeated_vertical_keys_advance_once_and_release_never_moves_or_opens_a_drawer() {
    let (_dir, mut w) = fixture("0123456789\nx\nabcdefghij\nABCDEFGHIJ");
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    draw(&mut w, &mut t);
    for _ in 0..7 {
        press(&mut w, KeyCode::Right);
    }
    for (kind, expected) in [
        (KeyEventKind::Press, 12),
        (KeyEventKind::Release, 12),
        (KeyEventKind::Repeat, 20),
        (KeyEventKind::Release, 20),
    ] {
        key(&mut w, KeyCode::Down, KeyModifiers::NONE, kind);
        draw(&mut w, &mut t);
        assert_eq!(w.cursor(), expected, "{kind:?}");
    }
    key(
        &mut w,
        KeyCode::F(1),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert!(w.palette().is_none());
    key(
        &mut w,
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
        KeyEventKind::Release,
    );
    assert!(w.palette().is_none());
    key(
        &mut w,
        KeyCode::Up,
        KeyModifiers::SHIFT,
        KeyEventKind::Repeat,
    );
    assert_eq!(w.cursor(), 12);
    let selection = w.selected_text().to_owned();
    key(
        &mut w,
        KeyCode::Up,
        KeyModifiers::SHIFT,
        KeyEventKind::Release,
    );
    assert_eq!(w.selected_text(), selection);
}

#[test]
fn clicking_the_rendered_body_after_closing_settings_replaces_the_old_vertical_goal() {
    let text = "0123456789\nx\nabcdefghij\nABCDEFGHIJ";
    let (_dir, mut w) = fixture(text);
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    draw(&mut w, &mut t);
    for _ in 0..7 {
        press(&mut w, KeyCode::Right);
    }
    press(&mut w, KeyCode::Down);
    w.open_design_settings().unwrap();
    draw(&mut w, &mut t);
    press(&mut w, KeyCode::Esc);
    draw(&mut w, &mut t);
    let buffer = t.backend().buffer();
    let index = buffer
        .content
        .iter()
        .position(|cell| cell.symbol() == "c")
        .unwrap();
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (index % 100) as u16,
        row: (index / 100) as u16,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: (index % 100) as u16,
        row: (index / 100) as u16,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert_eq!(w.cursor(), 15);
    press(&mut w, KeyCode::Down);
    draw(&mut w, &mut t);
    assert_eq!(
        w.cursor(),
        26,
        "clicking column 2 replaces the earlier column 7 goal"
    );
    assert_eq!(w.text(), text);
}
