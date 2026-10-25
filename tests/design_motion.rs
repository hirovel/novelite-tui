use novelite_tui::{Action, Background, ColorMode, Workspace};
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, Instant};
fn draw(w: &mut Workspace, t: &mut Terminal<TestBackend>, now: Instant) -> ratatui::buffer::Buffer {
    t.draw(|f| w.render_design(f, "", now)).unwrap();
    t.backend().buffer().clone()
}
#[test]
fn transition_keeps_text_immediate_settles_and_can_be_disabled() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "夜航").unwrap();
    w.set_color_mode(ColorMode::TrueColor).unwrap();
    w.set_background(Background::Dark).unwrap();
    w.set_design_motion(true);
    w.dispatch(Action::Insert("中文正文 👩‍💻 é，不参与面板动画。".into()))
        .unwrap();
    let original = w.text().to_owned();
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let now = Instant::now();
    draw(&mut w, &mut t, now);
    w.execute_command("chapters").unwrap();
    w.begin_design_transition(now);
    let first = draw(&mut w, &mut t, now);
    assert!(w.design_needs_frame());
    let middle = draw(&mut w, &mut t, now + Duration::from_millis(60));
    let last = draw(&mut w, &mut t, now + Duration::from_millis(230));
    assert!(!w.design_needs_frame());
    assert_eq!(
        first.content.iter().map(|c| c.symbol()).collect::<Vec<_>>(),
        last.content.iter().map(|c| c.symbol()).collect::<Vec<_>>()
    );
    assert_ne!(first, middle);
    assert_ne!(middle, last);
    assert_eq!(w.text(), original);
    w.execute_command("chapters").unwrap();
    w.begin_design_transition(now);
    w.set_design_motion(false);
    let no_motion = draw(&mut w, &mut t, now);
    assert!(!w.design_needs_frame());
    assert_eq!(
        no_motion,
        draw(&mut w, &mut t, now + Duration::from_millis(60))
    );
    w.execute_command("focus-editor").unwrap();
    w.dispatch(Action::Insert("立即输入".into())).unwrap();
    assert!(w.text().contains("立即输入"));
}
#[test]
fn resizing_cancels_effect_and_monochrome_never_animates() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "尺寸").unwrap();
    w.set_design_motion(true);
    w.set_color_mode(ColorMode::TrueColor).unwrap();
    w.set_background(Background::Dark).unwrap();
    let now = Instant::now();
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    draw(&mut w, &mut t, now);
    w.execute_command("chapters").unwrap();
    w.begin_design_transition(now);
    let mut small = Terminal::new(TestBackend::new(32, 10)).unwrap();
    draw(&mut w, &mut small, now);
    assert!(!w.design_needs_frame());
    w.set_color_mode(ColorMode::Monochrome).unwrap();
    w.begin_design_transition(now);
    assert!(!w.design_needs_frame());
    w.set_color_mode(ColorMode::TrueColor).unwrap();
    w.begin_design_transition(now);
    draw(&mut w, &mut small, now + Duration::from_millis(40));
    w.begin_design_transition(now + Duration::from_millis(40));
    draw(&mut w, &mut small, now + Duration::from_millis(270));
    assert!(!w.design_needs_frame());
}

