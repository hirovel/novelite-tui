use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Direction, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Position};

fn key(work: &mut Workspace, code: KeyCode) {
    work.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn paste(work: &mut Workspace, text: &str) {
    work.handle_event(Event::Paste(text.into())).unwrap();
}
fn draw(work: &mut Workspace, width: u16) -> (Buffer, Position) {
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal.draw(|frame| work.render(frame, "")).unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    (terminal.backend().buffer().clone(), cursor)
}

#[test]
fn correcting_a_command_in_the_middle_executes_it_without_touching_the_manuscript() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("draft.txt");
    let mut work = Workspace::open(&path).unwrap();
    work.dispatch(Action::Insert("正文/".into())).unwrap();
    key(&mut work, KeyCode::F(1));
    paste(&mut work, "sXve");
    key(&mut work, KeyCode::Home);
    key(&mut work, KeyCode::Right);
    key(&mut work, KeyCode::Delete);
    key(&mut work, KeyCode::Char('a'));
    assert_eq!(work.palette().unwrap().query, "save");
    let selected = work.palette().unwrap().entries[0].id.clone();
    assert_eq!(selected, "save");
    key(&mut work, KeyCode::Down);
    let selected = work.palette().unwrap().selected;
    key(&mut work, KeyCode::End);
    key(&mut work, KeyCode::Left);
    assert_eq!(
        work.palette().unwrap().selected,
        selected,
        "caret movement must not reset the list selection"
    );
    key(&mut work, KeyCode::Home);
    key(&mut work, KeyCode::Char('x'));
    assert_eq!(work.palette().unwrap().selected, 0);
    key(&mut work, KeyCode::Backspace);
    key(&mut work, KeyCode::Enter);
    assert!(work.palette().is_none());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "正文/");
}

#[test]
fn query_editing_keeps_graphemes_whole_and_cancel_restores_body_selection() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.txt")).unwrap();
    work.dispatch(Action::Insert("正文/".into())).unwrap();
    work.dispatch(Action::Select(Direction::Left)).unwrap();
    key(&mut work, KeyCode::F(1));
    paste(&mut work, "甲👩‍💻e\u{301}乙");
    key(&mut work, KeyCode::Left);
    key(&mut work, KeyCode::Backspace);
    assert_eq!(work.palette().unwrap().query, "甲👩‍💻乙");
    key(&mut work, KeyCode::Left);
    key(&mut work, KeyCode::Delete);
    paste(&mut work, "中\n\r\t间");
    assert_eq!(work.palette().unwrap().query, "甲中间乙");
    key(&mut work, KeyCode::Home);
    key(&mut work, KeyCode::Char('e'));
    paste(&mut work, "\u{301}");
    key(&mut work, KeyCode::Backspace);
    assert_eq!(work.palette().unwrap().query, "甲中间乙");
    key(&mut work, KeyCode::Enter);
    assert!(
        work.palette().is_some(),
        "no match must not execute a command"
    );
    assert_eq!(work.text(), "正文/");
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.selected_text(), "/");
    key(&mut work, KeyCode::Char('/'));
    assert_eq!(work.text(), "正文/");
    assert!(work.palette().is_none());
}

#[test]
fn query_viewport_follows_caret_and_survives_resizing_without_splitting_unicode() {
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.txt")).unwrap();
    key(&mut work, KeyCode::F(1));
    let value = format!("前{}👩‍💻e\u{301}尾", "中文目录".repeat(12));
    paste(&mut work, &value);
    for width in [32, 80, 120, 40] {
        key(&mut work, KeyCode::End);
        let (buffer, end) = draw(&mut work, width);
        assert_eq!(buffer[(end.x - 2, end.y)].symbol(), "尾");
        key(&mut work, KeyCode::Left);
        key(&mut work, KeyCode::Left);
        let (buffer, caret) = draw(&mut work, width);
        assert_eq!(buffer[(caret.x, caret.y)].symbol(), "e\u{301}");
        key(&mut work, KeyCode::Left);
        let (buffer, caret) = draw(&mut work, width);
        assert_eq!(buffer[(caret.x, caret.y)].symbol(), "👩‍💻");
        key(&mut work, KeyCode::Home);
        let (buffer, start) = draw(&mut work, width);
        assert_eq!(buffer[(start.x, start.y)].symbol(), "前");
        key(&mut work, KeyCode::Right);
        let (_, right) = draw(&mut work, width);
        assert_eq!(right.x, start.x + 2);
        assert!(right.x < width);
    }
    assert_eq!(work.palette().unwrap().query, value);
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.text(), "");
}
#[test]
fn wide_grapheme_under_the_query_caret_stays_visible_at_the_viewport_edge() {
    use unicode_segmentation::UnicodeSegmentation;
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(directory.path().join("draft.txt")).unwrap();
    key(&mut work, KeyCode::F(1));
    let value = "a中👩‍💻e\u{301}".repeat(16);
    paste(&mut work, &value);
    for width in [32, 33, 40, 80] {
        key(&mut work, KeyCode::Home);
        for glyph in value.graphemes(true) {
            let (buffer, cursor) = draw(&mut work, width);
            assert_eq!(
                buffer[(cursor.x, cursor.y)].symbol(),
                glyph,
                "width={width}, caret={cursor:?}"
            );
            key(&mut work, KeyCode::Right);
        }
    }
}
