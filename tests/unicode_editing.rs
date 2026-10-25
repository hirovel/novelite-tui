use novelite_tui::{Action, Direction, Workspace};
use std::fs;

#[test]
fn grapheme_movement_and_backspace_keep_combined_characters_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.txt");
    fs::write(&path, "中e\u{301}👩‍💻🇨🇳末").unwrap();
    let mut work = Workspace::open(path).unwrap();
    for _ in 0..3 {
        work.dispatch(Action::Move(Direction::Right)).unwrap();
    }
    assert_eq!(work.cursor(), "中e\u{301}👩‍💻".len());
    work.dispatch(Action::Backspace).unwrap();
    assert_eq!(work.text(), "中e\u{301}🇨🇳末");
    work.dispatch(Action::Move(Direction::Left)).unwrap();
    work.dispatch(Action::Delete).unwrap();
    assert_eq!(work.text(), "中🇨🇳末");
}

#[test]
fn replacing_selected_text_with_itself_collapses_the_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.txt");
    fs::write(&path, "abc").unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("abc".into())).unwrap();
    assert_eq!(work.selected_text(), "");
    assert!(!work.is_dirty());
    work.dispatch(Action::Insert("x".into())).unwrap();
    assert_eq!(work.text(), "abcx");
}

#[test]
fn a_hard_newline_after_a_full_visual_row_does_not_add_a_phantom_row() {
    for ending in ["\n", "\r\n"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full.txt");
        fs::write(&path, format!("abcd{ending}x")).unwrap();
        let mut work = Workspace::open(path).unwrap();
        assert_eq!(work.view(4, 6).lines, ["abcd", "x"]);
        work.dispatch(Action::Move(Direction::Down)).unwrap();
        assert_eq!(work.view(4, 6).cursor, Some((0, 1)));
    }
}

#[test]

fn decoded_text_is_committed_but_ordinary_key_releases_are_not_repeated() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind as Kind, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.txt");
    let mut work = Workspace::open(path).unwrap();
    // Committed text events are distinct from ordinary key releases.
    for (ch, kind) in [
        ('e', Kind::Press),
        ('e', Kind::Release),
        ('\u{301}', Kind::Press),
        ('👩', Kind::Press),
        ('\u{200d}', Kind::Press),
        ('💻', Kind::Press),
    ] {
        work.handle_event(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char(ch),
            KeyModifiers::NONE,
            kind,
        )))
        .unwrap();
    }
    assert_eq!(work.text(), "e\u{301}👩‍💻");
}

#[test]
fn arrow_without_shift_collapses_selection_to_its_directional_edge() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selection.txt");
    fs::write(&path, "甲乙丙").unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Move(Direction::Left)).unwrap();
    assert_eq!(work.cursor(), 0);
    assert_eq!(work.selected_text(), "");
    work.dispatch(Action::Select(Direction::Right)).unwrap();
    work.dispatch(Action::Select(Direction::Right)).unwrap();
    assert_eq!(work.view(20, 4).selection, [(0, 0, 2), (2, 0, 2)]);
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    assert_eq!(work.cursor(), "甲乙".len());
}

#[test]
fn inserting_a_joiner_keeps_the_caret_outside_the_new_grapheme() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("join.txt");
    fs::write(&path, "👩💻!").unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Insert("\u{200d}".into())).unwrap();
    assert_eq!(work.text(), "👩‍💻!");
    assert_eq!(work.cursor(), "👩‍💻".len());
    work.dispatch(Action::Backspace).unwrap();
    assert_eq!(work.text(), "!");
    work.dispatch(Action::Undo).unwrap();
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "👩💻!");
}

#[test]
fn soft_wrap_uses_terminal_columns_and_vertical_motion_remembers_the_column() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrap.txt");
    fs::write(&path, "甲乙丙丁e\u{301}Z\n尾").unwrap();
    let mut work = Workspace::open(path).unwrap();
    assert_eq!(work.view(6, 6).lines, ["甲乙丙", "丁e\u{301}Z", "尾"]);
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Move(Direction::Down)).unwrap();
    assert_eq!(work.cursor(), "甲乙丙丁e\u{301}Z".len());
    work.dispatch(Action::Move(Direction::Down)).unwrap();
    assert_eq!(work.view(6, 6).cursor, Some((2, 2)));
    work.dispatch(Action::Move(Direction::Up)).unwrap();
    assert_eq!(work.view(6, 6).cursor, Some((4, 1)));
    work.dispatch(Action::Select(Direction::Home)).unwrap();
    assert_eq!(work.selected_text(), "丁e\u{301}Z");
    work.view(4, 6);
    assert_eq!(work.selected_text(), "丁e\u{301}Z");
    assert_eq!(work.text(), "甲乙丙丁e\u{301}Z\n尾");
    assert!(!work.is_dirty());
}

#[test]
fn replacing_a_selection_is_one_undoable_transaction_and_redo_is_branch_safe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.txt");
    fs::write(&path, "甲👩‍💻乙").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Select(Direction::Right)).unwrap();
    assert_eq!(work.selected_text(), "👩‍💻");
    work.dispatch(Action::Insert("替换\n整批".into())).unwrap();
    assert_eq!(work.text(), "甲替换\n整批乙");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "甲👩‍💻乙");
    assert_eq!(work.selected_text(), "👩‍💻");
    assert!(!work.is_dirty());
    work.dispatch(Action::Redo).unwrap();
    assert_eq!(work.text(), "甲替换\n整批乙");
    work.dispatch(Action::Save).unwrap();
    work.dispatch(Action::Undo).unwrap();
    assert!(work.is_dirty());
    work.dispatch(Action::Insert("新".into())).unwrap();
    work.dispatch(Action::Redo).unwrap();
    assert_eq!(work.text(), "甲新乙");
}
