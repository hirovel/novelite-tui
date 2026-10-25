use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Direction, Workspace};
use unicode_segmentation::UnicodeSegmentation;
#[test]
fn vertical_arrows_visit_adjacent_visual_rows_at_every_grapheme_boundary() {
    for text in [
        "一二三\n\n四五六\n\n末尾",
        "abcd\nef\n\nghijklmnop\n",
        "甲乙👩‍💻e\u{301}XYZ\n\n短\n末尾长段abcdef",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("draft.txt");
        std::fs::write(&path, text).unwrap();
        for width in [1, 2, 4, 6, 7, 10] {
            for n in 0..=text.graphemes(true).count() {
                for (key, down) in [(KeyCode::Down, true), (KeyCode::Up, false)] {
                    let mut w = Workspace::open(&path).unwrap();
                    w.view(width, 200);
                    for _ in 0..n {
                        w.dispatch(Action::Move(Direction::Right)).unwrap();
                    }
                    let before = w.view(width, 200);
                    let row = before.cursor.unwrap().1;
                    let expected = if down {
                        (row + 1).min(before.lines.len() as u16 - 1)
                    } else {
                        row.saturating_sub(1)
                    };
                    w.handle_event(Event::Key(KeyEvent::new(key, KeyModifiers::NONE)))
                        .unwrap();
                    let after = w.view(width, 200);
                    assert_eq!(
                        after.cursor.unwrap().1,
                        expected,
                        "{key:?}, width {width}, grapheme {n}, before {:?}, after {:?}, text {text:?}",
                        before.cursor,
                        after.cursor
                    );
                }
            }
        }
    }
}

#[test]
fn full_renderer_keeps_vertical_goal_through_blank_short_and_wrapped_lines() {
    use ratatui::{Terminal, backend::TestBackend};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rendered.txt");
    let text = format!(
        "{}\n\n短\n\n{}\n尾行",
        "0123456789".repeat(12),
        "abcdefghij".repeat(14)
    );
    std::fs::write(&path, &text).unwrap();
    for width in [80, 120, 150] {
        let mut w = Workspace::open(&path).unwrap();
        let mut t = Terminal::new(TestBackend::new(width, 40)).unwrap();
        t.draw(|f| w.render(f, "")).unwrap();
        for _ in 0..20 {
            w.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Right,
                KeyModifiers::NONE,
            )))
            .unwrap();
        }
        t.draw(|f| w.render(f, "")).unwrap();
        let origin = t.get_cursor_position().unwrap();
        let mut moves = 0;
        loop {
            let before = t.get_cursor_position().unwrap();
            w.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Down,
                KeyModifiers::SHIFT,
            )))
            .unwrap();
            t.draw(|f| w.render(f, "")).unwrap();
            let after = t.get_cursor_position().unwrap();
            if after.y == before.y {
                break;
            }
            assert_eq!(after.y, before.y + 1);
            moves += 1;
            assert!(moves < 25);
        }
        for _ in 0..moves {
            let before = t.get_cursor_position().unwrap();
            w.handle_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)))
                .unwrap();
            t.draw(|f| w.render(f, "")).unwrap();
            assert_eq!(t.get_cursor_position().unwrap().y, before.y - 1);
        }
        assert_eq!(t.get_cursor_position().unwrap(), origin);
        assert_eq!(w.cursor(), 20);
        assert_eq!(w.selected_text(), "");
        assert_eq!(w.text(), text);
    }
}

#[test]
fn empty_eof_row_is_clickable_and_end_does_not_send_it_to_previous_row() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("full.txt");
    std::fs::write(&path, "abcd").unwrap();
    let mut w = Workspace::open(path).unwrap();
    w.configure_mouse(true);
    w.editor_view(Rect::new(0, 0, 4, 4));
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 1,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert_eq!(w.view(4, 4).cursor, Some((0, 1)));
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(w.view(4, 4).cursor, Some((0, 1)));
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(w.view(4, 4).cursor, Some((0, 0)));
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(w.view(4, 4).cursor, Some((0, 1)));
}

#[test]
fn vertical_navigation_keeps_the_visible_column_after_end_at_a_wrap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wrapped.txt");
    std::fs::write(&path, "abcdEFGH\n0123456").unwrap();
    let mut w = Workspace::open(path).unwrap();
    w.view(4, 20);
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)))
        .unwrap();
    let origin = w.view(4, 20).cursor.unwrap();
    for _ in 0..2 {
        w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
            .unwrap();
        let after = w.view(4, 20).cursor.unwrap();
        assert_eq!(
            after.0, origin.0,
            "vertical motion must use the caret column the writer sees"
        );
    }
}

#[test]
fn vertical_motion_round_trips_with_the_same_insertion_offset() {
    for text in [
        "abcdefghijklm",
        "a甲乙丙丁efgh",
        "0123456\n\nx\nabcdefgh",
        "甲乙\r\n\r\nabcd甲乙",
        "a\te\u{301}👩‍💻XYZ\n\n末尾",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.txt");
        std::fs::write(&path, text).unwrap();
        for width in [1, 2, 3, 4, 5, 8] {
            for n in 0..=text.graphemes(true).count() {
                let mut w = Workspace::open(&path).unwrap();
                w.view(width, 200);
                for _ in 0..n {
                    w.dispatch(Action::Move(Direction::Right)).unwrap();
                }
                let before = w.view(width, 200);
                let row = before.cursor.unwrap().1;
                let offset = w.cursor();
                if row + 1 >= before.lines.len() as u16 {
                    continue;
                }
                w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
                    .unwrap();
                w.view(width, 200);
                w.handle_event(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)))
                    .unwrap();
                assert_eq!(
                    w.view(width, 200).cursor,
                    before.cursor,
                    "screen, width {width}, n {n}, text {text:?}"
                );
                assert_eq!(
                    w.cursor(),
                    offset,
                    "offset, width {width}, n {n}, text {text:?}"
                );
            }
        }
    }
}

#[test]
fn resizing_while_on_a_short_line_does_not_lose_the_vertical_goal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resize.txt");
    std::fs::write(&path, "0123456789\nx\nabcdefghij").unwrap();
    let mut w = Workspace::open(path).unwrap();
    w.view(20, 10);
    for _ in 0..7 {
        w.dispatch(Action::Move(Direction::Right)).unwrap();
    }
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(w.view(20, 10).cursor, Some((1, 1)));
    w.handle_event(Event::Resize(30, 10)).unwrap();
    w.view(30, 10);
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(w.view(30, 10).cursor, Some((7, 2)));
}

#[test]
fn panel_relayout_preserves_goal_and_horizontal_input_sets_a_new_goal() {
    use ratatui::{Terminal, backend::TestBackend};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("panels.txt");
    std::fs::write(&path, "0123456789\nx\nabcdefghij\nABCDEFGHIJ").unwrap();
    let mut w = Workspace::open(path).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    for _ in 0..7 {
        w.dispatch(Action::Move(Direction::Right)).unwrap();
    }
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    w.execute_command("pin-chapters").unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    assert_eq!(
        w.cursor(),
        20,
        "panel width must not replace the remembered column with the short line's end"
    );
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)))
        .unwrap();
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)))
        .unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    assert_eq!(
        w.cursor(),
        30,
        "horizontal navigation deliberately starts a new column goal"
    );
}
