use novelite_tui::{Action, Direction, Workspace};
use std::fs;

#[test]
fn library_and_keymap_cannot_overwrite_each_other_in_either_loading_order() {
    let folder = tempfile::tempdir().unwrap();
    let shared = folder.path().join("shared.json");
    let mut work = Workspace::create_project(folder.path().join("book"), "保留最近作品").unwrap();
    work.load_library(&shared).unwrap();
    work.load_keymap(folder.path().join("./shared.json"));
    assert!(work.execute_command("reset-keys").is_err());
    let state: serde_json::Value = serde_json::from_slice(&fs::read(&shared).unwrap()).unwrap();
    assert_eq!(state["projects"][0]["title"], "保留最近作品");
    let other = folder.path().join("keys-first.json");
    let mut second = Workspace::create_project(folder.path().join("second"), "另一本").unwrap();
    second.load_keymap(&other);
    second.set_keybinding("F6", "save").unwrap();
    let keys = fs::read(&other).unwrap();
    assert!(second.load_library(&other).is_err());
    assert_eq!(fs::read(&other).unwrap(), keys);
    second.execute_command("reset-keys").unwrap();
    assert_eq!(fs::read_to_string(&other).unwrap(), "{}");
}

#[test]
fn save_and_exit_shortcuts_remain_available_while_chapter_panel_is_open() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let folder = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    work.dispatch(Action::Insert("需要保存".into())).unwrap();
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert!(!work.is_dirty());
    assert_eq!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::CONTROL
        )))
        .unwrap(),
        Some(novelite_tui::Outcome::Exit)
    );
}

#[test]
fn failed_save_blocks_chapter_and_project_switches_without_losing_the_current_buffer() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let other = folder.path().join("other");
    drop(Workspace::create_project(&other, "另一本").unwrap());
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "新章").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    work.dispatch(Action::Insert("切换前未保存".into()))
        .unwrap();
    let path = work.path().to_owned();
    fs::rename(&path, root.join("original")).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(work.switch_chapter(&second).is_err());
    assert!(work.switch_project(&other).is_err());
    assert_eq!(work.active_chapter().unwrap().id, first);
    assert_eq!(work.text(), "切换前未保存");
    fs::remove_dir(&path).unwrap();
    fs::rename(root.join("original"), &path).unwrap();
    work.switch_project(&other).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "切换前未保存");
}

#[test]
fn resuming_a_later_chapter_does_not_reset_the_first_chapters_saved_position() {
    let folder = tempfile::tempdir().unwrap();
    let state = folder.path().join("recent.json");
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    work.load_library(&state).unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    work.dispatch(Action::Insert("第一章的光标位置".into()))
        .unwrap();
    let position = work.cursor();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "末章").unwrap();
    work.switch_chapter(&second).unwrap();
    work.dispatch(Action::Insert("末章".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut reopened = Workspace::resume(&state).unwrap().unwrap();
    assert_eq!(reopened.recent_projects()[0].chapter, second);
    reopened.switch_chapter(&first).unwrap();
    assert_eq!(reopened.cursor(), position);
}

#[test]
fn removing_current_recent_record_stays_removed_after_quit() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let state = folder.path().join("recent.json");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.load_library(&state).unwrap();
    work.execute_command("forget-project").unwrap();
    work.dispatch(Action::Insert("仍要保存的正文".into()))
        .unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    assert!(Workspace::resume(&state).unwrap().is_none());
    assert_eq!(
        Workspace::open_project(root).unwrap().text(),
        "仍要保存的正文"
    );
}

#[test]
fn chapter_panel_searches_titles_and_closes_back_to_editor_without_typing_into_body() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let folder = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "重逢").unwrap();
    work.dispatch(Action::Insert("留在原章".into())).unwrap();
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Paste("重逢".into())).unwrap();
    assert_eq!(work.navigation().unwrap().entries.len(), 1);
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(work.active_chapter().unwrap().id, second);
    assert!(work.navigation().is_none());
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    assert!(work.navigation().is_none());
    assert!(work.palette().is_none());
    work.handle_event(Event::Paste("正文".into())).unwrap();
    assert_eq!(work.text(), "正文");
    work.previous_chapter().unwrap();
    assert_eq!(work.text(), "留在原章");
}

#[test]
fn recent_projects_restore_chapter_positions_and_removing_record_keeps_manuscripts() {
    let folder = tempfile::tempdir().unwrap();
    let state = folder.path().join("settings/recent.json");
    let first_root = folder.path().join("first");
    let second_root = folder.path().join("second");
    let mut work = Workspace::create_project(&first_root, "第一本").unwrap();
    work.load_library(&state).unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let chapter = work.create_chapter(&volume, "末章").unwrap();
    work.switch_chapter(&chapter).unwrap();
    work.dispatch(Action::Insert("重开继续写作".into()))
        .unwrap();
    work.dispatch(Action::Move(Direction::Left)).unwrap();
    let position = work.cursor();
    work.new_project(&second_root, "第二本").unwrap();
    work.dispatch(Action::Insert("第二本内容".into())).unwrap();
    work.switch_project(&first_root).unwrap();
    assert_eq!(work.text(), "重开继续写作");
    assert_eq!(work.cursor(), position);
    assert_eq!(work.recent_projects().len(), 2);
    work.rename_project("第一本改名").unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut resumed = Workspace::resume(&state).unwrap().unwrap();
    assert_eq!(resumed.active_chapter().unwrap().id, chapter);
    assert_eq!(resumed.cursor(), position);
    assert_eq!(resumed.recent_projects()[0].title, "第一本改名");
    resumed.forget_project(&second_root).unwrap();
    assert_eq!(resumed.recent_projects().len(), 1);
    let second = Workspace::open_project(second_root).unwrap();
    assert_eq!(second.text(), "第二本内容");
}

#[test]
fn writer_creates_named_volumes_and_chapters_and_returns_to_each_editing_position() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "长夜").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    work.dispatch(Action::Insert("第一章正文".into())).unwrap();
    work.dispatch(Action::Move(Direction::Left)).unwrap();
    let position = work.cursor();
    let volume = work.create_volume("第二卷").unwrap();
    let second = work.create_chapter(&volume, "同名章节").unwrap();
    work.switch_chapter(&second).unwrap();
    work.dispatch(Action::Insert("第二章正文".into())).unwrap();
    work.rename_chapter(&second, "新标题").unwrap();
    work.rename_volume(&volume, "终卷").unwrap();
    work.switch_chapter(&first).unwrap();
    assert_eq!(work.text(), "第一章正文");
    assert_eq!(work.cursor(), position);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "");
    work.dispatch(Action::Redo).unwrap();
    work.next_chapter().unwrap();
    assert_eq!(work.active_chapter().unwrap().id, second);
    assert_eq!(work.text(), "第二章正文");
    work.previous_chapter().unwrap();
    assert_eq!(work.active_chapter().unwrap().id, first);
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut reopened = Workspace::open_project(&root).unwrap();
    let project = reopened.project().unwrap();
    assert_eq!(project.volumes[1].title, "终卷");
    assert_eq!(project.volumes[1].chapters[0].title, "新标题");
    assert_eq!(fs::read_to_string(reopened.path()).unwrap(), "第一章正文");
    reopened.switch_chapter(&second).unwrap();
    assert_eq!(fs::read_to_string(reopened.path()).unwrap(), "第二章正文");
}
