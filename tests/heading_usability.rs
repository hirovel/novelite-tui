use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, ColorMode, Direction, Workspace};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
};

fn draw(w: &mut Workspace, width: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, 30)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn click_label(w: &mut Workspace, b: &Buffer, label: &str) {
    for y in 0..b.area.height {
        for x in 0..b.area.width {
            let row: String = (x..b.area.width).map(|col| b[(col, y)].symbol()).collect();
            if b[(x, y)].symbol() == label.chars().next().unwrap().to_string()
                && row.replace(' ', "").starts_with(label)
            {
                w.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: x,
                    row: y,
                    modifiers: KeyModifiers::NONE,
                }))
                .unwrap();
                return;
            }
        }
    }
    panic!("missing {label}");
}
#[test]
fn visible_paragraph_control_formats_one_line_and_undo_restores_selection() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("保留上段\n雨夜👩‍💻e\u{301}来信".into()))
        .unwrap();
    w.dispatch(Action::Select(Direction::Left)).unwrap();
    let selected = w.selected_text().to_owned();
    let b = draw(&mut w, 80);
    click_label(&mut w, &b, "正文");
    let palette = w.palette().expect("visible style control opens choices");
    assert!(palette.entries.iter().any(|e| e.title.contains("设为标题")));
    assert!(palette.entries.iter().any(|e| e.title.contains("改回正文")));
    key(&mut w, KeyCode::Enter);
    assert_eq!(w.text(), "保留上段\n# 雨夜👩‍💻e\u{301}来信");
    assert_eq!(w.selected_text(), selected);
    assert!(
        draw(&mut w, 80)
            .content
            .iter()
            .any(|c| c.symbol() == "雨" && c.fg == Color::LightBlue)
    );
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), "保留上段\n雨夜👩‍💻e\u{301}来信");
    assert_eq!(w.selected_text(), selected);
    w.dispatch(Action::Redo).unwrap();
    w.dispatch(Action::Save).unwrap();
    assert_eq!(
        std::fs::read_to_string(w.path()).unwrap(),
        "保留上段\n# 雨夜👩‍💻e\u{301}来信"
    );
    w.execute_command("paragraph-body").unwrap();
    assert_eq!(w.text(), "保留上段\n雨夜👩‍💻e\u{301}来信");
    key(&mut w, KeyCode::Char('/'));
    assert!(w.palette().is_none());
}
#[test]
fn wrapped_titles_keep_color_and_tags_or_fenced_code_are_plain() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    let title = format!("{}“蓝光”", "星河".repeat(30));
    w.dispatch(Action::Insert(format!(
        "# {title}\n#标签\n```md\n# 代码\n```\n## 正式标题"
    )))
    .unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Move(Direction::Left)).unwrap();
    for width in [32, 80, 120] {
        let b = draw(&mut w, width);
        let stars: Vec<_> = b.content.iter().filter(|c| c.symbol() == "星").collect();
        assert!(!stars.is_empty());
        assert!(
            stars
                .iter()
                .all(|c| c.fg == Color::LightBlue && c.modifier.contains(Modifier::BOLD))
        );
        assert!(
            b.content
                .iter()
                .filter(|c| c.symbol() == "蓝")
                .all(|c| c.fg == Color::LightBlue)
        );
        for plain in ["签", "代"] {
            assert!(
                b.content
                    .iter()
                    .filter(|c| c.symbol() == plain)
                    .all(|c| c.fg != Color::LightBlue)
            );
        }
    }
    w.set_color_mode(ColorMode::Monochrome).unwrap();
    let b = draw(&mut w, 80);
    assert!(
        b.content
            .iter()
            .filter(|c| c.symbol() == "星")
            .all(|c| c.fg == Color::Reset && c.modifier.contains(Modifier::BOLD))
    );
}
#[test]
fn code_block_rejects_heading_action_without_changing_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    w.dispatch(Action::Insert("```\n代码".into())).unwrap();
    assert!(w.execute_command("paragraph-heading").is_err());
    assert_eq!(w.text(), "```\n代码");
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), "");
}
#[test]
fn chapter_name_is_preserved_but_display_labels_do_not_turn_into_titles() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "测试书").unwrap();
    let id = w.active_chapter().unwrap().id.clone();
    let name = format!("第一章 {}", "银河".repeat(18));
    w.rename_chapter(&id, &name).unwrap();
    w.dispatch(Action::Insert(name.clone())).unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Move(Direction::Left)).unwrap();
    w.configure_mouse(true);
    w.execute_command("split-right").unwrap();
    for width in [32, 80, 120] {
        let b = draw(&mut w, width);
        let title_cells: Vec<_> = b
            .content
            .iter()
            .filter(|c| c.symbol() == "银" && c.modifier.contains(Modifier::BOLD))
            .collect();
        assert!(!title_cells.is_empty());
        assert!(title_cells.iter().all(|c| c.fg == Color::LightBlue));
    }
    w.execute_command("close-pane").unwrap();
    let b = draw(&mut w, 80);
    click_label(&mut w, &b, "章节标题");
    assert!(w.prompt().is_some());
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), name);
    assert!(
        w.execute_command("paragraph-body")
            .unwrap_err()
            .to_string()
            .contains("章节名称")
    );
    assert_eq!(w.active_chapter().unwrap().title, name);
    w.dispatch(Action::Insert("# ".into())).unwrap();
    let b = draw(&mut w, 80);
    click_label(&mut w, &b, "章节标题");
    assert!(w.prompt().is_some());
    key(&mut w, KeyCode::Esc);
    assert!(w.execute_command("paragraph-body").is_err());
    assert_eq!(w.text(), format!("# {name}"));
    let b = draw(&mut w, 32);
    click_label(&mut w, &b, "章名");
    assert!(w.prompt().unwrap().title.contains("正文不替换"));
    let narrow_prompt = draw(&mut w, 32);
    let visible: String = narrow_prompt.content.iter().map(|c| c.symbol()).collect();
    assert!(visible.replace(' ', "").contains("正文不替换"));
    w.handle_event(Event::Paste("新的目录章名".into())).unwrap();
    key(&mut w, KeyCode::Enter);
    assert_eq!(w.active_chapter().unwrap().title, "新的目录章名");
    assert_eq!(w.text(), format!("# {name}"));
    let mut file = Workspace::open(dir.path().join("single.txt")).unwrap();
    file.dispatch(Action::Insert("文稿".into())).unwrap();
    let b = draw(&mut file, 80);
    assert!(
        b.content
            .iter()
            .filter(|c| c.symbol() == "稿")
            .all(|c| c.fg != Color::LightBlue)
    );
}
#[test]
fn fence_edit_and_undo_reclassify_following_lines_without_changing_other_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    w.dispatch(Action::Insert("```\r\n# 围栏内\r\n```\r\n# 围栏外".into()))
        .unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Move(Direction::Left)).unwrap();
    let b = draw(&mut w, 80);
    assert!(
        b.content
            .iter()
            .filter(|c| c.symbol() == "内")
            .all(|c| c.fg != Color::LightBlue)
    );
    w.dispatch(Action::Select(Direction::Right)).unwrap();
    w.dispatch(Action::Select(Direction::Right)).unwrap();
    w.dispatch(Action::Select(Direction::Right)).unwrap();
    w.dispatch(Action::Delete).unwrap();
    let b = draw(&mut w, 80);
    assert!(
        b.content
            .iter()
            .any(|c| c.symbol() == "内" && c.fg == Color::LightBlue)
    );
    assert!(
        b.content
            .iter()
            .filter(|c| c.symbol() == "外")
            .all(|c| c.fg != Color::LightBlue)
    );
    w.dispatch(Action::Undo).unwrap();
    let b = draw(&mut w, 80);
    assert!(
        b.content
            .iter()
            .any(|c| c.symbol() == "外" && c.fg == Color::LightBlue)
    );
    assert_eq!(w.text(), "```\r\n# 围栏内\r\n```\r\n# 围栏外");
}

#[test]
fn heading_edit_keeps_both_pane_selections_through_resize_and_undo() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    let prose = format!("上段\n{}尾声", "星河".repeat(40));
    w.dispatch(Action::Insert(prose.clone())).unwrap();
    w.dispatch(Action::Select(Direction::Left)).unwrap();
    w.dispatch(Action::Select(Direction::Left)).unwrap();
    w.execute_command("split-right").unwrap();
    w.execute_command("paragraph-heading").unwrap();
    for width in [120, 32, 80, 160] {
        let b = draw(&mut w, width);
        assert_eq!(w.selected_text(), "尾声");
        assert!(
            b.content
                .iter()
                .any(|c| c.symbol() == "星" && c.fg == Color::LightBlue)
        );
    }
    w.execute_command("focus-other-pane").unwrap();
    assert_eq!(w.selected_text(), "尾声");
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), prose);
    assert_eq!(w.selected_text(), "尾声");
    w.execute_command("focus-other-pane").unwrap();
    assert_eq!(w.selected_text(), "尾声");
    w.dispatch(Action::Redo).unwrap();
    assert_eq!(w.text(), prose.replacen("\n", "\n# ", 1));
    assert_eq!(w.selected_text(), "尾声");
}
