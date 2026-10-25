use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use novelite_tui::{Action, Workspace};
use ratatui::layout::Rect;

struct DeniedClipboard;
impl novelite_tui::Clipboard for DeniedClipboard {
    fn read_text(&mut self) -> std::io::Result<String> {
        Err(std::io::Error::other("受控拒绝读取"))
    }
    fn write_text(&mut self, _: &str) -> std::io::Result<()> {
        Err(std::io::Error::other("受控拒绝写入"))
    }
}
fn settle(work: &mut Workspace) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while work.clipboard_busy() {
        work.tick(std::time::Instant::now()).unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn failed_system_clipboard_preserves_cut_selection_and_never_pastes_stale_internal_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.set_clipboard(Box::new(DeniedClipboard)).unwrap();
    work.dispatch(Action::Insert("本地选区".into())).unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.execute_command("cut").unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "本地选区");
    assert_eq!(work.selected_text(), "本地选区");
    assert!(
        work.clipboard_message()
            .unwrap()
            .contains("系统剪贴板写入失败")
    );
    work.dispatch(Action::Move(novelite_tui::Direction::Right))
        .unwrap();
    work.execute_command("paste").unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "本地选区");
    assert!(work.clipboard_message().unwrap().contains("未粘贴"));
    work.execute_command("paste-internal").unwrap();
    assert_eq!(work.text(), "本地选区本地选区");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "本地选区");
}

struct SharedClipboard(std::sync::Arc<std::sync::Mutex<String>>);
impl novelite_tui::Clipboard for SharedClipboard {
    fn read_text(&mut self) -> std::io::Result<String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write_text(&mut self, text: &str) -> std::io::Result<()> {
        *self.0.lock().unwrap() = text.into();
        Ok(())
    }
}

#[test]
fn system_copy_cut_paste_and_bracketed_paste_are_single_safe_edit_transactions() {
    use crossterm::event::{KeyCode, KeyEvent};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    let system = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    work.set_clipboard(Box::new(SharedClipboard(system.clone())))
        .unwrap();
    work.dispatch(Action::Insert("中文👩‍💻e\u{301}".into()))
        .unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    settle(&mut work);
    assert_eq!(*system.lock().unwrap(), "中文👩‍💻e\u{301}");
    work.execute_command("cut").unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "中文👩‍💻e\u{301}");
    let payload = "外部\r\n\x1b[31m\x11\0尾";
    *system.lock().unwrap() = payload.into();
    work.execute_command("paste").unwrap();
    settle(&mut work);
    assert_eq!(work.text(), payload);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "中文👩‍💻e\u{301}");
    work.handle_event(Event::Paste(payload.into())).unwrap();
    assert_eq!(work.text(), payload);
    assert!(work.view(40, 5).lines.join("\n").contains("␛[31m␑␀尾"));
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), payload);
}

struct DelayedClipboard(std::sync::mpsc::Receiver<String>);
impl novelite_tui::Clipboard for DelayedClipboard {
    fn read_text(&mut self) -> std::io::Result<String> {
        self.0.recv().map_err(std::io::Error::other)
    }
    fn write_text(&mut self, _: &str) -> std::io::Result<()> {
        self.0.recv().map(|_| ()).map_err(std::io::Error::other)
    }
}

#[test]
fn external_refresh_invalidates_pending_paste_even_at_the_same_cursor_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(&path, "原文").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    work.set_clipboard(Box::new(DelayedClipboard(rx))).unwrap();
    work.execute_command("paste").unwrap();
    std::fs::write(&path, "外部全新稿").unwrap();
    work.check_external().unwrap();
    assert_eq!(work.cursor(), 0);
    tx.send("过期素材".into()).unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "外部全新稿");
    assert!(work.clipboard_message().unwrap().contains("未自动粘贴"));
}

#[test]
fn delayed_clipboard_does_not_edit_a_new_location_or_block_typing() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    work.set_clipboard(Box::new(DelayedClipboard(rx))).unwrap();
    work.execute_command("paste").unwrap();
    work.dispatch(Action::Insert("等候时继续写".into()))
        .unwrap();
    assert!(work.clipboard_busy());
    assert!(work.dispatch(Action::Quit).is_err());
    tx.send("延迟返回的素材".into()).unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "等候时继续写");
    assert!(work.clipboard_message().unwrap().contains("未自动粘贴"));
    work.execute_command("paste-internal").unwrap();
    assert_eq!(work.text(), "等候时继续写延迟返回的素材");
    work.dispatch(Action::SelectAll).unwrap();
    work.execute_command("cut").unwrap();
    work.dispatch(Action::Move(novelite_tui::Direction::Left))
        .unwrap();
    tx.send(String::new()).unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "等候时继续写延迟返回的素材");
    assert!(work.clipboard_message().unwrap().contains("未删除正文"));
    work.execute_command("paste").unwrap();
    work.execute_command("cancel-clipboard").unwrap();
    assert_eq!(
        work.dispatch(Action::Quit).unwrap(),
        novelite_tui::Outcome::Exit
    );
    tx.send("已取消的内容".into()).unwrap();
    settle(&mut work);
    assert_eq!(work.text(), "等候时继续写延迟返回的素材");
}

