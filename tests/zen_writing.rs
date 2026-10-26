use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, Background, ColorMode, Direction, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};
use std::time::{Duration, Instant};

fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn draw(w: &mut Workspace, width: u16, height: u16, now: Instant) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render_design(f, "", now)).unwrap();
    t.backend().buffer().clone()
}
fn text(b: &Buffer) -> String {
    b.content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .replace(' ', "")
}
fn point(b: &Buffer, symbol: &str) -> (u16, u16) {
    let index = b
        .content
        .iter()
        .position(|c| c.symbol() == symbol)
        .unwrap_or_else(|| panic!("missing {symbol}: {}", text(b)));
    (
        (index % b.area.width as usize) as u16,
        (index / b.area.width as usize) as u16,
    )
}
fn click(w: &mut Workspace, at: (u16, u16)) {
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: at.0,
        row: at.1,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}

#[test]
fn zen_has_one_centered_pane_and_preserves_hidden_selection_and_shared_undo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let original = "甲乙丙丁👩‍💻e\u{301}。\r\n下一段。";
    std::fs::write(&path, original).unwrap();
    let mut w = Workspace::open(&path).unwrap();
    let area = Rect::new(0, 0, 120, 30);
    let unsplit = w.editor_area(area);
    w.execute_command("split-right").unwrap();
    for _ in 0..2 {
        w.dispatch(Action::Move(Direction::Right)).unwrap();
    }
    for _ in 0..2 {
        w.dispatch(Action::Select(Direction::Right)).unwrap();
    }
    assert_eq!(w.selected_text(), "丙丁");
    w.focus_other_pane().unwrap();
    assert_eq!(w.active_pane(), 1);
    w.execute_command("toggle-zen").unwrap();
    assert!(w.is_split(), "Zen must retain the split, not close it");
    assert_eq!(w.editor_area(area), unsplit);
    let panes = w.pane_views(w.editor_area(area));
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].index, 1);
    assert!(panes[0].heading.is_none());
    w.dispatch(Action::Insert("前".into())).unwrap();
    w.execute_command("toggle-zen").unwrap();
    assert_eq!(w.pane_views(area).len(), 2);
    assert_eq!(w.active_pane(), 1);
    w.focus_other_pane().unwrap();
    assert_eq!(w.selected_text(), "丙丁");
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), original);
    w.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn zen_restores_stacked_chapters_and_saves_the_active_second_pane() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let config = dir.path().join("layout.json");
    let mut w = Workspace::create_project(&root, "书名不应占专注顶栏").unwrap();
    w.load_layout(&config).unwrap();
    w.execute_command("pin-chapters").unwrap();
    w.dispatch(Action::Insert("第一章初稿".into())).unwrap();
    let first_path = w.path().to_owned();
    let volume = w.project().unwrap().volumes[0].id.clone();
    let second = w.create_chapter(&volume, "第二章").unwrap();
    w.execute_command("split-down").unwrap();
    w.switch_chapter(&second).unwrap();
    w.dispatch(Action::Insert("第二章初稿".into())).unwrap();
    let second_path = w.path().to_owned();
    let second_cursor = w.cursor();
    w.focus_other_pane().unwrap();
    assert_eq!(w.active_pane(), 1);
    let config_before = std::fs::read(&config).unwrap();
    w.execute_command("toggle-zen").unwrap();
    let panes = w.pane_views(Rect::new(0, 0, 120, 40));
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].index, 1);
    w.dispatch(Action::Insert("续写".into())).unwrap();
    for (width, height) in [(32, 10), (80, 24), (120, 40), (180, 50)] {
        draw(&mut w, width, height, Instant::now());
        assert_eq!(w.text(), "第一章初稿续写");
    }
    w.execute_command("toggle-zen").unwrap();
    let panes = w.pane_views(Rect::new(0, 0, 120, 40));
    assert_eq!(panes.len(), 2);
    assert!(panes[0].area.y < panes[1].area.y);
    assert_eq!(std::fs::read(&config).unwrap(), config_before);
    w.focus_other_pane().unwrap();
    assert_eq!(w.active_chapter().unwrap().id, second);
    assert_eq!(w.cursor(), second_cursor);
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), "");
    w.dispatch(Action::Redo).unwrap();
    w.execute_command("toggle-zen").unwrap();
    w.dispatch(Action::Quit).unwrap();
    assert_eq!(
        std::fs::read_to_string(first_path).unwrap(),
        "第一章初稿续写"
    );
    assert_eq!(std::fs::read_to_string(second_path).unwrap(), "第二章初稿");
}

