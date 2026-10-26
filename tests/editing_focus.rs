use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Outcome, Workspace};

fn control(work: &mut Workspace, character: char) {
    assert_eq!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        )))
        .unwrap(),
        Some(Outcome::Continue)
    );
}

#[test]
fn undo_in_a_chapter_filter_does_not_edit_the_manuscript_or_consume_its_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "写作").unwrap();
    work.dispatch(Action::Insert("draft1".into())).unwrap();
    work.dispatch(Action::Insert("draft2".into())).unwrap();
    let cursor = work.cursor();
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Paste("old-query".into())).unwrap();

    control(&mut work, 'z');

    assert_eq!(work.text(), "draft1draft2");
    assert_eq!(work.cursor(), cursor);
    assert_eq!(work.navigation().unwrap().query, "old-query");
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    control(&mut work, 'z');
    assert_eq!(work.text(), "draft1");
}

struct DelayedClipboard(std::sync::mpsc::Receiver<String>);

impl novelite_tui::Clipboard for DelayedClipboard {
    fn read_text(&mut self) -> std::io::Result<String> {
        self.0.recv().map_err(std::io::Error::other)
    }

    fn write_text(&mut self, _: &str) -> std::io::Result<()> {
        unreachable!("this session only requests a paste")
    }
}

#[test]
fn opening_the_command_query_prevents_a_pending_paste_from_changing_the_manuscript() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("原稿".into())).unwrap();
    let (release, result) = std::sync::mpsc::channel();
    work.set_clipboard(Box::new(DelayedClipboard(result)))
        .unwrap();
    work.execute_command("paste").unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    release.send("返回的素材".into()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while work.clipboard_busy() {
        work.tick(std::time::Instant::now()).unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    assert_eq!(work.text(), "原稿");
    assert!(work.palette().is_some());
    assert!(work.clipboard_message().unwrap().contains("未自动粘贴"));
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    work.execute_command("paste-internal").unwrap();
    assert_eq!(work.text(), "原稿返回的素材");
}

#[test]
fn returning_from_a_tool_does_not_retarget_an_earlier_system_paste() {
    for tool in ["chapters", "commands"] {
        let directory = tempfile::tempdir().unwrap();
        let mut work = Workspace::create_project(directory.path().join("book"), "写作").unwrap();
        work.dispatch(Action::Insert("原稿".into())).unwrap();
        let (release, result) = std::sync::mpsc::channel();
        work.set_clipboard(Box::new(DelayedClipboard(result)))
            .unwrap();
        work.execute_command("paste").unwrap();
        if tool == "commands" {
            work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)))
                .unwrap();
        } else {
            work.execute_command(tool).unwrap();
        }
        work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
            .unwrap();
        assert!(work.navigation().is_none());
        assert!(work.palette().is_none());
        release.send("过期素材".into()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while work.clipboard_busy() {
            work.tick(std::time::Instant::now()).unwrap();
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(work.text(), "原稿", "{tool}");
        assert!(work.clipboard_message().unwrap().contains("未自动粘贴"));
        work.execute_command("paste-internal").unwrap();
        assert_eq!(work.text(), "原稿过期素材");
    }
}

