use novelite_tui::{Action, Direction, ModuleId, VisualStyle, Workspace};
use ratatui::layout::Rect;

#[test]
fn style_is_persistent_and_independent_of_layout_focus_selection_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let config = dir.path().join("workspace.json");
    let mut work = Workspace::create_project(&root, "风格").unwrap();
    work.load_layout(&config).unwrap();
    assert_eq!(work.visual_style(), VisualStyle::Reading);
    work.dispatch(Action::Insert("春风又绿江南岸".into()))
        .unwrap();
    work.dispatch(Action::Select(Direction::Left)).unwrap();
    work.execute_command("pin-chapters").unwrap();
    work.execute_command("chapters-right").unwrap();
    work.execute_command("chapters").unwrap();
    let panel = work.panel_preferences(ModuleId::Chapters);
    for (command, style) in [
        ("style-terminal", VisualStyle::Terminal),
        ("style-compact", VisualStyle::Compact),
        ("style-reading", VisualStyle::Reading),
    ] {
        work.execute_command(command).unwrap();
        assert_eq!(work.visual_style(), style);
        assert_eq!(work.selected_text(), "岸");
        assert!(work.module_focused(ModuleId::Chapters));
        assert_eq!(work.panel_preferences(ModuleId::Chapters), panel);
        assert_eq!(work.text(), "春风又绿江南岸");
    }
    work.execute_command("style-compact").unwrap();
    work.execute_command("focus-editor").unwrap();
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "");
    work.dispatch(Action::Redo).unwrap();
    work.dispatch(Action::Save).unwrap();
    drop(work);
    let mut reopened = Workspace::open_project(&root).unwrap();
    reopened.load_layout(config).unwrap();
    assert_eq!(reopened.visual_style(), VisualStyle::Compact);
    assert_eq!(reopened.panel_preferences(ModuleId::Chapters), panel);
    assert_eq!(
        reopened.editor_area(Rect::new(0, 0, 120, 30)),
        Rect::new(0, 0, 120, 30)
    );
    assert_eq!(reopened.text(), "春风又绿江南岸");
}

#[test]
fn monochrome_and_light_dark_rendering_keep_status_and_escape_routes_visible() {
    use novelite_tui::{Background, ColorMode};
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    let dir = tempfile::tempdir().unwrap();
    let mut work =
        Workspace::create_project(dir.path().join("book"), &"很长的书名".repeat(40)).unwrap();
    work.dispatch(Action::Insert("中文 👩‍💻 é\n第二段".into()))
        .unwrap();
    work.set_color_mode(ColorMode::Monochrome).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(35, 10)).unwrap();
    terminal
        .draw(|frame| work.render(frame, &"写入失败，请重试".repeat(30)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = visible_text(buffer);
    assert!(text.contains("未保存"));
    assert!(text.contains("命令"));
    assert!(
        buffer
            .content
            .iter()
            .all(|c| c.fg == Color::Reset && c.bg == Color::Reset)
    );
    for background in [Background::Light, Background::Dark] {
        work.set_background(background).unwrap();
        work.set_color_mode(ColorMode::Basic).unwrap();
        terminal.draw(|frame| work.render(frame, "")).unwrap();
        let cell = &terminal.backend().buffer()[(0, 0)];
        assert_ne!(cell.fg, cell.bg);
    }
    let mut tiny = Terminal::new(TestBackend::new(24, 1)).unwrap();
    tiny.draw(|frame| work.render(frame, "错误")).unwrap();
    let text = visible_text(tiny.backend().buffer());
    assert!(text.contains("命令"));
}

fn visible_text(buffer: &ratatui::buffer::Buffer) -> String {
    use unicode_width::UnicodeWidthStr;
    let mut text = String::new();
    for row in buffer.content.chunks(buffer.area.width as usize) {
        let mut column = 0;
        while column < row.len() {
            let symbol = row[column].symbol();
            text.push_str(symbol);
            column += symbol.width().max(1);
        }
        text.push('\n');
    }
    text
}

#[test]
fn decorative_padding_yields_before_an_otherwise_usable_split() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "边界").unwrap();
    assert_eq!(
        work.editor_area(Rect::new(0, 1, 120, 30)),
        Rect::new(14, 3, 92, 26)
    );
    work.execute_command("style-terminal").unwrap();
    assert_eq!(
        work.editor_area(Rect::new(0, 1, 120, 30)),
        Rect::new(1, 1, 118, 30)
    );
    work.execute_command("style-reading").unwrap();
    work.execute_command("split-down").unwrap();
    let area = work.editor_area(Rect::new(0, 1, 80, 23));
    let panes = work.pane_views(area);
    assert_eq!(panes.len(), 2);
    assert!(panes.iter().all(|p| p.area.height >= 10));
    work.execute_command("split-right").unwrap();
    let area = work.editor_area(Rect::new(0, 1, 81, 11));
    let panes = work.pane_views(area);
    assert_eq!(panes.len(), 2);
    assert!(panes.iter().all(|p| p.area.width >= 40));
}

#[test]
fn terminal_style_explicit_background_keeps_contrasting_status_foreground() {
    use novelite_tui::{Background, ColorMode};
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.set_visual_style(VisualStyle::Terminal).unwrap();
    work.set_color_mode(ColorMode::Basic).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    for (background, foreground, fill) in [
        (Background::Light, Color::Black, Color::White),
        (Background::Dark, Color::White, Color::Black),
    ] {
        work.set_background(background).unwrap();
        terminal.draw(|frame| work.render(frame, "")).unwrap();
        let cell = &terminal.backend().buffer()[(0, 0)];
        assert_eq!(cell.fg, foreground);
        assert_eq!(cell.bg, fill);
    }
}
