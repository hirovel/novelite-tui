use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};
use std::fs;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn custom_ctrl_alt_p_binding_is_not_stolen_by_the_ctrl_p_palette_shortcut() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.txt");
    let mut work = Workspace::open(&path).unwrap();
    work.load_keymap(dir.path().join("keys.json"));
    work.set_keybinding("Ctrl+Alt+p", "save").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    )))
    .unwrap();
    assert!(work.palette().is_none());
    assert_eq!(fs::read_to_string(path).unwrap(), "正文");
}

#[test]
fn duplicate_json_shortcuts_are_reported_instead_of_silently_choosing_the_last_command() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("keys.json");
    fs::write(&config, r#"{"F6":"save","F6":"quit"}"#).unwrap();
    let mut work = Workspace::open(dir.path().join("draft.txt")).unwrap();
    work.load_keymap(config);
    assert!(work.configuration_warning().is_some());
}

#[test]
fn timed_out_keyboard_detection_keeps_commands_available_and_releases_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.txt")).unwrap();
    assert!(!work.configure_keyboard(Err(std::io::Error::from(std::io::ErrorKind::TimedOut))));
    work.handle_event(key(KeyCode::F(1))).unwrap();
    assert!(work.palette().is_some());
    work.handle_event(key(KeyCode::Esc)).unwrap();
    assert!(work.configure_keyboard(Ok(true)));
    let released = KeyEvent::new_with_kind(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
        crossterm::event::KeyEventKind::Release,
    );
    assert!(work.handle_event(Event::Key(released)).unwrap().is_none());
    work.handle_event(key(KeyCode::Char('字'))).unwrap();
    assert_eq!(work.text(), "字");
}

#[test]
fn a_registered_command_is_discoverable_and_palette_cancel_preserves_editor_selection() {
    use novelite_tui::{Command, Direction};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.txt")).unwrap();
    work.dispatch(Action::Insert("abc".into())).unwrap();
    work.dispatch(Action::Select(Direction::Left)).unwrap();
    work.register_command(Command {
        id: "insert-heading".into(),
        title: "插入标题".into(),
        run: |work| work.dispatch(Action::Insert("# 章节".into())),
    })
    .unwrap();
    work.handle_event(key(KeyCode::F(1))).unwrap();
    work.handle_event(Event::Paste("nothing".into())).unwrap();
    work.handle_event(key(KeyCode::Esc)).unwrap();
    assert_eq!(work.selected_text(), "c");
    work.handle_event(key(KeyCode::F(1))).unwrap();
    work.handle_event(Event::Paste("insert-heading".into()))
        .unwrap();
    work.handle_event(key(KeyCode::Enter)).unwrap();
    assert_eq!(work.text(), "ab# 章节");
}

#[test]
fn configuration_cannot_overwrite_the_open_manuscript() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.txt");
    fs::write(&path, "my manuscript").unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.load_keymap(&path);
    assert!(work.execute_command("reset-keys").is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "my manuscript");
}

#[test]
fn custom_binding_survives_reopen_and_invalid_configuration_keeps_a_working_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.txt");
    let config = dir.path().join("keys.json");
    let mut work = Workspace::open(&path).unwrap();
    work.load_keymap(&config);
    work.set_keybinding("F6", "save").unwrap();
    work.dispatch(Action::Insert("保存我".into())).unwrap();
    work.handle_event(key(KeyCode::F(6))).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "保存我");
    // Reopening a closed session must not race the old owner's disk fingerprint.
    drop(work);
    let mut reopened = Workspace::open(&path).unwrap();
    reopened.load_keymap(&config);
    assert!(
        reopened.configuration_warning().is_none(),
        "{:?}",
        reopened.configuration_warning()
    );
    reopened.dispatch(Action::Insert("前言".into())).unwrap();
    reopened.handle_event(key(KeyCode::F(6))).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "前言保存我");
    assert!(reopened.set_keybinding("F1", "quit").is_err());
    fs::write(&config, r#"{"Ctrl+s":"save", "CTRL+S":"quit"}"#).unwrap();
    reopened.load_keymap(&config);
    assert!(reopened.configuration_warning().is_some());
    reopened.handle_event(key(KeyCode::F(1))).unwrap();
    assert!(reopened.palette().is_some());
    reopened.execute_command("reset-keys").unwrap();
    assert!(reopened.configuration_warning().is_none());
}

#[test]
fn settings_comma_binding_survives_custom_binding_save_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.txt");
    let keys = dir.path().join("keys.json");
    let mut work = Workspace::open(&path).unwrap();
    work.load_keymap(&keys);
    work.set_keybinding("F6", "save").unwrap();
    work.load_keymap(&keys);
    assert!(work.configuration_warning().is_none());
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char(','),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert!(work.design_settings_open());
    assert_eq!(work.text(), "");
}
