use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};
use ratatui::{
    Terminal,
    backend::{Backend, TestBackend},
    buffer::Buffer,
    layout::Position,
};

fn key(work: &mut Workspace, code: KeyCode) {
    work.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn paste(work: &mut Workspace, value: &str) {
    work.handle_event(Event::Paste(value.into())).unwrap();
}
fn draw(work: &mut Workspace, width: u16) -> (Buffer, Position) {
    let mut terminal = Terminal::new(TestBackend::new(width, 18)).unwrap();
    terminal.draw(|frame| work.render(frame, "")).unwrap();
    let position = terminal.backend_mut().get_cursor_position().unwrap();
    (terminal.backend().buffer().clone(), position)
}
#[test]
fn rename_prompt_edits_at_grapheme_boundaries_without_touching_manuscript() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "旧名").unwrap();
    work.dispatch(Action::Insert("正文/不变".into())).unwrap();
    work.execute_command("rename-project").unwrap();
    paste(&mut work, "甲👩‍💻e\u{301}乙");
    key(&mut work, KeyCode::Left);
    key(&mut work, KeyCode::Backspace);
    assert_eq!(work.prompt().unwrap().text, "甲👩‍💻乙");
    key(&mut work, KeyCode::Left);
    key(&mut work, KeyCode::Delete);
    assert_eq!(work.prompt().unwrap().text, "甲乙");
    paste(&mut work, "中\n\t间");
    key(&mut work, KeyCode::Home);
    key(&mut work, KeyCode::Char('前'));
    key(&mut work, KeyCode::End);
    key(&mut work, KeyCode::Char('后'));
    key(&mut work, KeyCode::Home);
    key(&mut work, KeyCode::Right);
    key(&mut work, KeyCode::Delete);
    key(&mut work, KeyCode::Enter);
    assert_eq!(work.project().unwrap().title, "前中间乙后");
    assert_eq!(work.text(), "正文/不变");
}
#[test]
fn corrected_save_path_is_the_path_written_to_disk() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.txt")).unwrap();
    work.dispatch(Action::Insert("路径修正后正文完整".into()))
        .unwrap();
    work.execute_command("save-as").unwrap();
    let wrong = directory.path().join("xy.txt");
    paste(&mut work, &wrong.to_string_lossy());
    for _ in 0..5 {
        key(&mut work, KeyCode::Left);
    }
    key(&mut work, KeyCode::Delete);
    key(&mut work, KeyCode::Char('z'));
    key(&mut work, KeyCode::Enter);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("xz.txt")).unwrap(),
        "路径修正后正文完整"
    );
    assert!(!wrong.exists());
}
#[test]
fn long_cjk_prompt_scrolls_with_cursor_and_preserves_whole_graphemes() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.txt")).unwrap();
    work.execute_command("save-as").unwrap();
    let value = format!("前{}👩‍💻e\u{301}尾", "中文目录".repeat(12));
    paste(&mut work, &value);
    for width in [12, 20, 32] {
        let (buffer, cursor) = draw(&mut work, width);
        assert_eq!(buffer[(cursor.x - 2, cursor.y)].symbol(), "尾");
        assert!(cursor.x < width - 1);
        key(&mut work, KeyCode::Home);
        let (buffer, cursor) = draw(&mut work, width);
        assert_eq!(cursor.x, 1);
        assert_eq!(buffer[(cursor.x, cursor.y)].symbol(), "前");
        key(&mut work, KeyCode::Right);
        let (_, cursor) = draw(&mut work, width);
        assert_eq!(cursor.x, 3);
        key(&mut work, KeyCode::End);
        key(&mut work, KeyCode::Left);
        key(&mut work, KeyCode::Left);
        let (buffer, cursor) = draw(&mut work, width);
        assert_eq!(buffer[(cursor.x, cursor.y)].symbol(), "e\u{301}");
        key(&mut work, KeyCode::End);
    }
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.text(), "");
}

#[test]
fn horizontal_scroll_stays_put_while_the_caret_is_still_visible() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.txt")).unwrap();
    work.execute_command("save-as").unwrap();
    paste(&mut work, "abcdefghijklmnopqrstuvwxyz");
    let (before, caret) = draw(&mut work, 20);
    for step in 1..=4 {
        key(&mut work, KeyCode::Left);
        let (after, moved) = draw(&mut work, 20);
        assert_eq!(moved.x, caret.x - step);
        for x in 1..19 {
            assert_eq!(before[(x, caret.y)], after[(x, caret.y)]);
        }
    }
    key(&mut work, KeyCode::End);
    let (_, expanded) = draw(&mut work, 40);
    assert_eq!(expanded.x, 27);
}
