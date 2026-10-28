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
fn draw(w: &mut Workspace, width: u16, height: u16, now: Instant) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| w.render_design(f, "", now)).unwrap();
    t.backend().buffer().clone()
}
fn point(b: &Buffer, symbol: &str, start_y: u16) -> (u16, u16) {
    let index = b
        .content
        .iter()
        .enumerate()
        .find(|(i, c)| *i / b.area.width as usize >= start_y as usize && c.symbol() == symbol)
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("missing {symbol}"));
    (
        (index % b.area.width as usize) as u16,
        (index / b.area.width as usize) as u16,
    )
}
fn click(w: &mut Workspace, p: (u16, u16)) {
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: p.0,
        row: p.1,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}
fn row(b: &Buffer, y: u16) -> String {
    (0..b.area.width)
        .map(|x| b[(x, y)].symbol())
        .collect::<String>()
        .replace(' ', "")
}

#[test]
fn bottom_entry_switches_modes_without_dimming_manuscript_or_leaving_old_footer() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "夜航").unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.set_design_motion(false);
    w.configure_mouse(true);
    w.dispatch(Action::Insert("独立正文保持清晰。".into()))
        .unwrap();
    let now = Instant::now();
    let before = draw(&mut w, 120, 40, now);
    let body_style = before[point(&before, "独", 0)].clone();
    click(&mut w, point(&before, "命", 39));
    let commands = draw(&mut w, 120, 40, now);
    let query_y = point(&commands, "›", 20).1;
    assert_eq!(row(&commands, query_y), "›搜索全部操作…");
    assert!(row(&commands, 39).ends_with("Esc返回正文"));
    click(&mut w, point(&commands, "设", 20));
    assert!(w.design_settings_open());
    assert!(w.palette().is_none());
    let settings = draw(&mut w, 120, 40, now);
    assert_eq!(point(&commands, "命", 20).1, point(&settings, "设", 20).1);
    let body = &settings[point(&settings, "独", 0)];
    assert_eq!((body.fg, body.bg), (body_style.fg, body_style.bg));
    assert!(!body.modifier.contains(Modifier::DIM));
    assert!(row(&settings, 39).starts_with("›命令"));
    click(&mut w, point(&settings, "命", 39));
    assert!(!w.design_settings_open());
    assert!(w.palette().is_some());
    key(&mut w, KeyCode::Esc);
    key(&mut w, KeyCode::Char('/'));
    assert_eq!(w.text(), "独立正文保持清晰。/");
    assert!(
        w.palette().is_none(),
        "literal slash must remain normal writing input"
    );
}

#[test]
fn drawer_motion_keeps_text_and_click_targets_final_from_first_frame() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "过渡").unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.set_design_motion(true);
    w.configure_mouse(true);
    w.dispatch(Action::Insert("正文始终清晰".into())).unwrap();
    let now = Instant::now();
    draw(&mut w, 120, 40, now);
    w.open_design_settings().unwrap();
    w.begin_design_transition(now);
    let first = draw(&mut w, 120, 40, now);
    let middle = draw(&mut w, 120, 40, now + Duration::from_millis(70));
    let settled = draw(&mut w, 120, 40, now + Duration::from_millis(250));
    assert_eq!(
        first.content.iter().map(|c| c.symbol()).collect::<Vec<_>>(),
        settled
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
    );
    assert_ne!(first, middle);
    assert_ne!(middle, settled);
    assert!(!w.design_needs_frame());
    for y in 0..24 {
        for x in 0..120 {
            assert_eq!(first[(x, y)], settled[(x, y)], "body must not fade");
        }
    }
    let previous_width = w
        .editor_area(ratatui::layout::Rect::new(0, 0, 120, 40))
        .width;
    let full = point(&first, "阅", 20);
    assert_eq!(full, point(&settled, "阅", 20));
    w.begin_design_transition(now + Duration::from_millis(260));
    draw(&mut w, 120, 40, now + Duration::from_millis(260));
    click(&mut w, full);
    let changed = draw(&mut w, 120, 40, Instant::now() + Duration::from_secs(1));
    assert_eq!(changed[(0, 0)].bg, settled[(0, 0)].bg);
    assert!(
        w.editor_area(ratatui::layout::Rect::new(0, 0, 120, 40))
            .width
            > previous_width
    );
    assert_eq!(w.text(), "正文始终清晰");
}

