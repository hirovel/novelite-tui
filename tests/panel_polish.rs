use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Command, Material, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn row(b: &Buffer, y: u16) -> String {
    (0..b.area.width)
        .map(|x| b[(x, y)].symbol())
        .collect::<String>()
        .replace(' ', "")
}
#[test]
fn command_status_describes_visible_results_and_never_offers_execution_for_no_matches() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.dispatch(Action::Insert("正文/".into())).unwrap();
    key(&mut w, KeyCode::F(1));
    let first = draw(&mut w, 80, 24);
    assert!(row(&first, 22).contains("1–"));
    for _ in 0..15 {
        key(&mut w, KeyCode::Down);
        draw(&mut w, 80, 24);
    }
    assert!(!row(&draw(&mut w, 80, 24), 22).starts_with("1–"));
    w.handle_event(Event::Paste("不存在的操作xyz".into()))
        .unwrap();
    let empty = draw(&mut w, 80, 24);
    assert!(row(&empty, 22).contains("换个关键词"));
    assert!(!row(&empty, 22).contains("Enter"));
    key(&mut w, KeyCode::Enter);
    assert!(w.palette().is_some());
    assert_eq!(w.text(), "正文/");
}
#[test]
fn settings_show_only_the_focused_description_and_a_quiet_terminal_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.set_material(Material::Terminal).unwrap();
    w.open_design_settings().unwrap();
    let b = draw(&mut w, 120, 40);
    assert!(row(&b, 38).contains("阅读随窗口调整"));
    assert!((24..38).any(|y| row(&b, y).contains("字体与光标外观沿用终端")));
    for y in 24..38 {
        assert!(!row(&b, y).contains("不改动段落内容"));
    }
    key(&mut w, KeyCode::Down);
    let next = draw(&mut w, 120, 40);
    assert!(row(&next, 38).contains("不改动段落内容"));
    assert!(!row(&next, 38).contains("即时生效"));
    assert_eq!(w.text(), "");
}
#[test]
fn a_long_unicode_command_title_has_a_visible_truncation_marker() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.register_command(Command {
        id: "long-label".into(),
        title: "很长的中文标题👩‍💻e\u{301}".repeat(8),
        run: |_| Ok(novelite_tui::Outcome::Continue),
    })
    .unwrap();
    key(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("long-label".into())).unwrap();
    let b = draw(&mut w, 32, 10);
    let y = (0..10).find(|&y| row(&b, y).contains('▎')).unwrap();
    assert!(row(&b, y).ends_with('…'));
    assert!(!row(&b, y).contains('�'));
    assert_eq!(w.text(), "");
}

#[test]
fn settings_footer_keeps_complete_navigation_keys_when_narrowed() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.dispatch(Action::Insert("正文不变".into())).unwrap();
    w.open_design_settings().unwrap();
    for width in [32, 40, 50, 60, 80, 120] {
        let buffer = draw(&mut w, width, 24);
        let footer = row(&buffer, 23);
        assert!(footer.contains("命令"), "{width}: {footer}");
        for shortcut in ["Tab", "←→", "Enter"] {
            assert!(
                footer.contains(shortcut),
                "{width}: missing {shortcut} in {footer}"
            );
        }
        assert!(!footer.contains('…'), "{width}: {footer}");
    }
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), "正文不变");
}