#[test]
fn padded_panel_click_targets_visible_chapter_and_small_screen_keeps_commands() {
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "试写").unwrap();
    let volume = w.project().unwrap().volumes[0].id.clone();
    let first = w.active_chapter().unwrap().id.clone();
    let second = w.create_chapter(&volume, "海港").unwrap();
    w.switch_chapter(&first).unwrap();
    w.configure_mouse(true);
    w.execute_command("chapters").unwrap();
    let mut t = Terminal::new(TestBackend::new(150, 42)).unwrap();
    let b = draw(&mut w, &mut t, Instant::now());
    let at = b.content.iter().position(|c| c.symbol() == "海").unwrap();
    w.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (at % 150) as u16,
        row: (at / 150) as u16,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert_eq!(w.active_chapter().unwrap().id, second);
    let mut small = Terminal::new(TestBackend::new(32, 10)).unwrap();
    let b = draw(&mut w, &mut small, Instant::now());
    let text = b.content.iter().map(|c| c.symbol()).collect::<String>();
    assert!(text.replace(' ', "").contains("命令"));
}
#[test]
fn motion_setting_survives_project_change_and_old_frames_are_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("one"), "一").unwrap();
    w.set_color_mode(ColorMode::TrueColor).unwrap();
    w.set_design_motion(false);
    w.new_project(dir.path().join("two"), "二").unwrap();
    assert!(!w.design_motion_enabled());
    w.set_design_motion(true);
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let now = Instant::now();
    draw(&mut w, &mut t, now);
    w.switch_project(dir.path().join("one")).unwrap();
    w.begin_design_transition(now);
    assert!(
        !w.design_needs_frame(),
        "must not animate from a different project's frame"
    );
}

#[test]
fn design_shortcuts_use_normal_keymap_and_respect_user_save_binding() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "键位").unwrap();
    w.set_design_motion(true);
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::F(9), KeyModifiers::NONE)))
        .unwrap();
    assert!(
        w.design_motion_enabled(),
        "unused function keys must not change UI state"
    );
    w.execute_command("design-motion").unwrap();
    assert!(!w.design_motion_enabled());
    let keys = dir.path().join("keys.json");
    std::fs::write(&keys, r#"{"F9":"save","F8":"save"}"#).unwrap();
    w.load_keymap(&keys);
    for key in [8, 9] {
        w.dispatch(Action::Insert("新文字".into())).unwrap();
        w.handle_event(Event::Key(KeyEvent::new(
            KeyCode::F(key),
            KeyModifiers::NONE,
        )))
        .unwrap();
        assert!(!w.is_dirty());
        assert!(!w.design_motion_enabled());
    }
    w.execute_command("design-motion").unwrap();
    assert!(w.design_motion_enabled());
}

#[test]
fn native_and_ansi_colors_do_not_schedule_color_animation() {
    for (mode, background) in [
        (ColorMode::TrueColor, Background::Terminal),
        (ColorMode::Basic, Background::Dark),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut w = Workspace::create_project(dir.path().join("book"), "原生").unwrap();
        w.set_color_mode(mode).unwrap();
        w.set_background(background).unwrap();
        w.set_design_motion(true);
        let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let now = Instant::now();
        draw(&mut w, &mut t, now);
        w.open_design_settings().unwrap();
        w.begin_design_transition(now);
        assert!(
            !w.design_needs_frame(),
            "non-RGB palette must not request animation"
        );
        let first = draw(&mut w, &mut t, now);
        assert_eq!(first, draw(&mut w, &mut t, now + Duration::from_millis(50)));
    }
}
#[test]
fn reenabling_motion_does_not_reuse_disabled_frame_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "历史").unwrap();
    w.set_color_mode(ColorMode::TrueColor).unwrap();
    w.set_background(Background::Dark).unwrap();
    w.set_design_motion(true);
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let now = Instant::now();
    draw(&mut w, &mut t, now);
    w.set_design_motion(false);
    w.set_design_motion(true);
    w.begin_design_transition(now);
    assert!(
        !w.design_needs_frame(),
        "disabling releases earlier history immediately"
    );
    w.set_design_motion(false);
    w.open_design_settings().unwrap();
    draw(&mut w, &mut t, now);
    w.set_design_motion(true);
    w.begin_design_transition(now);
    assert!(
        !w.design_needs_frame(),
        "disabled draws do not capture history"
    );
}
#[test]
fn unchanged_rgb_surface_does_not_continue_animation() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "无变化").unwrap();
    w.set_color_mode(ColorMode::TrueColor).unwrap();
    w.set_background(Background::Dark).unwrap();
    w.set_design_motion(true);
    let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let now = Instant::now();
    draw(&mut w, &mut t, now);
    w.begin_design_transition(now);
    draw(&mut w, &mut t, now);
    assert!(!w.design_needs_frame());
}