#[test]
fn settings_can_return_to_commands_by_existing_keys_and_resize_discards_stale_hits() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "缩放").unwrap();
    w.configure_mouse(true);
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.set_design_motion(false);
    let now = Instant::now();
    w.open_design_settings().unwrap();
    let large = draw(&mut w, 120, 40, now);
    let stale_full = point(&large, "阅", 20);
    draw(&mut w, 80, 24, now);
    click(&mut w, stale_full);
    let small = draw(&mut w, 80, 24, now);
    assert_eq!(small[(0, 0)].bg, large[(0, 0)].bg);
    key(&mut w, KeyCode::F(1));
    assert!(w.palette().is_some());
    assert!(!w.design_settings_open());
    w.handle_event(Event::Paste("design-settings".into()))
        .unwrap();
    key(&mut w, KeyCode::Enter);
    assert!(w.design_settings_open());
    w.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert!(w.palette().is_some());
    let b = draw(&mut w, 80, 24, now);
    assert_eq!(row(&b, point(&b, "›", 12).1), "›搜索全部操作…");
    assert!(row(&b, 23).ends_with("Esc返回正文"));
    key(&mut w, KeyCode::Down);
    // Selection is a motion scene, while typing immediately interrupts it.
    w.set_design_motion(true);
    // Re-enabling starts with the next visible frame, not disabled-frame history.
    draw(&mut w, 80, 24, now);
    key(&mut w, KeyCode::Down);
    assert!(w.design_needs_frame());
    key(&mut w, KeyCode::Char('保'));
    assert!(
        !w.design_needs_frame(),
        "search input must interrupt the focus animation immediately"
    );
    draw(&mut w, 80, 24, now + Duration::from_secs(1));
    assert!(!w.design_needs_frame());
    for (width, height) in [(24, 1), (1, 1), (32, 10), (160, 48)] {
        draw(&mut w, width, height, now + Duration::from_secs(1));
    }
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), "");
}

#[test]
fn slash_typing_pasting_and_escape_are_plain_editing_until_explicit_command_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.txt");
    let mut w = Workspace::open(&path).unwrap();
    for text in ["/开头", "https://example.com/a/b", "中/文/"] {
        for ch in text.chars() {
            key(&mut w, KeyCode::Char(ch));
            assert!(w.palette().is_none());
        }
    }
    w.handle_event(Event::Paste("\n/设置 /save /night\n".into()))
        .unwrap();
    let original = w.text().to_owned();
    key(&mut w, KeyCode::Esc);
    key(&mut w, KeyCode::Esc);
    assert!(
        w.palette().is_none(),
        "Esc only dismisses; it must not enter command mode"
    );
    w.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    key(&mut w, KeyCode::Char('/'));
    assert_eq!(w.palette().unwrap().query, "/");
    assert_eq!(w.text(), original);
    key(&mut w, KeyCode::Esc);
    key(&mut w, KeyCode::Char('/'));
    assert_eq!(w.text(), format!("{original}/"));
    w.dispatch(Action::Save).unwrap();
    drop(w);
    assert_eq!(
        Workspace::open(path).unwrap().text(),
        format!("{original}/")
    );
}

#[test]
fn resized_solid_canvas_fills_every_cell_outside_the_manuscript() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "背景").unwrap();
    for theme in [ColorTheme::Night, ColorTheme::Forest, ColorTheme::Paper] {
        w.set_color_theme(theme).unwrap();
        let mut t = Terminal::new(TestBackend::new(150, 42)).unwrap();
        for (width, height) in [(150, 42), (151, 43), (71, 19), (180, 50), (1, 1)] {
            t.backend_mut().resize(width, height);
            t.draw(|f| w.render(f, "")).unwrap();
            let b = t.backend().buffer();
            let background = b[(0, 0)].bg;
            assert_ne!(background, ratatui::style::Color::Reset);
            // A wide glyph paints its continuation column in the terminal;
            // TestBackend intentionally leaves that backing cell reset.
            for y in 0..height {
                let mut x = 0;
                while x < width {
                    let cell = &b[(x, y)];
                    assert_eq!(
                        cell.bg, background,
                        "unpainted cell at {x},{y} in {width}x{height}"
                    );
                    x += unicode_width::UnicodeWidthStr::width(cell.symbol()).max(1) as u16;
                }
            }
        }
    }
}

