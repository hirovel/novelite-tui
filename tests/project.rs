use novelite_tui::{Action, Workspace};
use std::fs;

#[cfg(windows)]
#[test]
fn a_file_held_by_another_windows_program_keeps_input_and_can_be_retried() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "书").unwrap();
    work.dispatch(Action::Insert("占用期间的输入".into()))
        .unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(work.path())
        .unwrap();
    assert!(work.dispatch(Action::Quit).is_err());
    assert!(work.is_dirty());
    assert_eq!(work.text(), "占用期间的输入");
    drop(held);
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "占用期间的输入");
}

#[test]
fn saving_refuses_a_newer_manifest_that_appeared_after_open() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("待保存".into())).unwrap();
    let path = root.join("novelite.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["version"] = 99.into();
    let future = serde_json::to_vec(&manifest).unwrap();
    fs::write(&path, &future).unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    assert_eq!(fs::read(path).unwrap(), future);
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "");
    assert_eq!(work.text(), "待保存");
}

#[test]
fn configuration_writes_remain_outside_project_even_after_save_as() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.load_keymap(root.join("missing-parent/keys.json"));
    assert!(work.configuration_warning().is_some());
    assert!(work.execute_command("reset-keys").is_err());
    assert!(!root.join("missing-parent").exists());
    let target = directory.path().join("rescue.txt");
    let mut single = Workspace::open(directory.path().join("source.txt")).unwrap();
    single.load_keymap(&target);
    single
        .dispatch(Action::Insert("不能被配置覆盖".into()))
        .unwrap();
    single.save_as(&target).unwrap();
    assert!(single.execute_command("reset-keys").is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "不能被配置覆盖");
}

#[test]
fn command_prompts_rename_and_save_a_copy_without_inserting_their_input_into_body() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "旧名").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    work.execute_command("rename-project").unwrap();
    assert!(work.prompt().is_some());
    work.handle_event(Event::Paste("新书名".into())).unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(work.project().unwrap().title, "新书名");
    work.execute_command("save-as").unwrap();
    let copy = directory.path().join("copy");
    work.handle_event(Event::Paste(copy.to_string_lossy().into_owned()))
        .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert!(work.prompt().is_none());
    assert_eq!(work.text(), "正文");
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "正文");
    assert!(copy.join("novelite.json").exists());
}

#[test]
fn undo_after_failed_commit_flushes_the_current_text_instead_of_replaying_abandoned_text() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("后来撤销的内容".into()))
        .unwrap();
    fs::rename(root.join("novelite.json"), root.join("original.json")).unwrap();
    fs::create_dir(root.join("novelite.json")).unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    work.dispatch(Action::Undo).unwrap();
    fs::remove_dir(root.join("novelite.json")).unwrap();
    fs::rename(root.join("original.json"), root.join("novelite.json")).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    assert_eq!(Workspace::open_project(root).unwrap().text(), "");
}

#[test]
fn failed_save_can_save_the_whole_project_elsewhere_without_losing_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let copy = directory.path().join("rescued");
    let mut work = Workspace::create_project(&root, "旧名").unwrap();
    work.dispatch(Action::Insert("保留正文".into())).unwrap();
    work.rename_project("保留书名").unwrap();
    let original_path = work.path().to_owned();
    let permissions = fs::metadata(&original_path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&original_path, readonly).unwrap();
    assert!(work.dispatch(Action::Quit).is_err());
    assert_eq!(work.text(), "保留正文");
    assert!(matches!(
        work.save_state(),
        novelite_tui::SaveState::Failed(_)
    ));
    work.save_as(&copy).unwrap();
    assert!(!work.is_dirty());
    assert!(work.path().starts_with(fs::canonicalize(&copy).unwrap()));
    fs::set_permissions(original_path, permissions).unwrap();
    drop(work);
    let reopened = Workspace::open_project(&copy).unwrap();
    assert_eq!(reopened.text(), "保留正文");
    assert_eq!(reopened.project().unwrap().title, "保留书名");
    assert!(Workspace::create_project(&copy, "不可覆盖").is_err());
}

