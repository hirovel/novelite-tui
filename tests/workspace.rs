use novelite_tui::{Action, Outcome, Workspace};
use std::fs;

#[test]
fn writer_can_edit_save_exit_and_reopen_the_exact_manuscript() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("雨夜.md");
    fs::write(&path, "# 雨夜\r\n灯还亮着。\r\n").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Insert("序章\n".into())).unwrap();
    assert_eq!(work.dispatch(Action::Quit).unwrap(), Outcome::Exit);
    let reopened = Workspace::open(&path).unwrap();
    assert_eq!(reopened.text(), "序章\n# 雨夜\r\n灯还亮着。\r\n");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "序章\n# 雨夜\r\n灯还亮着。\r\n"
    );
}

#[test]
fn visible_manuscript_shows_markdown_and_escapes_terminal_controls() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("draft.md");
    fs::write(&path, "# 标题\r\n\x1b[2J正文\t尾").unwrap();
    let mut work = Workspace::open(path).unwrap();
    let view = work.view(30, 4);
    assert_eq!(view.lines, ["# 标题", "␛[2J正文    尾"]);
    assert_eq!(view.cursor, Some((0, 0)));
    assert_eq!(work.text(), "# 标题\r\n\x1b[2J正文\t尾");
}

#[test]
fn failed_save_keeps_the_session_and_text_for_retry() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("draft.txt");
    fs::write(&path, "old").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Insert("new ".into())).unwrap();
    fs::rename(&path, folder.path().join("original.txt")).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(work.dispatch(Action::Quit).is_err());
    assert!(work.is_dirty());
    assert_eq!(work.text(), "new old");
    assert_eq!(
        fs::read_to_string(folder.path().join("original.txt")).unwrap(),
        "old"
    );
    fs::remove_dir(&path).unwrap();
    fs::rename(folder.path().join("original.txt"), &path).unwrap();
    assert_eq!(work.dispatch(Action::Quit).unwrap(), Outcome::Exit);
    assert_eq!(fs::read_to_string(path).unwrap(), "new old");
}

#[test]
fn new_document_can_be_written_without_overwriting_a_file_created_after_open() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("new.txt");
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Insert("手稿".into())).unwrap();
    fs::write(&path, "external").unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    fs::remove_file(&path).unwrap();
    work.dispatch(Action::Save).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "手稿");
}

#[test]
fn writer_can_move_across_lines_and_correct_text() {
    use novelite_tui::Direction;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("draft.txt");
    fs::write(&path, "abc\nxyz").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Move(Direction::Down)).unwrap();
    work.dispatch(Action::Delete).unwrap();
    work.dispatch(Action::Backspace).unwrap();
    work.dispatch(Action::Insert("Q".into())).unwrap();
    assert_eq!(work.text(), "abc\nQz");
    assert_eq!(work.cursor(), 5);
    work.dispatch(Action::Save).unwrap();
    assert!(!work.is_dirty());
    assert_eq!(fs::read_to_string(path).unwrap(), "abc\nQz");
}

#[test]
fn end_and_enter_preserve_crlf_without_splitting_the_line_ending() {
    use novelite_tui::Direction;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("windows.txt");
    fs::write(&path, "甲\r\n乙").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Move(Direction::End)).unwrap();
    assert_eq!(work.view(40, 4).cursor, Some((2, 0)));
    work.dispatch(Action::Insert("末".into())).unwrap();
    work.dispatch(Action::Newline).unwrap();
    work.dispatch(Action::Insert("新".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "甲末\r\n新\r\n乙");
}

#[test]
fn crossing_and_deleting_crlf_does_not_leave_a_carriage_return() {
    use novelite_tui::Direction;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("windows.txt");
    fs::write(&path, "a\r\nb\r\nc").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Move(Direction::End)).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    assert_eq!(work.cursor(), 3);
    work.dispatch(Action::Backspace).unwrap();
    assert_eq!(work.text(), "ab\r\nc");
    work.dispatch(Action::Move(Direction::End)).unwrap();
    work.dispatch(Action::Delete).unwrap();
    assert_eq!(work.text(), "abc");
}
