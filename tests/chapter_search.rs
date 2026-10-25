use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};

fn key(work: &mut Workspace, code: KeyCode) {
    work.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn query(work: &mut Workspace, text: &str) {
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    work.handle_event(Event::Paste(text.into())).unwrap();
    key(work, KeyCode::Enter);
}

#[test]
fn chapter_search_exposes_context_and_wraps_matches_without_splitting_graphemes() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("序：中文👩‍💻 e\u{301}。\n又见中文。".into()))
        .unwrap();
    query(&mut work, "中文");
    let panel = work.chapter_search().unwrap();
    assert_eq!(panel.total, 2);
    assert_eq!(panel.current, Some(1));
    assert!(panel.context.contains("序：【中文】👩‍💻"));
    assert_eq!(work.selected_text(), "中文");
    assert_eq!(work.cursor(), "序：中文".len());
    work.view(6, 8);
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::F(3),
        KeyModifiers::SHIFT,
    )))
    .unwrap();
    assert_eq!(work.cursor(), "序：中文👩‍💻 e\u{301}。\n又见中文".len());
    work.execute_command("find-next").unwrap();
    assert_eq!(work.chapter_search().unwrap().current, Some(1));
    query(&mut work, "e");
    assert_eq!(work.chapter_search().unwrap().total, 0);
    query(&mut work, "e\u{301}");
    assert_eq!(work.selected_text(), "e\u{301}");
    key(&mut work, KeyCode::Esc);
    assert!(work.chapter_search().is_none());
    work.dispatch(Action::Insert("é".into())).unwrap();
    assert_eq!(work.text(), "序：中文👩‍💻 é。\n又见中文。");
}

#[test]
fn literal_case_setting_handles_unicode_expansion_and_is_visible() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("Word word WORD İ i\u{307} i a.b axb".into()))
        .unwrap();
    query(&mut work, "word");
    assert!(work.chapter_search().unwrap().case_sensitive);
    assert_eq!(work.chapter_search().unwrap().total, 1);
    work.execute_command("find-toggle-case").unwrap();
    assert!(!work.chapter_search().unwrap().case_sensitive);
    assert_eq!(work.chapter_search().unwrap().total, 3);
    assert_eq!(work.selected_text(), "Word");
    query(&mut work, "İ");
    assert_eq!(work.chapter_search().unwrap().total, 2);
    work.execute_command("find-next").unwrap();
    assert_eq!(work.selected_text(), "i\u{307}");
    query(&mut work, "a.b");
    assert_eq!(work.chapter_search().unwrap().total, 1);
    assert_eq!(work.selected_text(), "a.b");
}

fn replace(work: &mut Workspace, command: &str, text: &str) {
    work.execute_command(command).unwrap();
    work.handle_event(Event::Paste(text.into())).unwrap();
    key(work, KeyCode::Enter);
}

#[test]
fn replacements_are_current_chapter_transactions_with_one_undo_for_all() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "查找").unwrap();
    work.dispatch(Action::Insert("甲中间甲末尾甲".into()))
        .unwrap();
    // Another pane's position inside unchanged text must survive batch edits.
    work.dispatch(Action::Move(novelite_tui::Direction::Home))
        .unwrap();
    work.dispatch(Action::Move(novelite_tui::Direction::Right))
        .unwrap();
    work.dispatch(Action::Move(novelite_tui::Direction::Right))
        .unwrap();
    work.execute_command("split-right").unwrap();
    query(&mut work, "甲");
    replace(&mut work, "replace-all", "乙乙");
    assert_eq!(work.text(), "乙乙中间乙乙末尾乙乙");
    work.focus_other_pane().unwrap();
    assert_eq!(work.cursor(), "乙乙中".len());
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "甲中间甲末尾甲");
    work.dispatch(Action::Redo).unwrap();
    assert_eq!(work.text(), "乙乙中间乙乙末尾乙乙");
    query(&mut work, "乙乙");
    replace(&mut work, "replace-current", "");
    assert_eq!(work.text(), "中间乙乙末尾乙乙");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "乙乙中间乙乙末尾乙乙");
    work.dispatch(Action::Save).unwrap();
    assert_eq!(
        std::fs::read_to_string(work.path()).unwrap(),
        "乙乙中间乙乙末尾乙乙"
    );
}

#[test]
fn stale_results_and_pending_replacements_cannot_modify_a_different_revision_or_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "过期查询").unwrap();
    work.dispatch(Action::Insert("前 猫 后 猫".into())).unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "别章").unwrap();
    query(&mut work, "猫");
    work.dispatch(Action::Move(novelite_tui::Direction::Home))
        .unwrap();
    work.dispatch(Action::Insert("较长的开头".into())).unwrap();
    assert!(work.chapter_search().unwrap().stale);
    work.execute_command("find-next").unwrap();
    assert_eq!(work.selected_text(), "猫");
    assert_eq!(work.cursor(), "较长的开头前 猫".len());
    work.execute_command("replace-current").unwrap();
    work.dispatch(Action::Insert("狗".into())).unwrap();
    work.handle_event(Event::Paste("不能插入".into())).unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.text(), "较长的开头前 狗 后 猫");
    work.execute_command("replace-all").unwrap();
    work.switch_chapter(&second).unwrap();
    work.handle_event(Event::Paste("跨章错误".into())).unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    key(&mut work, KeyCode::Esc);
    assert!(work.chapter_search().is_none());
    assert_eq!(work.text(), "");
    work.dispatch(Action::Insert("两个  空格".into())).unwrap();
    query(&mut work, " ");
    assert_eq!(work.chapter_search().unwrap().total, 2);
    replace(&mut work, "replace-all", "");
    assert_eq!(work.text(), "两个空格");
}

#[test]
fn closing_find_restores_the_originating_panel_and_its_filter() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "焦点").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Paste("第一章".into())).unwrap();
    query(&mut work, "正文");
    assert!(work.navigation().is_none());
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.navigation().unwrap().query, "第一章");
    key(&mut work, KeyCode::Enter);
    assert!(work.navigation().is_none());
    assert_eq!(work.text(), "正文");
}