#[test]
fn autosave_finishes_old_revision_without_marking_later_input_saved() {
    use novelite_tui::SaveState;
    use std::time::{Duration, Instant};
    let directory = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(directory.path().join("book"), "初名").unwrap();
    work.dispatch(Action::Insert("第一批".into())).unwrap();
    assert_eq!(work.save_state(), SaveState::Pending);
    work.tick(Instant::now() + Duration::from_millis(350))
        .unwrap();
    assert_eq!(work.save_state(), SaveState::Saving);
    work.dispatch(Action::Insert("第二批".into())).unwrap();
    work.rename_project("新名").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while work.save_state() == SaveState::Saving {
        work.poll_save().unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(work.is_dirty());
    assert_eq!(work.save_state(), SaveState::Pending);
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "第一批");
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "第一批第二批");
    assert_eq!(work.save_state(), SaveState::Saved);
    let root = directory.path().join("book");
    drop(work);
    assert_eq!(
        Workspace::open_project(root)
            .unwrap()
            .project()
            .unwrap()
            .title,
        "新名"
    );
}

#[test]
fn interrupted_body_and_manifest_commit_replays_a_complete_revision() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let mut work = Workspace::create_project(&root, "旧书名").unwrap();
    work.dispatch(Action::Insert("完整新正文".into())).unwrap();
    work.rename_project("新书名").unwrap();
    let manifest = root.join("novelite.json");
    let original = fs::metadata(&manifest).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&manifest, readonly).unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    assert!(work.is_dirty());
    assert_eq!(work.text(), "完整新正文");
    fs::set_permissions(&manifest, original).unwrap();
    drop(work);
    let reopened = Workspace::open_project(&root).unwrap();
    assert_eq!(reopened.text(), "完整新正文");
    assert_eq!(reopened.project().unwrap().title, "新书名");
    assert!(!root.join(".novelite-journal.json").exists());
}

#[test]
fn unknown_versions_unsafe_paths_and_duplicate_identities_are_rejected_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    drop(Workspace::create_project(&root, "书").unwrap());
    let path = root.join("novelite.json");
    let original: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    fs::write(directory.path().join("outside.md"), "不可覆盖").unwrap();
    for case in 0..4 {
        let mut manifest = original.clone();
        match case {
            0 => manifest["version"] = 99.into(),
            1 => manifest["volumes"][0]["chapters"][0]["path"] = "../outside.md".into(),
            2 => {
                let chapter = manifest["volumes"][0]["chapters"][0].clone();
                manifest["volumes"][0]["chapters"]
                    .as_array_mut()
                    .unwrap()
                    .push(chapter);
            }
            _ => manifest["volumes"] = serde_json::json!([]),
        }
        let bytes = serde_json::to_vec(&manifest).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(Workspace::open_project(&root).is_err(), "case {case}");
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(
            fs::read_to_string(directory.path().join("outside.md")).unwrap(),
            "不可覆盖"
        );
    }
    assert!(Workspace::create_project(&root, "碰撞").is_err());
}

#[test]
fn second_instance_is_read_only_and_releases_lock_when_writer_closes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let mut writer = Workspace::create_project(&root, "书").unwrap();
    writer.dispatch(Action::Insert("原文".into())).unwrap();
    writer.dispatch(Action::Save).unwrap();
    let mut reader = Workspace::open_project(&root).unwrap();
    assert!(reader.is_read_only());
    assert!(reader.dispatch(Action::Insert("不能写入".into())).is_err());
    assert!(reader.rename_project("不能改名").is_err());
    assert_eq!(reader.text(), "原文");
    reader.dispatch(Action::Quit).unwrap();
    drop(writer);
    let reopened = Workspace::open_project(&root).unwrap();
    assert!(!reopened.is_read_only());
}

#[test]
fn portable_project_reopens_body_and_metadata_with_stable_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("雨夜");
    let mut work = Workspace::create_project(&root, "雨夜").unwrap();
    let id = work.project().unwrap().id.clone();
    let chapter = work.project().unwrap().volumes[0].chapters[0].id.clone();
    work.dispatch(Action::Insert("窗外有雨。".into())).unwrap();
    work.dispatch(Action::Newline).unwrap();
    work.dispatch(Action::Insert("灯还亮着。".into())).unwrap();
    work.rename_project("长夜").unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let reopened = Workspace::open_project(&root).unwrap();
    assert_eq!(reopened.text(), "窗外有雨。\n灯还亮着。");
    assert_eq!(reopened.project().unwrap().title, "长夜");
    assert_eq!(reopened.project().unwrap().id, id);
    assert_eq!(
        reopened.project().unwrap().volumes[0].chapters[0].id,
        chapter
    );
    assert_eq!(
        fs::read_to_string(reopened.path()).unwrap(),
        "窗外有雨。\n灯还亮着。"
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("novelite.json")).unwrap()).unwrap();
    assert_eq!(manifest["version"], 1);
    assert_eq!(manifest["title"], "长夜");
    assert_eq!(manifest["volumes"].as_array().unwrap().len(), 1);
    assert!(!reopened.is_dirty());
}
