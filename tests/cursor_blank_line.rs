use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::Workspace;
use ratatui::layout::Rect;

fn fixture() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blank.txt");
    std::fs::write(&path, "0123456789\n\nabcdefghij").unwrap();
    let mut w = Workspace::open(path).unwrap();
    w.configure_mouse(true);
    w.editor_view(Rect::new(0, 0, 40, 10));
    (dir, w)
}
fn press(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn click(w: &mut Workspace, column: u16, row: u16) {
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        w.handle_event(Event::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }
}

#[test]
fn clicking_blank_line_start_then_up_reaches_previous_line_start() {
    let (_dir, mut w) = fixture();
    click(&mut w, 0, 1);
    assert_eq!(w.view(40, 10).cursor, Some((0, 1)));
    press(&mut w, KeyCode::Up);
    assert_eq!(w.view(40, 10).cursor, Some((0, 0)));
    assert_eq!(w.cursor(), 0);
}

#[test]
fn keyboard_crossing_a_blank_line_keeps_the_preceding_column() {
    let (_dir, mut w) = fixture();
    click(&mut w, 7, 0);
    press(&mut w, KeyCode::Down);
    assert_eq!(w.view(40, 10).cursor, Some((0, 1)));
    press(&mut w, KeyCode::Down);
    assert_eq!(w.view(40, 10).cursor, Some((7, 2)));
    press(&mut w, KeyCode::Up);
    assert_eq!(w.view(40, 10).cursor, Some((0, 1)));
    press(&mut w, KeyCode::Up);
    assert_eq!(w.view(40, 10).cursor, Some((7, 0)));
    assert_eq!(w.cursor(), 7);
}

#[test]
fn clicking_right_of_an_empty_line_clamps_to_its_only_insertion_position() {
    let (_dir, mut w) = fixture();
    click(&mut w, 7, 0);
    press(&mut w, KeyCode::Down);
    click(&mut w, 7, 1);
    assert_eq!(w.view(40, 10).cursor, Some((0, 1)));
    press(&mut w, KeyCode::Up);
    assert_eq!(w.view(40, 10).cursor, Some((0, 0)));
    assert_eq!(
        w.cursor(),
        0,
        "a fresh mouse placement resets the former keyboard column goal"
    );
}

#[test]
fn rendered_chinese_text_click_and_up_survive_padding_wrapping_and_scroll() {
    use novelite_tui::{Action, Direction};
    use ratatui::{Terminal, backend::TestBackend};
    use unicode_segmentation::UnicodeSegmentation;

    let ending = "他看了一眼墙上的钟。十一点四十七分，秒针已经停了。\n\n林舟把信放进口袋，拿起伞。楼梯上响起脚步声，他却没有回头。\n\n有些故事，必须回到开始的地方才能写完。\n";
    for (width, height) in [(80, 12), (120, 24), (150, 42)] {
        for scrolled in [false, true] {
            let prefix = if scrolled {
                "较早的段落用于把文稿滚动到后面。\n\n".repeat(30)
            } else {
                String::new()
            };
            let text = format!("{prefix}{ending}");
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("chinese.txt");
            std::fs::write(&path, &text).unwrap();
            let mut w = Workspace::open(path).unwrap();
            w.configure_mouse(true);
            w.set_design_motion(false);
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| w.render(f, "")).unwrap();
            for _ in text.graphemes(true) {
                w.dispatch(Action::Move(Direction::Right)).unwrap();
            }
            t.draw(|f| w.render(f, "")).unwrap();
            let buffer = t.backend().buffer();
            let index = buffer
                .content
                .iter()
                .position(|c| c.symbol() == "始")
                .expect("last paragraph must be visible");
            let point = (
                (index % width as usize) as u16,
                (index / width as usize) as u16,
            );
            click(&mut w, point.0, point.1);
            t.draw(|f| w.render(f, "")).unwrap();
            assert_eq!(
                w.cursor(),
                text.rfind('始').unwrap(),
                "actual rendered click: {width}x{height}, scrolled {scrolled}"
            );
            let origin = t.get_cursor_position().unwrap();
            assert_eq!(
                (origin.x, origin.y),
                point,
                "click must leave the caret on the clicked glyph"
            );
            press(&mut w, KeyCode::Up);
            t.draw(|f| w.render(f, "")).unwrap();
            let empty = t.get_cursor_position().unwrap();
            assert!(
                empty.x < origin.x,
                "the empty line only has one insertion position"
            );
            assert_eq!(empty.y + 1, origin.y);
            press(&mut w, KeyCode::Up);
            t.draw(|f| w.render(f, "")).unwrap();
            assert_eq!(
                t.get_cursor_position().unwrap().x,
                origin.x,
                "preferred column must return beyond the empty line"
            );
            press(&mut w, KeyCode::Down);
            press(&mut w, KeyCode::Down);
            t.draw(|f| w.render(f, "")).unwrap();
            assert_eq!(w.cursor(), text.rfind('始').unwrap());
            assert_eq!(t.get_cursor_position().unwrap(), origin);
            assert_eq!(w.text(), text);
        }
    }
}

#[test]
fn clicked_chinese_column_survives_blank_short_and_wrapped_rows_in_both_directions() {
    let text = "甲乙丙丁戊己庚辛壬癸子丑\n\n短句\n\n天地玄黄宇宙洪荒日月盈昃";
    // At width 12 each long physical line has two visual rows. The other
    // widths keep it on one row. Coordinates are relative to the editor.
    for (width, lower_long_row, upper_long_row, expected_upper_character) in
        [(12, 5, 1, '子'), (40, 4, 0, '戊'), (80, 4, 0, '戊')]
    {
        for clicked_column in [8, 9] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("wide-characters.txt");
            std::fs::write(&path, text).unwrap();
            let mut w = Workspace::open(path).unwrap();
            w.configure_mouse(true);
            w.editor_view(Rect::new(0, 0, width, 20));
            click(&mut w, clicked_column, lower_long_row);
            assert_eq!(w.cursor(), text.find('宇').unwrap());
            assert_eq!(w.view(width, 20).cursor, Some((8, lower_long_row)));

            press(&mut w, KeyCode::Up);
            assert_eq!(w.view(width, 20).cursor, Some((0, lower_long_row - 1)));
            press(&mut w, KeyCode::Up);
            assert_eq!(w.view(width, 20).cursor, Some((4, lower_long_row - 2)));
            assert_eq!(w.cursor(), text.find("短句").unwrap() + "短句".len());
            press(&mut w, KeyCode::Up);
            assert_eq!(w.view(width, 20).cursor, Some((0, lower_long_row - 3)));
            press(&mut w, KeyCode::Up);
            assert_eq!(w.view(width, 20).cursor, Some((8, upper_long_row)));
            assert_eq!(w.cursor(), text.find(expected_upper_character).unwrap());

            for _ in 0..4 {
                press(&mut w, KeyCode::Down);
                w.view(width, 20);
            }
            assert_eq!(w.view(width, 20).cursor, Some((8, lower_long_row)));
            assert_eq!(w.cursor(), text.find('宇').unwrap());
            assert_eq!(w.text(), text);
        }
    }
}
