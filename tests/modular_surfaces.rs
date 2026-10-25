use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, Brightness, ColorTheme, Material, Workspace};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    style::{Color, Modifier},
};
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn at(b: &Buffer, glyph: &str) -> (u16, u16) {
    let i = b
        .content
        .iter()
        .position(|c| c.symbol() == glyph && !c.modifier.contains(Modifier::DIM))
        .unwrap_or_else(|| panic!("missing {glyph}"));
    (
        (i % b.area.width as usize) as u16,
        (i / b.area.width as usize) as u16,
    )
}
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn click(w: &mut Workspace, b: &Buffer, glyph: &str) {
    let (column, row) = at(b, glyph);
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}

#[test]
fn drawer_reflows_and_brightness_material_survive_reopening_without_changing_text() {
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book");
    let layout = dir.path().join("layout.json");
    let mut w = Workspace::create_project(&book, "夜航").unwrap();
    w.load_layout(&layout).unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("原稿不因自动排版而改变。".into()))
        .unwrap();
    w.dispatch(Action::Save).unwrap();
    w.open_design_settings().unwrap();
    let wide = draw(&mut w, 120, 40);
    let a = at(&wide, "阅");
    let b = at(&wide, "舒");
    assert_eq!(a.0, b.0);
    assert!(a.1 < b.1);
    assert!(a.1 > 20, "settings must be docked beneath the manuscript");
    let narrow = draw(&mut w, 80, 24);
    let a = at(&narrow, "阅");
    let b = at(&narrow, "舒");
    assert_eq!(a.0, b.0);
    assert!(a.1 < b.1);
    click(&mut w, &narrow, "阅");
    key(&mut w, KeyCode::Right); // full
    key(&mut w, KeyCode::Tab); // layout
    let layout_panel = draw(&mut w, 120, 40);
    click(&mut w, &layout_panel, "单");
    assert!(w.is_split());
    // Advanced color controls remain available through the public appearance API.
    w.set_brightness(Brightness::Soft).unwrap();
    w.set_material(Material::Terminal).unwrap();
    key(&mut w, KeyCode::Esc);
    let transparent = draw(&mut w, 120, 40);
    assert_eq!(transparent[(0, 0)].bg, Color::Reset);
    w.set_material(Material::Solid).unwrap();
    let soft = draw(&mut w, 120, 40);
    w.set_brightness(Brightness::Bright).unwrap();
    let bright = draw(&mut w, 120, 40);
    assert_ne!(soft[(0, 0)].bg, bright[(0, 0)].bg);
    w.set_material(Material::Terminal).unwrap();
    drop(w);
    let mut w = Workspace::open_project(book).unwrap();
    w.load_layout(&layout).unwrap();
    assert_eq!(w.text(), "原稿不因自动排版而改变。");
    assert_eq!(draw(&mut w, 120, 40)[(0, 0)].bg, Color::Reset);
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(layout).unwrap()).unwrap();
    assert_eq!(config["appearance"]["brightness"], "bright");
    assert_eq!(config["appearance"]["material"], "terminal");
    assert_eq!(config["appearance"]["text_width"], "full");
}

#[test]
fn last_settings_card_stays_visible_and_clickable_after_narrowing() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "布局").unwrap();
    w.configure_mouse(true);
    w.open_design_settings().unwrap();
    key(&mut w, KeyCode::Tab);
    for _ in 0..4 {
        key(&mut w, KeyCode::Down);
        draw(&mut w, 120, 40);
    }
    let b = draw(&mut w, 50, 24);
    at(&b, "纲");
    let previous = w
        .panel_preferences(novelite_tui::ModuleId::Planning)
        .unwrap()
        .width;
    key(&mut w, KeyCode::Left);
    assert!(
        w.panel_preferences(novelite_tui::ModuleId::Planning)
            .unwrap()
            .width
            < previous
    );
    assert_eq!(w.text(), "");
}

#[test]
fn command_search_wheel_and_close_keep_editor_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "搜索").unwrap();
    w.configure_mouse(true);
    key(&mut w, KeyCode::F(1));
    draw(&mut w, 80, 24);
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 10,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert_eq!(w.palette().unwrap().selected, 1);
    w.handle_event(Event::Paste("不存在的命令👩‍💻".repeat(20)))
        .unwrap();
    let b = draw(&mut w, 50, 20);
    at(&b, "没");
    click(&mut w, &b, "关");
    assert!(w.palette().is_none());
    assert_eq!(w.text(), "");
    key(&mut w, KeyCode::Char('雨'));
    assert_eq!(w.text(), "雨");
}