#[test]
fn modal_mouse_and_clipboard_commands_do_not_change_hidden_editor_and_settings_survive_project_switch()
 {
    use crossterm::event::{KeyCode, KeyEvent};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("one"), "一").unwrap();
    work.configure_mouse(true);
    work.dispatch(Action::Insert("应用选区".into())).unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.execute_command("copy-internal").unwrap();
    work.editor_view(Rect::new(4, 2, 20, 5));
    work.execute_command("chapters").unwrap();
    mouse(&mut work, MouseEventKind::Down(MouseButton::Left), 4, 2);
    assert_eq!(work.selected_text(), "应用选区");
    assert!(work.execute_command("cut").is_err());
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    work.new_project(dir.path().join("two"), "二").unwrap();
    assert!(work.mouse_enabled());
    work.execute_command("paste-internal").unwrap();
    assert_eq!(work.text(), "应用选区");
}

fn mouse(work: &mut Workspace, kind: MouseEventKind, column: u16, row: u16) {
    work.handle_event(Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}

#[test]
fn clicks_and_drag_selection_use_rendered_cells_without_splitting_graphemes() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("中👩‍💻e\u{301}文\n第二行".into()))
        .unwrap();
    work.configure_mouse(true);
    work.editor_view(Rect::new(4, 2, 8, 4));
    mouse(&mut work, MouseEventKind::Down(MouseButton::Left), 7, 2);
    assert_eq!(work.cursor(), "中".len()); // second cell of emoji stays at its boundary
    mouse(&mut work, MouseEventKind::Drag(MouseButton::Left), 9, 2);
    assert_eq!(work.selected_text(), "👩‍💻e\u{301}");
    mouse(&mut work, MouseEventKind::Up(MouseButton::Left), 9, 2);
    work.dispatch(Action::Insert("替".into())).unwrap();
    assert_eq!(work.text(), "中替文\n第二行");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "中👩‍💻e\u{301}文\n第二行");
    let cursor = work.cursor();
    work.configure_mouse(false);
    mouse(&mut work, MouseEventKind::Down(MouseButton::Left), 4, 2);
    assert_eq!(work.cursor(), cursor);
}

#[test]
fn wheel_scroll_keeps_caret_and_outside_drag_extends_selection_with_autoscroll() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("scroll.md")).unwrap();
    work.dispatch(Action::Insert("甲\n乙\n丙\n丁\n戊\n己".into()))
        .unwrap();
    work.configure_mouse(true);
    let area = Rect::new(2, 3, 6, 2);
    work.editor_view(area);
    mouse(&mut work, MouseEventKind::ScrollUp, 3, 3);
    let view = work.editor_view(area);
    assert_eq!(view.lines, ["乙", "丙"]);
    assert!(view.cursor.is_none());
    assert_eq!(work.cursor(), "甲\n乙\n丙\n丁\n戊\n己".len());
    mouse(&mut work, MouseEventKind::Down(MouseButton::Left), 2, 3);
    mouse(&mut work, MouseEventKind::Drag(MouseButton::Left), 4, 6);
    assert_eq!(work.selected_text(), "乙\n丙\n丁");
    let now = std::time::Instant::now();
    work.tick(now + std::time::Duration::from_secs(1)).unwrap();
    assert_eq!(work.selected_text(), "乙\n丙\n丁\n戊");
    mouse(&mut work, MouseEventKind::Up(MouseButton::Left), 4, 6);
    work.tick(now + std::time::Duration::from_secs(2)).unwrap();
    assert_eq!(work.selected_text(), "乙\n丙\n丁\n戊");
}

#[test]
fn resizing_preserves_reading_anchor_and_keeps_a_previously_visible_caret_in_view() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("resize.md")).unwrap();
    work.dispatch(Action::Insert(
        "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉".into(),
    ))
    .unwrap();
    work.configure_mouse(true);
    let wide = Rect::new(0, 1, 10, 2);
    work.editor_view(wide);
    mouse(&mut work, MouseEventKind::Down(MouseButton::Left), 6, 1);
    let cursor = work.cursor();
    let narrow = work.editor_view(Rect::new(0, 1, 4, 2));
    assert_eq!(work.cursor(), cursor);
    assert!(narrow.cursor.is_some(), "visible caret lost on resize");
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Move(novelite_tui::Direction::Left))
        .unwrap();
    work.editor_view(wide);
    mouse(&mut work, MouseEventKind::ScrollDown, 1, 1);
    let before = work.editor_view(wide);
    assert_eq!(before.lines[0], "巳午未申酉");
    let after = work.editor_view(Rect::new(0, 1, 4, 2));
    assert_eq!(after.lines[0], "辰巳");
}