#[test]
fn switching_between_same_chapter_panes_cancels_the_earlier_paste_target() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("原稿".into())).unwrap();
    work.execute_command("split-right").unwrap();
    let (release, result) = std::sync::mpsc::channel();
    work.set_clipboard(Box::new(DelayedClipboard(result)))
        .unwrap();
    work.execute_command("paste").unwrap();
    for _ in 0..2 {
        work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)))
            .unwrap();
    }
    assert_eq!(work.active_pane(), 0);
    release.send("旧目标素材".into()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while work.clipboard_busy() {
        work.tick(std::time::Instant::now()).unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(work.text(), "原稿");
    assert!(work.clipboard_message().unwrap().contains("未自动粘贴"));
}

#[test]
fn editing_commands_respect_each_tool_and_leave_manuscript_selection_and_history_intact() {
    for tool in [
        "chapters",
        "recent-projects",
        "planning",
        "history",
        "preview",
        "conflicts",
        "move-chapter",
        "book-search",
        "design-settings",
        "commands",
        "rename-project",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut work = Workspace::create_project(directory.path().join("book"), "写作").unwrap();
        work.dispatch(Action::Insert("第一段".into())).unwrap();
        work.dispatch(Action::Insert("第二段".into())).unwrap();
        work.dispatch(Action::Undo).unwrap();
        let cursor = work.cursor();
        match tool {
            "preview" => {
                work.mark_snapshot().unwrap();
                work.execute_command("history").unwrap();
                work.handle_event(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::NONE,
                )))
                .unwrap();
                assert!(work.history_preview(40, 5).is_some());
            }
            "conflicts" => {
                std::fs::write(work.path(), "磁盘稿").unwrap();
                work.check_external().unwrap();
                work.execute_command(tool).unwrap();
            }
            "book-search" => work.start_book_search("段").unwrap(),
            "commands" => {
                work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)))
                    .unwrap();
            }
            _ => {
                work.execute_command(tool).unwrap();
            }
        }
        for character in ['z', 'y', 'a'] {
            work.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
            )))
            .unwrap();
        }
        for command in ["undo", "redo", "select-all"] {
            work.execute_command(command).unwrap();
        }
        assert_eq!(work.text(), "第一段", "{tool}");
        assert_eq!(work.cursor(), cursor, "{tool}");
        assert_eq!(work.selected_text(), "", "{tool}");
        work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
            .unwrap();
        if tool != "conflicts" {
            control(&mut work, 'y');
            assert_eq!(work.text(), "第一段第二段", "{tool}");
        } else {
            assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "磁盘稿");
            assert!(work.external_conflict().is_some());
        }
    }
}

#[test]
fn custom_editing_bindings_follow_the_same_target_as_default_shortcuts() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "写作").unwrap();
    work.load_keymap(directory.path().join("keys.json"));
    for (key, command) in [("F6", "undo"), ("F8", "redo"), ("F9", "select-all")] {
        work.set_keybinding(key, command).unwrap();
    }
    work.dispatch(Action::Insert("第一段".into())).unwrap();
    work.dispatch(Action::Insert("第二段".into())).unwrap();
    work.dispatch(Action::Undo).unwrap();
    work.execute_command("chapters").unwrap();
    for number in [6, 8, 9] {
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::F(number),
            KeyModifiers::NONE,
        )))
        .unwrap();
    }
    assert_eq!(work.text(), "第一段");
    assert_eq!(work.selected_text(), "");
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(work.text(), "第一段第二段");
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)))
        .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(work.selected_text(), "第一段");
}

#[test]
fn outline_commands_edit_the_draft_and_save_without_consuming_manuscript_history() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "写作").unwrap();
    work.dispatch(Action::Insert("正文保留".into())).unwrap();
    work.execute_command("edit-outline").unwrap();
    work.handle_event(Event::Paste("情节".into())).unwrap();
    work.handle_event(Event::Paste("转折".into())).unwrap();
    for character in ['z', 'y', 'a'] {
        control(&mut work, character);
    }
    work.handle_event(Event::Paste("替换草稿".into())).unwrap();
    control(&mut work, 'z');
    control(&mut work, 's');
    assert_eq!(work.chapter_plan().unwrap().outline, "情节转折");
    assert_eq!(work.text(), "正文保留");
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "正文保留");
    control(&mut work, 'z');
    assert_eq!(work.text(), "");
}

#[test]
fn choosing_paste_from_the_command_center_targets_the_returned_editor() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("原稿".into())).unwrap();
    let (release, result) = std::sync::mpsc::channel();
    work.set_clipboard(Box::new(DelayedClipboard(result)))
        .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)))
        .unwrap();
    work.handle_event(Event::Paste("paste".into())).unwrap();
    assert_eq!(work.palette().unwrap().entries[0].id, "paste");
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert!(work.palette().is_none());
    assert!(work.clipboard_busy());
    release.send("素材".into()).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while work.clipboard_busy() {
        work.tick(std::time::Instant::now()).unwrap();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(work.text(), "原稿素材");
    assert!(
        work.clipboard_message()
            .unwrap()
            .contains("已从系统剪贴板粘贴")
    );
}
