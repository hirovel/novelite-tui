use novelite_tui::{Action, ColorMode, Density, TextWidth, Workspace};
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};

fn assert_terminal_palette(work: &mut Workspace, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| work.render(frame, "")).unwrap();
    for cell in &terminal.backend().buffer().content {
        assert_eq!(cell.bg, Color::Reset);
        assert!(
            !matches!(cell.fg, Color::Rgb(..)),
            "terminal palette must own named accent colors"
        );
    }
}

#[test]
fn fresh_workspace_uses_terminal_colors_in_editor_and_responsive_panels() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.configure_color_support(ColorMode::TrueColor);
    work.dispatch(Action::Insert("中文/正文".into())).unwrap();
    for (width, height) in [(150, 42), (80, 24), (32, 10), (1, 1)] {
        assert_terminal_palette(&mut work, width, height);
        work.open_design_settings().unwrap();
        assert_terminal_palette(&mut work, width, height);
        work.execute_command("design-commands").unwrap();
        assert_terminal_palette(&mut work, width, height);
        work.handle_event(crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Esc,
                crossterm::event::KeyModifiers::NONE,
            ),
        ))
        .unwrap();
    }
    assert_eq!(work.text(), "中文/正文");
}

#[test]
fn restoring_terminal_preferences_is_persistent_and_preserves_layout_cursor_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let config = dir.path().join("layout.json");
    let mut work = Workspace::open(&path).unwrap();
    work.configure_color_support(ColorMode::TrueColor);
    work.load_layout(&config).unwrap();
    work.set_text_width(TextWidth::Full).unwrap();
    work.set_density(Density::Compact).unwrap();
    work.dispatch(Action::Insert("保留稿件".into())).unwrap();
    let cursor = work.cursor();
    let rect = Rect::new(0, 0, 120, 40);
    let area = work.editor_area(rect);
    work.execute_command("theme-paper").unwrap();
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    t.draw(|f| work.render(f, "")).unwrap();
    assert!(matches!(t.backend().buffer()[(0, 0)].bg, Color::Rgb(..)));
    work.execute_command("terminal-defaults").unwrap();
    assert_terminal_palette(&mut work, 120, 40);
    assert_eq!(work.cursor(), cursor);
    assert_eq!(work.editor_area(rect), area);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "");
    work.dispatch(Action::Redo).unwrap();
    work.dispatch(Action::Save).unwrap();
    drop(work);
    let mut reopened = Workspace::open(&path).unwrap();
    reopened.configure_color_support(ColorMode::TrueColor);
    reopened.load_layout(&config).unwrap();
    assert_terminal_palette(&mut reopened, 120, 40);
    assert_eq!(reopened.text(), "保留稿件");
    assert_eq!(reopened.editor_area(rect), area);
}