#[test]
fn terminal_material_leaves_canvas_and_drawers_background_to_the_host() {
    use novelite_tui::Material;
    use ratatui::style::Color;
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "终端背景").unwrap();
    w.set_material(Material::Terminal).unwrap();
    w.set_design_motion(false);
    for theme in [ColorTheme::Night, ColorTheme::Forest, ColorTheme::Paper] {
        w.set_color_theme(theme).unwrap();
        for (width, height) in [(150, 42), (80, 24), (32, 10)] {
            for mode in 0..3 {
                if mode == 1 {
                    w.open_design_settings().unwrap();
                }
                if mode == 2 {
                    w.execute_command("design-commands").unwrap();
                }
                let b = draw(&mut w, width, height, Instant::now());
                for (i, cell) in b.content.iter().enumerate() {
                    assert_eq!(
                        cell.bg,
                        Color::Reset,
                        "explicit background at {i} in mode {mode}"
                    );
                }
                if mode != 0 {
                    key(&mut w, KeyCode::Esc);
                }
            }
        }
    }
}

#[test]
fn command_selection_moves_up_within_the_visible_window_before_scrolling() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "命令").unwrap();
    w.set_design_motion(false);
    w.execute_command("browse-commands").unwrap();
    for _ in 0..15 {
        key(&mut w, KeyCode::Down);
        draw(&mut w, 120, 40, Instant::now());
    }
    let before = draw(&mut w, 120, 40, Instant::now());
    let (x, y) = point(&before, "▎", 20);
    key(&mut w, KeyCode::Up);
    let after = draw(&mut w, 120, 40, Instant::now());
    assert_eq!(
        point(&after, "▎", 20),
        (x, y - 1),
        "Up must move the highlight, not pull the list beneath it"
    );
    assert_eq!(w.palette().unwrap().selected, 14);
    assert_eq!(w.text(), "");
}

#[test]
fn terminal_selection_has_one_focus_marker_and_color_motion_without_background_or_text_changes() {
    use novelite_tui::Material;
    use ratatui::style::Color;
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "焦点").unwrap();
    w.set_color_theme(ColorTheme::Night).unwrap();
    w.set_material(Material::Terminal).unwrap();
    w.set_design_motion(true);
    key(&mut w, KeyCode::F(1));
    let now = Instant::now();
    draw(&mut w, 120, 40, now + Duration::from_secs(1));
    key(&mut w, KeyCode::Down);
    w.begin_design_transition(now);
    let first = draw(&mut w, 120, 40, now);
    let last = draw(&mut w, 120, 40, now + Duration::from_millis(250));
    assert_ne!(
        first, last,
        "accent color should settle gently on the terminal's own background"
    );
    assert_eq!(point(&first, "▎", 20), point(&last, "▎", 20));
    for (a, b) in first.content.iter().zip(&last.content) {
        assert_eq!(a.symbol(), b.symbol());
        assert_eq!(a.bg, Color::Reset);
        assert_eq!(b.bg, Color::Reset);
    }
    assert_eq!(last.content.iter().filter(|c| c.symbol() == "▎").count(), 1);
    assert_eq!(w.text(), "");
}

#[test]
fn short_terminal_keeps_commands_and_settings_operable_without_editing_the_draft() {
    for (width, height) in [(80, 12), (60, 12), (32, 10)] {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::create_project(dir.path().join("book"), "小窗").unwrap();
        w.configure_mouse(true);
        w.set_design_motion(false);
        w.dispatch(Action::Insert("正文 / 原样保留".into()))
            .unwrap();
        let original = w.text().to_owned();
        key(&mut w, KeyCode::F(1));
        let commands = draw(&mut w, width, height, Instant::now());
        assert!(
            commands.content.iter().any(|c| c.symbol() == "▎"),
            "commands must remain visible at {width}x{height}"
        );
        w.handle_event(Event::Paste("design-settings".into()))
            .unwrap();
        key(&mut w, KeyCode::Enter);
        assert!(w.design_settings_open());
        draw(&mut w, width, height, Instant::now());
        key(&mut w, KeyCode::Down);
        let settings = draw(&mut w, width, height, Instant::now());
        let focus = point(&settings, "▎", height / 2);
        assert!(row(&settings, focus.1).contains("页面留白"));
        click(&mut w, focus);
        let applied = draw(&mut w, width, height, Instant::now());
        assert!(row(&applied, focus.1).contains("宽松"));
        assert_eq!(applied[(0, 0)].bg, commands[(0, 0)].bg);
        assert_eq!(w.text(), original);
        key(&mut w, KeyCode::Esc);
        key(&mut w, KeyCode::Char('/'));
        assert_eq!(w.text(), format!("{original}/"));
    }
}
