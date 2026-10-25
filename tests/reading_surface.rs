use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, ColorTheme, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn draw(w: &mut Workspace, width: u16, height: u16, message: &str) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, message)).unwrap();
    t.backend().buffer().clone()
}
fn text(b: &Buffer) -> String {
    b.content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .replace(' ', "")
}
fn click(w: &mut Workspace, b: &Buffer, glyph: &str) {
    let i = b.content.iter().position(|c| c.symbol() == glyph).unwrap();
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (i % b.area.width as usize) as u16,
        row: (i / b.area.width as usize) as u16,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}

#[test]
fn quiet_surface_exposes_commands_on_click_and_keeps_editing_after_resize_or_notice() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "夜航").unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("雨落在窗沿。".into())).unwrap();
    for (width, height) in [(32, 10), (80, 24), (120, 40), (160, 50)] {
        for message in ["", "操作失败，正文保留"] {
            let b = draw(&mut w, width, height, message);
            let visible = text(&b);
            for removed in ["F2", "F8", "F9", "F10", "布局[", "novelite"] {
                assert!(!visible.contains(removed), "unexpected chrome: {removed}");
            }
            assert!(visible.contains(message));
            assert!(visible.contains("命令"));
            assert!(
                visible.contains("Ctrl+P"),
                "visible keyboard entry required"
            );
            if width >= 80 {
                assert!(visible.contains("F1"));
            }
            click(&mut w, &b, "C"); // The shortcut hint is part of the same clickable entry.
            assert!(w.palette().is_some());
            w.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap();
            let b = draw(&mut w, width, height, message);
            click(&mut w, &b, "雨");
            w.dispatch(Action::Insert("新".into())).unwrap();
            assert_eq!(w.text(), "新雨落在窗沿。");
            w.dispatch(Action::Undo).unwrap();
        }
    }
    for (width, height) in [(1, 1), (4, 2), (16, 3)] {
        draw(&mut w, width, height, "异常");
    }
    w.dispatch(Action::Save).unwrap();
    assert_eq!(w.text(), "雨落在窗沿。");
}

#[test]
fn readonly_warning_survives_the_quiet_chrome() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book");
    let mut owner = Workspace::create_project(&book, "共享书稿").unwrap();
    owner.dispatch(Action::Insert("保留正文".into())).unwrap();
    owner.dispatch(Action::Save).unwrap();
    let mut reader = Workspace::open_project(&book).unwrap();
    let visible = text(&draw(&mut reader, 80, 24, ""));
    assert!(visible.contains("只读"));
    assert!(visible.contains("作品正在其他实例中打开"));
    assert!(visible.contains("保留正文"));
}