#[test]
fn zen_settings_and_footer_exit_are_discoverable_without_growing_common_commands() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("正文 / 保留".into())).unwrap();
    let cursor = w.cursor();
    key(&mut w, KeyCode::F(1));
    assert_eq!(w.palette().unwrap().entries.len(), 9);
    w.handle_event(Event::Paste("专注".into())).unwrap();
    assert!(
        w.palette()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.id == "toggle-zen")
    );
    key(&mut w, KeyCode::Esc);
    key(&mut w, KeyCode::F(4));
    key(&mut w, KeyCode::Tab);
    key(&mut w, KeyCode::Tab);
    for _ in 0..4 {
        key(&mut w, KeyCode::Down);
    }
    let b = draw(&mut w, 120, 40, Instant::now());
    assert!(text(&b).contains("专注写作"));
    key(&mut w, KeyCode::Enter);
    assert!(w.zen_mode());
    key(&mut w, KeyCode::Esc);
    let b = draw(&mut w, 120, 40, Instant::now());
    assert!(text(&b).contains("退出专注"));
    click(&mut w, point(&b, "退"));
    assert!(!w.zen_mode());
    draw(&mut w, 120, 40, Instant::now());
    click(&mut w, point(&b, "退"));
    assert!(
        !w.zen_mode(),
        "stale exit target must not toggle Zen back on"
    );
    assert_eq!(w.cursor(), cursor);
    key(&mut w, KeyCode::Char('/'));
    assert_eq!(w.text(), "正文 / 保留/");
}

#[test]
fn zen_keeps_native_canvas_and_immediate_geometry_for_all_color_modes() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "隐藏的书名").unwrap();
    w.dispatch(Action::Insert("正文按自己的节奏展开。".into()))
        .unwrap();
    w.execute_command("split-right").unwrap();
    let now = Instant::now();
    for (mode, background) in [
        (ColorMode::TrueColor, Background::Terminal),
        (ColorMode::Monochrome, Background::Terminal),
        (ColorMode::TrueColor, Background::Dark),
    ] {
        w.set_color_mode(mode).unwrap();
        w.set_background(background).unwrap();
        w.set_design_motion(true);
        draw(&mut w, 120, 40, now);
        w.execute_command("toggle-zen").unwrap();
        for (width, height) in [(1, 1), (32, 10), (80, 24), (120, 40), (180, 50)] {
            let first = draw(&mut w, width, height, now);
            let final_frame = draw(&mut w, width, height, now + Duration::from_millis(250));
            assert_eq!(first, final_frame);
            assert!(!w.design_needs_frame());
            assert!(!text(&first).contains("隐藏的书名"));
            if width >= 32 {
                assert!(text(&first).contains("退出专注"));
            }
            if background == Background::Terminal {
                assert!(
                    first
                        .content
                        .iter()
                        .all(|c| c.bg == ratatui::style::Color::Reset)
                );
            }
        }
        w.execute_command("toggle-zen").unwrap();
    }
    assert_eq!(w.text(), "正文按自己的节奏展开。");
}

#[test]
fn zen_ignores_hidden_pane_hits_and_keeps_visual_row_navigation_and_tools_safe() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    let original = "甲乙丙丁戊己庚辛壬癸。".repeat(12) + "\r\n\r\n末段。";
    w.dispatch(Action::Insert(original.clone())).unwrap();
    w.configure_mouse(true);
    w.execute_command("split-right").unwrap();
    let before = w.pane_views(Rect::new(0, 0, 120, 30));
    let hidden = before[1].area;
    w.execute_command("toggle-zen").unwrap();
    click(&mut w, (hidden.x, hidden.y));
    assert_eq!(w.active_pane(), 0);
    let body = w.editor_area(Rect::new(0, 0, 120, 30));
    let pane = w.pane_views(body).pop().unwrap();
    click(&mut w, (pane.area.x + 14, pane.area.y + 1));
    let clicked = w.pane_views(body).pop().unwrap().view.cursor.unwrap();
    assert_eq!(clicked, (14, 1));
    key(&mut w, KeyCode::Up);
    assert_eq!(
        w.pane_views(body).pop().unwrap().view.cursor.unwrap(),
        (14, 0)
    );
    key(&mut w, KeyCode::Down);
    assert_eq!(
        w.pane_views(body).pop().unwrap().view.cursor.unwrap(),
        clicked
    );
    let caret = w.cursor();
    for code in [KeyCode::F(1), KeyCode::F(4)] {
        key(&mut w, code);
        draw(&mut w, 120, 40, Instant::now());
        key(&mut w, KeyCode::Esc);
        draw(&mut w, 120, 40, Instant::now());
        assert_eq!(w.cursor(), caret);
        assert!(w.zen_mode());
    }
    assert_eq!(w.text(), original);
}

