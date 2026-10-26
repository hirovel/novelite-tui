use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{
    Action, Direction, Outcome, RecentKind, SaveState, StartOutcome, StartScreen, Workspace,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{fs, path::Path};
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn open_result(result: StartOutcome) -> Workspace {
    match result {
        StartOutcome::Open(work) => *work,
        _ => panic!("expected editor"),
    }
}
fn record(path: &Path, title: &str) -> serde_json::Value {
    serde_json::json!({"kind":"file","root":path,"title":title,"chapter":"","positions":{"":0}})
}
#[test]
fn first_start_has_real_actions_and_closing_does_not_create_a_document() {
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("recent.json");
    let mut start = StartScreen::new(Some(state.clone()));
    assert!(start.resume().is_none());
    for (width, height) in [(12, 5), (32, 12), (80, 24), (120, 40), (180, 50)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| start.render(f)).unwrap();
    }
    start.handle_event(key(KeyCode::Enter)).unwrap();
    assert!(start.prompt().unwrap().title.contains("打开"));
    start.handle_event(key(KeyCode::Esc)).unwrap();
    assert!(matches!(
        start.handle_event(key(KeyCode::Esc)).unwrap(),
        StartOutcome::Exit
    ));
    assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    assert!(start.open_requested(d.path(), false).is_err());
    assert_eq!(
        fs::read_dir(d.path()).unwrap().count(),
        0,
        "browsing an ordinary folder must not create a project lock"
    );
}
#[test]
fn first_start_open_write_quit_and_resume_a_unicode_single_file() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("夜航 👩‍💻.MD");
    let state = d.path().join("recent.json");
    fs::write(&file, "雨夜").unwrap();
    let mut start = StartScreen::new(Some(state.clone()));
    start.handle_event(key(KeyCode::Enter)).unwrap();
    start
        .handle_event(Event::Paste(file.to_string_lossy().into_owned()))
        .unwrap();
    let mut work = open_result(start.handle_event(key(KeyCode::Enter)).unwrap());
    work.dispatch(Action::Insert("/👩‍💻e\u{301}".into())).unwrap();
    work.dispatch(Action::Move(Direction::Left)).unwrap();
    let position = work.cursor();
    assert_eq!(work.dispatch(Action::Quit).unwrap(), Outcome::Exit);
    drop(work);
    let mut start = StartScreen::new(Some(state.clone()));
    let resumed = start.resume().unwrap();
    assert_eq!(resumed.text(), "/👩‍💻e\u{301}雨夜");
    assert_eq!(resumed.cursor(), position);
    assert_eq!(resumed.recent_projects()[0].kind, RecentKind::File);
    let data: serde_json::Value = serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(data["version"], 2);
}
#[test]
fn missing_first_recent_stays_visible_and_another_entry_can_be_opened() {
    let d = tempfile::tempdir().unwrap();
    let good = d.path().join("可用.md");
    let missing = d.path().join("缺失.md");
    let state = d.path().join("recent.json");
    fs::write(&good, "继续写作").unwrap();
    fs::write(&state,serde_json::to_vec(&serde_json::json!({"version":2,"projects":[record(&missing,"缺失"),record(&good,"可用")]})).unwrap()).unwrap();
    let mut start = StartScreen::new(Some(state.clone()));
    assert!(start.resume().is_none());
    assert!(start.message().contains("缺失"));
    assert_eq!(start.entries().len(), 2);
    start.handle_event(key(KeyCode::Down)).unwrap();
    let work = open_result(start.handle_event(key(KeyCode::Enter)).unwrap());
    assert_eq!(work.text(), "继续写作");
    assert_eq!(work.recent_projects().len(), 2);
    assert!(
        work.recent_projects()
            .iter()
            .any(|e| e.root.ends_with("缺失.md"))
    );
}
#[test]
fn damaged_or_future_recent_data_is_preserved_while_the_manuscript_saves() {
    for contents in ["{broken", r#"{"version":999,"projects":[]}"#] {
        let d = tempfile::tempdir().unwrap();
        let state = d.path().join("recent.json");
        let file = d.path().join("正文.md");
        fs::write(&state, contents).unwrap();
        fs::write(&file, "原文").unwrap();
        let start = StartScreen::new(Some(state.clone()));
        assert!(!start.message().is_empty());
        let mut work = start.open_requested(&file, false).unwrap();
        work.dispatch(Action::Insert("新稿/".into())).unwrap();
        work.dispatch(Action::Save).unwrap();
        assert_eq!(work.save_state(), SaveState::Saved);
        assert!(work.library_warning().unwrap().contains("正文保存不受影响"));
        assert_eq!(fs::read_to_string(&file).unwrap(), "新稿/原文");
        work.dispatch(Action::Quit).unwrap();
        assert_eq!(fs::read_to_string(&state).unwrap(), contents);
    }
}
#[test]
fn recent_write_failure_and_external_change_do_not_misreport_body_save_failure() {
    for directory_failure in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let state = d.path().join("recent.json");
        let file = d.path().join("稿.md");
        let mut work = Workspace::open(&file).unwrap();
        work.load_library(&state).unwrap();
        if directory_failure {
            fs::remove_file(&state).unwrap();
            fs::create_dir(&state).unwrap();
        } else {
            fs::write(&state, "external data").unwrap();
        }
        work.dispatch(Action::Insert("必须落盘".into())).unwrap();
        work.dispatch(Action::Save).unwrap();
        assert_eq!(work.save_state(), SaveState::Saved);
        assert!(work.library_warning().is_some());
        assert_eq!(fs::read_to_string(&file).unwrap(), "必须落盘");
        if directory_failure {
            assert!(state.is_dir());
        } else {
            assert_eq!(fs::read_to_string(&state).unwrap(), "external data");
        }
    }
}
#[test]
fn old_project_records_survive_adding_a_single_file_and_restore_their_chapter() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("book");
    let file = d.path().join("随笔.md");
    let state = d.path().join("recent.json");
    let mut original = Workspace::create_project(&root, "作品").unwrap();
    let first = original.active_chapter().unwrap().id.clone();
    original
        .dispatch(Action::Insert("作品正文".into()))
        .unwrap();
    original.dispatch(Action::Quit).unwrap();
    drop(original);
    fs::write(&file, "单篇正文").unwrap();
    fs::write(&state,serde_json::to_vec(&serde_json::json!({"version":1,"projects":[{"root":fs::canonicalize(&root).unwrap(),"title":"作品","chapter":first,"positions":{first.clone():6}}]})).unwrap()).unwrap();
    let mut start = StartScreen::new(Some(state.clone()));
    let mut work = start.resume().unwrap();
    assert_eq!(work.cursor(), 6);
    work.switch_path(&file).unwrap();
    assert_eq!(work.recent_projects().len(), 2);
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut resumed = Workspace::resume(&state).unwrap().unwrap();
    assert!(resumed.project().is_none());
    resumed.switch_project(&root).unwrap();
    assert_eq!(resumed.cursor(), 6);
    assert_eq!(resumed.active_chapter().unwrap().id, first);
}
#[test]
fn forgetting_a_single_file_keeps_the_file_and_does_not_reappear_on_save_or_quit() {
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("recent.json");
    let file = d.path().join("稿.md");
    let mut work = Workspace::open(&file).unwrap();
    work.load_library(&state).unwrap();
    work.execute_command("forget-project").unwrap();
    work.dispatch(Action::Insert("保留".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    assert!(Workspace::resume(&state).unwrap().is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "保留");
}
#[test]
fn single_file_cursor_is_clamped_to_a_grapheme_after_external_shortening() {
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("recent.json");
    let file = d.path().join("稿.md");
    fs::write(&file, "很长的👩‍💻正文").unwrap();
    let mut work = Workspace::open(&file).unwrap();
    work.load_library(&state).unwrap();
    work.dispatch(Action::Move(Direction::End)).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    fs::write(&file, "e\u{301}").unwrap();
    let resumed = Workspace::resume(&state).unwrap().unwrap();
    assert_eq!(resumed.cursor(), "e\u{301}".len());
}
#[test]
fn path_completion_cycles_unicode_candidates_and_errors_preserve_the_input() {
    let d = tempfile::tempdir().unwrap();
    for name in ["夜 一.md", "夜 二.txt", "夜 其他.log"] {
        fs::write(d.path().join(name), "稿").unwrap();
    }
    let mut start = StartScreen::new(None);
    start.handle_event(key(KeyCode::Enter)).unwrap();
    let prefix = d.path().join("夜 ").to_string_lossy().into_owned();
    start.handle_event(Event::Paste(prefix)).unwrap();
    start.handle_event(key(KeyCode::Tab)).unwrap();
    assert!(start.prompt().unwrap().text.ends_with("夜 一.md"));
    start.handle_event(key(KeyCode::Tab)).unwrap();
    assert!(start.prompt().unwrap().text.ends_with("夜 二.txt"));
    let file = start.prompt().unwrap().text.clone();
    let mut work = open_result(start.handle_event(key(KeyCode::Enter)).unwrap());
    assert_eq!(work.path(), fs::canonicalize(file).unwrap());
    work.execute_command("open-project").unwrap();
    let absent = d.path().join("不存在").to_string_lossy().into_owned();
    work.handle_event(Event::Paste(absent.clone())).unwrap();
    work.handle_event(key(KeyCode::Tab)).unwrap();
    assert_eq!(work.prompt().unwrap().text, absent);
    assert!(work.handle_event(key(KeyCode::Enter)).is_err());
    assert_eq!(work.prompt().unwrap().text, absent);
    work.handle_event(key(KeyCode::Esc)).unwrap();
    assert_eq!(work.text(), "稿");
}
#[test]
fn explicit_user_paths_and_recent_entries_cannot_bypass_managed_chapter_protection() {
    let d = tempfile::tempdir().unwrap();
    let managed = Workspace::create_project(d.path().join("book"), "作品").unwrap();
    let chapter = managed.path().to_owned();
    assert!(
        Workspace::open_user_path(&chapter, false)
            .unwrap_err_text()
            .contains("作品内")
    );
    let start = StartScreen::new(None);
    assert!(start.open_requested(&chapter, false).is_err());
    let state = d.path().join("recent.json");
    fs::write(
        &state,
        serde_json::to_vec(
            &serde_json::json!({"version":2,"projects":[record(&chapter,"错误单篇")]}),
        )
        .unwrap(),
    )
    .unwrap();
    let mut start = StartScreen::new(Some(state));
    assert!(start.resume().is_none());
    assert!(start.message().contains("作品内"));
    assert_eq!(start.entries().len(), 1);
}
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl<T> ErrorText for std::io::Result<T> {
    fn unwrap_err_text(self) -> String {
        match self {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error"),
        }
    }
}
#[test]
fn configuration_paths_are_protected_before_creation_and_when_switching() {
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("keymap.md");
    fs::write(&config, "{}").unwrap();
    let mut start = StartScreen::new(None);
    start.reserve(&config).unwrap();
    assert!(start.open_requested(&config, false).is_err());
    let mut work = Workspace::open(d.path().join("稿.md")).unwrap();
    work.load_keymap(&config);
    assert!(work.switch_path(&config).is_err());
    assert!(work.save_as(&config).is_err());
    assert_eq!(fs::read_to_string(&config).unwrap(), "{}");
    let new = d.path().join("new-book");
    start.reserve(new.join("workspace.json")).unwrap();
    assert!(start.open_requested(&new, true).is_err());
    assert!(!new.exists());
}
#[test]
fn cli_new_single_file_remains_supported_but_open_prompt_only_opens_existing_paths() {
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("新文稿.md");
    let start = StartScreen::new(None);
    let mut work = start.open_requested(&file, false).unwrap();
    assert!(!file.exists());
    work.dispatch(Action::Insert("新文稿".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "新文稿");
    let mut start = StartScreen::new(None);
    start.handle_event(key(KeyCode::Enter)).unwrap();
    start
        .handle_event(Event::Paste(
            d.path().join("absent.md").to_string_lossy().into_owned(),
        ))
        .unwrap();
    assert!(start.handle_event(key(KeyCode::Enter)).is_err());
    assert!(start.prompt().is_some());
}

#[test]
fn start_screen_can_create_a_real_project_and_mouse_open_is_bound_to_visible_rows() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("recent.json");
    let root = d.path().join("夜航");
    let mut start = StartScreen::new(Some(state.clone()));
    start.handle_event(key(KeyCode::Char('n'))).unwrap();
    start
        .handle_event(Event::Paste(root.to_string_lossy().into_owned()))
        .unwrap();
    let mut work = open_result(start.handle_event(key(KeyCode::Enter)).unwrap());
    assert_eq!(work.project().unwrap().title, "夜航");
    work.dispatch(Action::Insert("新故事".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut start = StartScreen::new(Some(state));
    start.configure_mouse(true);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| start.render(f)).unwrap();
    let b = terminal.backend().buffer();
    let index = b.content.iter().position(|c| c.symbol() == "夜").unwrap();
    let opened = start
        .handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: (index % 80) as u16,
            row: (index / 80) as u16,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    assert_eq!(open_result(opened).text(), "新故事");
}
#[test]
fn save_as_records_the_new_single_file_destination_for_the_next_start() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("原稿.md");
    let copied = d.path().join("新稿.md");
    let state = d.path().join("recent.json");
    fs::write(&source, "原稿").unwrap();
    let mut work = Workspace::open(&source).unwrap();
    work.load_library(&state).unwrap();
    work.dispatch(Action::Insert("续写/".into())).unwrap();
    work.save_as(&copied).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let resumed = Workspace::resume(state).unwrap().unwrap();
    assert_eq!(resumed.path(), fs::canonicalize(copied).unwrap());
    assert_eq!(resumed.text(), "续写/原稿");
    assert_eq!(fs::read_to_string(source).unwrap(), "原稿");
}
#[cfg(unix)]
#[test]
fn symbolic_file_aliases_cannot_open_managed_chapters_as_standalone_manuscripts() {
    let d = tempfile::tempdir().unwrap();
    let managed = Workspace::create_project(d.path().join("book"), "作品").unwrap();
    let alias = d.path().join("alias.md");
    std::os::unix::fs::symlink(managed.path(), &alias).unwrap();
    assert!(Workspace::open_user_path(&alias, false).is_err());
    assert!(StartScreen::new(None).open_requested(alias, false).is_err());
}

#[test]
fn manuscript_save_failure_takes_priority_over_a_recent_record_warning() {
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("recent.json");
    let file = d.path().join("draft.md");
    fs::write(&state, "{broken").unwrap();
    fs::write(&file, "safe text").unwrap();
    let start = StartScreen::new(Some(state.clone()));
    let mut work = start.open_requested(&file, false).unwrap();
    work.dispatch(Action::Insert("unsaved ".into())).unwrap();
    let original_permissions = fs::metadata(&file).unwrap().permissions();
    let mut readonly = original_permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&file, readonly).unwrap();
    let result = work.dispatch(Action::Save);
    fs::set_permissions(&file, original_permissions).unwrap();
    assert!(result.is_err());
    assert!(matches!(work.save_state(), SaveState::Failed(_)));
    assert!(work.library_warning().is_some());
    for zen in [false, true] {
        if zen {
            work.execute_command("toggle-zen").unwrap();
        }
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
        terminal.draw(|f| work.render(f, "")).unwrap();
        let notice = (0..120)
            .map(|x| terminal.backend().buffer()[(x, 22)].symbol())
            .collect::<String>()
            .replace(' ', "");
        assert!(notice.contains("保存失败"), "{notice}");
        assert!(!notice.contains("正文保存不受影响"), "{notice}");
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), "safe text");
    assert_eq!(fs::read_to_string(&state).unwrap(), "{broken");
    assert_eq!(work.text(), "unsaved safe text");
}
