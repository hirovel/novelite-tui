use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, ColorTheme, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Modifier};
use std::time::{Duration, Instant};
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render_design(f, "", Instant::now() + Duration::from_secs(1)))
        .unwrap();
    t.backend().buffer().clone()
}
fn click(w: &mut Workspace, b: &Buffer, glyph: &str) {
    let i = b
        .content
        .iter()
        .enumerate()
        .find(|(i, c)| {
            *i / b.area.width as usize >= b.area.height as usize / 2
                && c.symbol() == glyph
                && !c.modifier.contains(Modifier::DIM)
        })
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("missing visible {glyph}"));
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: i as u16 % b.area.width,
        row: i as u16 / b.area.width,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}
#[test]
fn clickable_settings_apply_layout_and_restore_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book");
    let layout = dir.path().join("layout.json");
    let mut w = Workspace::create_project(&book, "体验").unwrap();
    w.load_layout(&layout).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("正文不能被设置键污染。".into()))
        .unwrap();
    w.dispatch(Action::Save).unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    let b = draw(&mut w, 120, 40);
    // The only footer action opens commands; settings are selected inside that surface.
    let i = b.content.iter().rposition(|c| c.symbol() == "命").unwrap();
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: i as u16 % b.area.width,
        row: i as u16 / b.area.width,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert!(w.palette().is_some());
    let b = draw(&mut w, 120, 40);
    click(&mut w, &b, "设");
    assert!(w.design_settings_open());
    let b = draw(&mut w, 120, 40);
    click(&mut w, &b, "阅"); // reading -> wide
    key(&mut w, KeyCode::Left); // wide -> reading
    key(&mut w, KeyCode::Left); // reading -> full
    key(&mut w, KeyCode::Down);
    draw(&mut w, 120, 40);
    key(&mut w, KeyCode::Left); // comfortable -> compact
    key(&mut w, KeyCode::Char('x'));
    key(&mut w, KeyCode::Tab); // layout
    key(&mut w, KeyCode::Tab); // writing
    for _ in 0..3 {
        key(&mut w, KeyCode::Down);
    }
    draw(&mut w, 120, 40);
    key(&mut w, KeyCode::Enter); // disable motion, persisted
    assert!(!w.design_motion_enabled());
    key(&mut w, KeyCode::Esc);
    assert!(!w.design_settings_open());
    assert_eq!(w.text(), "正文不能被设置键污染。");
    let r = w.editor_area(ratatui::layout::Rect::new(0, 0, 120, 40));
    assert_eq!(r.width, 120);
    let cfg: serde_json::Value = serde_json::from_slice(&std::fs::read(&layout).unwrap()).unwrap();
    assert_eq!(cfg["appearance"]["text_width"], "full");
    assert_eq!(cfg["appearance"]["density"], "compact");
    drop(w);
    let mut again = Workspace::open_project(&book).unwrap();
    again.load_layout(&layout).unwrap();
    assert!(!again.design_motion_enabled());
    assert_eq!(
        again
            .editor_area(ratatui::layout::Rect::new(0, 0, 120, 40))
            .width,
        120
    );
}
#[test]
fn palette_mouse_executes_settings_and_small_settings_never_edit_manuscript() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "会话").unwrap();
    w.configure_mouse(true);
    w.set_color_theme(ColorTheme::Night).unwrap();
    key(&mut w, KeyCode::F(1));
    let b = draw(&mut w, 80, 24);
    click(&mut w, &b, "设");
    assert!(w.design_settings_open());
    for (width, height) in [(32, 10), (16, 6), (1, 1), (80, 24)] {
        draw(&mut w, width, height);
    }
    w.handle_event(Event::Paste("不应进入正文".into())).unwrap();
    assert_eq!(w.text(), "");
    key(&mut w, KeyCode::Tab);
    draw(&mut w, 80, 24);
    key(&mut w, KeyCode::Enter);
    assert!(w.is_split());
    key(&mut w, KeyCode::Esc);
    assert!(!w.design_settings_open());
    key(&mut w, KeyCode::Char('字'));
    assert_eq!(w.text(), "字");
}
#[test]
fn real_event_routing_starts_and_cancels_visible_transition() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "动画").unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.set_design_motion(true);
    draw(&mut w, 100, 30);
    key(&mut w, KeyCode::F(4));
    assert!(w.design_needs_frame());
    let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let now = Instant::now();
    t.draw(|f| w.render_design(f, "", now)).unwrap();
    let start = t.backend().buffer().clone();
    t.draw(|f| w.render_design(f, "", now + Duration::from_millis(250)))
        .unwrap();
    assert_ne!(start, *t.backend().buffer());
    assert!(!w.design_needs_frame());
    key(&mut w, KeyCode::Esc);
    assert!(w.design_needs_frame());
    key(&mut w, KeyCode::Char('雨'));
    assert!(!w.design_needs_frame());
    assert_eq!(w.text(), "雨");
}
#[test]
fn shrink_removes_toolbar_hits_and_hidden_settings_cannot_apply() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "缩放").unwrap();
    w.configure_mouse(true);
    w.set_color_theme(ColorTheme::Night).unwrap();
    let large = draw(&mut w, 120, 40);
    let i = large
        .content
        .iter()
        .position(|c| c.symbol() == "命")
        .unwrap();
    draw(&mut w, 120, 9);
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: i as u16 % 120,
        row: i as u16 / 120,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert!(w.palette().is_none());
    assert!(!w.design_settings_open());
    key(&mut w, KeyCode::F(4));
    // Below the compact minimum there are no actionable settings rows.
    draw(&mut w, 120, 6);
    key(&mut w, KeyCode::Down);
    key(&mut w, KeyCode::Enter);
    key(&mut w, KeyCode::Esc);
    let night = draw(&mut w, 120, 40);
    assert_eq!(large.content[0].bg, night.content[0].bg);
    w.execute_command("background-light").unwrap();
    let light = draw(&mut w, 120, 40);
    assert_ne!(night.content[0].bg, light.content[0].bg);
    w.execute_command("background-dark").unwrap();
    assert_eq!(draw(&mut w, 120, 40).content[0].bg, night.content[0].bg);
}
struct Delayed(std::sync::mpsc::Receiver<String>);
impl novelite_tui::Clipboard for Delayed {
    fn read_text(&mut self) -> std::io::Result<String> {
        self.0.recv().map_err(std::io::Error::other)
    }
    fn write_text(&mut self, _: &str) -> std::io::Result<()> {
        self.0.recv().map(|_| ()).map_err(std::io::Error::other)
    }
}
#[test]
fn pending_clipboard_never_edits_through_settings_even_after_closing() {
    for close_first in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::create_project(dir.path().join("book"), "剪贴板").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        w.set_clipboard(Box::new(Delayed(rx))).unwrap();
        w.execute_command("paste").unwrap();
        key(&mut w, KeyCode::F(4));
        if close_first {
            key(&mut w, KeyCode::Esc);
        }
        tx.send("延迟返回的文字".into()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while w.clipboard_busy() {
            w.tick(Instant::now()).unwrap();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(w.text(), "");
        assert!(w.clipboard_message().unwrap().contains("未自动粘贴"));
    }
}

fn screen_text(buffer: &Buffer) -> String {
    buffer
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
        .replace(' ', "")
}
#[test]
fn compact_properties_remain_reachable_and_distinct_from_category_navigation() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.open_design_settings().unwrap();
    for (first, last, count) in [
        ("正文宽度", "页面留白", 2),
        ("分栏", "大纲宽度", 5),
        ("对话高亮", "界面过渡", 4),
    ] {
        let b = draw(&mut w, 120, 40);
        assert!(screen_text(&b).contains(first));
        assert!(!screen_text(&b).contains("即时生效"));
        for _ in 1..count {
            key(&mut w, KeyCode::Down);
        }
        for (width, height) in [(120, 40), (80, 24), (32, 10)] {
            let b = draw(&mut w, width, height);
            assert!(screen_text(&b).contains(last));
            assert_eq!(b.content.iter().filter(|c| c.symbol() == "▎").count(), 1);
        }
        key(&mut w, KeyCode::Tab);
    }
    let b = draw(&mut w, 120, 40);
    assert!(screen_text(&b).contains("正文宽度阅读"));
    key(&mut w, KeyCode::Right);
    assert!(screen_text(&draw(&mut w, 120, 40)).contains("正文宽度宽幅"));
    key(&mut w, KeyCode::BackTab);
    assert!(screen_text(&draw(&mut w, 120, 40)).contains("对话高亮"));
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), "");
}
#[test]
fn failed_setting_write_keeps_the_actual_value_visible_and_draft_intact() {
    let dir = tempfile::tempdir().unwrap();
    let layout = dir.path().join("layout.json");
    let mut w = Workspace::open(dir.path().join("draft.txt")).unwrap();
    w.load_layout(&layout).unwrap();
    w.dispatch(Action::Insert("未保存正文".into())).unwrap();
    w.open_design_settings().unwrap();
    draw(&mut w, 120, 40);
    std::fs::create_dir(&layout).unwrap();
    let result = w.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE,
    )));
    assert!(result.is_err());
    assert!(screen_text(&draw(&mut w, 120, 40)).contains("正文宽度阅读"));
    assert_eq!(w.text(), "未保存正文");
}