#[test]
fn zen_keeps_read_only_conflict_and_failed_save_visible_on_a_narrow_footer() {
    use novelite_tui::SaveState;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let writer = Workspace::create_project(&root, "只读").unwrap();
    let mut reader = Workspace::open_project(&root).unwrap();
    assert_eq!(reader.save_state(), SaveState::ReadOnly);
    reader.execute_command("toggle-zen").unwrap();
    let b = draw(&mut reader, 32, 10, Instant::now());
    assert!(text(&b).contains("只读"));
    assert!(text(&b).contains("退出专注"));
    drop(reader);
    drop(writer);

    let path = dir.path().join("draft.md");
    std::fs::write(&path, "磁盘原稿").unwrap();
    let mut w = Workspace::open(&path).unwrap();
    w.execute_command("toggle-zen").unwrap();
    w.dispatch(Action::Insert("本地".into())).unwrap();
    std::fs::write(&path, "外部新稿").unwrap();
    w.check_external().unwrap();
    assert!(matches!(w.save_state(), SaveState::Conflict(_)));
    let b = draw(&mut w, 32, 10, Instant::now());
    assert!(text(&b).contains("冲突"));
    assert!(text(&b).contains("退出专注"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "外部新稿");

    let failure_path = dir.path().join("failure.md");
    std::fs::write(&failure_path, "安全原稿").unwrap();
    let mut w = Workspace::open(&failure_path).unwrap();
    w.execute_command("toggle-zen").unwrap();
    w.dispatch(Action::Insert("未保存".into())).unwrap();
    let original_permissions = std::fs::metadata(&failure_path).unwrap().permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    std::fs::set_permissions(&failure_path, permissions).unwrap();
    let result = w.dispatch(Action::Save);
    let state = w.save_state();
    let mut t = Terminal::new(TestBackend::new(32, 10)).unwrap();
    t.draw(|f| w.render(f, "其他操作提示")).unwrap();
    let rendered = text(t.backend().buffer());
    std::fs::set_permissions(&failure_path, original_permissions).unwrap();
    assert!(result.is_err());
    assert!(matches!(state, SaveState::Failed(_)));
    assert!(rendered.contains("保存失败"), "{rendered}");
    assert!(rendered.contains("退出专注"));
    assert_eq!(w.text(), "未保存安全原稿");
    assert_eq!(std::fs::read_to_string(&failure_path).unwrap(), "安全原稿");
}

#[test]
fn hidden_pane_keeps_its_scrolled_view_while_zen_edits_the_other_view() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(
        &path,
        (0..100).map(|i| format!("第{i}行\n")).collect::<String>(),
    )
    .unwrap();
    let mut w = Workspace::open(path).unwrap();
    let area = Rect::new(0, 0, 120, 30);
    w.execute_command("split-right").unwrap();
    w.pane_views(area);
    for _ in 0..60 {
        w.dispatch(Action::Move(Direction::Down)).unwrap();
    }
    let panes = w.pane_views(area);
    let hidden_lines = panes[0].view.lines.clone();
    let hidden_cursor = panes[0].view.cursor;
    w.focus_other_pane().unwrap();
    w.execute_command("toggle-zen").unwrap();
    for _ in 0..15 {
        w.dispatch(Action::Move(Direction::Down)).unwrap();
    }
    w.pane_views(w.editor_area(area));
    w.execute_command("toggle-zen").unwrap();
    let panes = w.pane_views(area);
    assert_eq!(panes[0].view.lines, hidden_lines);
    assert_eq!(panes[0].view.cursor, hidden_cursor);
    assert_eq!(w.active_pane(), 1);
}

#[test]
fn a_narrow_zen_footer_never_labels_an_outline_draft_as_saved() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "草稿").unwrap();
    w.dispatch(Action::Insert("已保存正文".into())).unwrap();
    w.dispatch(Action::Save).unwrap();
    w.execute_command("toggle-zen").unwrap();
    w.execute_command("edit-outline").unwrap();
    w.dispatch(Action::Insert("未提交的大纲".into())).unwrap();
    let b = draw(&mut w, 24, 10, Instant::now());
    let footer = (0..24)
        .map(|x| b[(x, 9)].symbol())
        .collect::<String>()
        .replace(' ', "");
    assert!(footer.contains("大纲未保存"), "{footer}");
    assert!(!footer.contains("已保存"));
}
