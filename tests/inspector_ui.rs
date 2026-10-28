use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, ColorMode, Direction, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn text(b: &Buffer) -> String {
    b.content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .replace(' ', "")
}
fn points(b: &Buffer, symbol: &str) -> Vec<(u16, u16)> {
    b.content
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            *i / usize::from(b.area.width) >= usize::from(b.area.height / 2) && c.symbol() == symbol
        })
        .map(|(i, _)| {
            (
                (i % usize::from(b.area.width)) as u16,
                (i / usize::from(b.area.width)) as u16,
            )
        })
        .collect()
}
fn click(w: &mut Workspace, point: (u16, u16)) {
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: point.0,
        row: point.1,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}

#[test]
fn filtered_explanation_and_real_keymap_follow_selection_without_editing_the_draft() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.load_keymap(dir.path().join("keys.json"));
    w.dispatch(Action::Insert("中/文👩‍💻e\u{301}\n另起一行".into()))
        .unwrap();
    w.dispatch(Action::Select(Direction::Left)).unwrap();
    let selection = w.selected_text().to_owned();
    w.set_keybinding("F4", "save").unwrap();
    w.set_keybinding("F6", "design-settings").unwrap();
    key(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("design-settings".into()))
        .unwrap();
    let wide = draw(&mut w, 120, 30);
    let visible = text(&wide);
    assert!(visible.contains("调整排版、布局与写作偏好"));
    assert!(visible.contains("F6"));
    assert!(
        !visible.contains("F4"),
        "explanation must use the customized keymap"
    );
    assert_eq!(
        points(&wide, "设").len(),
        2,
        "selected row and its read-only heading"
    );
    let compact = draw(&mut w, 80, 24);
    assert_eq!(points(&compact, "设").len(), 1);
    assert!(!text(&compact).contains("调整排版、布局与写作偏好"));
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.selected_text(), selection);
    key(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("没有这个操作👩‍💻".into()))
        .unwrap();
    assert!(text(&draw(&mut w, 120, 30)).contains("没有匹配的操作"));
    assert_eq!(points(&draw(&mut w, 120, 30), "▎").len(), 0);
    key(&mut w, KeyCode::Enter);
    assert!(w.palette().is_some());
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.selected_text(), selection);
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), "");
}

#[test]
fn inspector_is_read_only_and_resizing_rebuilds_list_mouse_targets() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("正文/保持原样".into())).unwrap();
    key(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("design-settings".into()))
        .unwrap();
    let wide = draw(&mut w, 120, 30);
    let labels = points(&wide, "设");
    let detail = *labels.iter().max_by_key(|p| p.0).unwrap();
    click(&mut w, detail);
    assert!(w.palette().is_some());
    assert!(!w.design_settings_open());
    let narrow = draw(&mut w, 80, 24);
    click(&mut w, detail);
    assert!(
        w.palette().is_some(),
        "old detail area must not become a list hit"
    );
    click(&mut w, points(&narrow, "设")[0]);
    assert!(w.design_settings_open());
    assert!(w.palette().is_none());
    key(&mut w, KeyCode::Esc);
    key(&mut w, KeyCode::Char('/'));
    assert_eq!(w.text(), "正文/保持原样/");
}

#[test]
fn settings_remain_operable_in_all_categories_without_rendering_preferences_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let layout = dir.path().join("layout.json");
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.load_layout(&layout).unwrap();
    w.set_color_mode(ColorMode::Monochrome).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("未保存正文/👩‍💻".into())).unwrap();
    key(&mut w, KeyCode::F(4));
    let original = std::fs::read(&layout).unwrap();
    let modified = std::fs::metadata(&layout).unwrap().modified().unwrap();
    for title in ["正文宽度", "分栏", "对话高亮"] {
        for (width, height) in [(120, 30), (80, 24), (60, 24), (40, 12)] {
            let b = draw(&mut w, width, height);
            assert!(
                text(&b).contains(title),
                "{width}×{height}: missing {title}"
            );
            assert!(text(&b).contains("Esc返回正文"));
            assert!(
                b.content
                    .iter()
                    .all(|c| c.bg == Color::Reset && c.fg == Color::Reset)
            );
        }
        key(&mut w, KeyCode::Tab);
    }
    assert_eq!(std::fs::read(&layout).unwrap(), original);
    assert_eq!(
        std::fs::metadata(&layout).unwrap().modified().unwrap(),
        modified
    );
    draw(&mut w, 60, 24);
    key(&mut w, KeyCode::Right);
    assert!(text(&draw(&mut w, 60, 24)).contains("宽幅"));
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&layout).unwrap()).unwrap();
    assert_eq!(saved["appearance"]["text_width"], "wide");
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), "未保存正文/👩‍💻");
    key(&mut w, KeyCode::F(1));
    w.handle_event(Event::Paste("save".into())).unwrap();
    assert!(text(&draw(&mut w, 40, 12)).contains("保存文稿"));
    key(&mut w, KeyCode::Enter);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("draft.txt")).unwrap(),
        w.text()
    );
}
