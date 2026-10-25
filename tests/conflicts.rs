use novelite_tui::{Action, Direction, Workspace};
use std::fs;

#[test]
fn recent_location_does_not_replace_the_draft_recovered_from_a_conflicted_journal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let state = dir.path().join("recent.json");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.load_library(&state).unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "冲突章").unwrap();
    work.switch_chapter(&second).unwrap();
    work.dispatch(Action::Insert("恢复日志中的第二章稿".into()))
        .unwrap();
    fs::write(work.path(), "外部第二章稿").unwrap();
    work.request_save().unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    drop(work);
    let reopened = Workspace::resume(&state).unwrap().unwrap();
    assert_eq!(reopened.active_chapter().unwrap().id, second);
    assert_eq!(reopened.text(), "恢复日志中的第二章稿");
    assert!(reopened.external_conflict().is_some());
}

#[test]
fn conflicted_journal_with_corrupt_manifest_reopens_for_copy_without_overwriting_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("清单损坏仍可救出的稿".into()))
        .unwrap();
    fs::write(work.path(), "外部稿").unwrap();
    work.request_save().unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    drop(work);
    fs::write(root.join("novelite.json"), "损坏清单").unwrap();
    let mut reopened = Workspace::open_project(&root).unwrap();
    assert!(reopened.external_conflict().is_some());
    let copy = dir.path().join("rescued.md");
    reopened.save_conflict_copy(&copy).unwrap();
    assert_eq!(fs::read_to_string(copy).unwrap(), "清单损坏仍可救出的稿");
    assert!(reopened.overwrite_external().is_err());
    assert_eq!(
        fs::read_to_string(root.join("novelite.json")).unwrap(),
        "损坏清单"
    );
    assert!(root.join(".novelite-journal.json").is_file());
}

#[test]
fn readonly_session_can_quit_after_external_deletion_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let writer = Workspace::create_project(&root, "书").unwrap();
    let mut reader = Workspace::open_project(&root).unwrap();
    assert!(reader.is_read_only());
    fs::remove_file(writer.path()).unwrap();
    reader.check_external().unwrap();
    assert!(reader.external_conflict().is_some());
    assert_eq!(
        reader.dispatch(Action::Quit).unwrap(),
        novelite_tui::Outcome::Exit
    );
    assert!(!writer.path().exists());
    assert!(!root.join("conflicts").exists());
}

#[test]
fn reopening_a_conflicted_save_recovers_local_draft_and_keeps_all_choices_available() {
    for choice in ["load", "overwrite", "invalid-body"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("book");
        let mut work = Workspace::create_project(&root, "书").unwrap();
        work.dispatch(Action::Insert("重启后应保留的本地稿".into()))
            .unwrap();
        fs::write(work.path(), "重启前的外部稿").unwrap();
        work.request_save().unwrap();
        assert!(work.dispatch(Action::Save).is_err());
        let path = work.path().to_owned();
        drop(work);
        if choice == "invalid-body" {
            fs::write(&path, [0xff, 0xfe]).unwrap();
        }
        let external = fs::read(&path).unwrap();
        let mut reopened = Workspace::open_project(&root).unwrap();
        assert_eq!(reopened.text(), "重启后应保留的本地稿");
        assert!(reopened.external_conflict().is_some());
        assert!(reopened.dispatch(Action::Save).is_err());
        assert_eq!(fs::read(&path).unwrap(), external);
        let copy = dir.path().join("local.md");
        reopened.save_conflict_copy(&copy).unwrap();
        assert_eq!(fs::read_to_string(copy).unwrap(), "重启后应保留的本地稿");
        if choice == "load" {
            reopened.load_external().unwrap();
            assert_eq!(reopened.text(), "重启前的外部稿");
        } else {
            reopened.overwrite_external().unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), "重启后应保留的本地稿");
        }
        let backup = reopened.last_conflict_backup().unwrap();
        assert_eq!(fs::read(backup.join("disk.bin")).unwrap(), external);
        assert!(backup.join("abandoned-journal.json").is_file());
        assert!(!root.join(".novelite-journal.json").exists());
    }
}

#[test]
fn clean_external_changes_refresh_preserve_cursor_and_snapshot_old_text() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("旧稿正文".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    work.dispatch(Action::Move(Direction::Home)).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    fs::write(work.path(), "外部新稿正文").unwrap();
    work.check_external().unwrap();
    assert_eq!(work.text(), "外部新稿正文");
    assert_eq!(work.cursor(), 3);
    assert!(!work.is_dirty());
    let old = work.history_entries().unwrap()[0].id.clone();
    assert_eq!(work.preview_snapshot(&old).unwrap(), "旧稿正文");
    work.dispatch(Action::Insert("本地".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    work.check_external().unwrap();
    assert!(work.external_conflict().is_none());
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "外本地部新稿正文");
}

#[test]
fn dirty_external_changes_block_save_and_loading_disk_keeps_a_local_recovery_copy() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("共同起点".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("本地新稿".into())).unwrap();
    fs::write(work.path(), "外部新稿").unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    assert!(work.external_conflict().is_some());
    assert_eq!(work.text(), "本地新稿");
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "外部新稿");
    work.load_external().unwrap();
    assert_eq!(work.text(), "外部新稿");
    assert!(work.external_conflict().is_none());
    let backup = work.last_conflict_backup().unwrap();
    assert_eq!(
        fs::read_to_string(backup.join("local.md")).unwrap(),
        "本地新稿"
    );
    assert_eq!(
        fs::read_to_string(backup.join("disk.bin")).unwrap(),
        "外部新稿"
    );
}

#[test]
fn external_structure_refresh_and_invalid_or_deleted_data_never_erase_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("安全原稿".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    let manifest = root.join("novelite.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    data["title"] = "外部书名".into();
    data["volumes"][0]["chapters"][0]["title"] = "外部章名".into();
    fs::write(&manifest, serde_json::to_vec(&data).unwrap()).unwrap();
    work.check_external().unwrap();
    assert_eq!(work.project().unwrap().title, "外部书名");
    assert_eq!(work.active_chapter().unwrap().title, "外部章名");
    work.dispatch(Action::Insert("续写".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(saved["title"], "外部书名");
    fs::write(&manifest, "破损清单").unwrap();
    work.check_external().unwrap();
    assert!(work.external_conflict().is_some());
    assert!(work.dispatch(Action::Save).is_err());
    assert_eq!(work.text(), "安全原稿续写");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), "破损清单");
    fs::write(&manifest, serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
    assert!(work.load_external().is_err()); // the observed disk changed again
    work.load_external().unwrap();
    fs::remove_file(work.path()).unwrap();
    work.check_external().unwrap();
    assert!(work.external_conflict().is_some());
    assert_eq!(work.text(), "安全原稿续写");
    assert!(work.load_external().is_err());
}

#[test]
fn conflict_choices_recheck_disk_and_explicit_overwrite_keeps_external_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("本地正文".into())).unwrap();
    fs::write(work.path(), "外部一稿").unwrap();
    work.check_external().unwrap();
    fs::write(work.path(), "外部二稿").unwrap();
    assert!(work.overwrite_external().is_err());
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "外部二稿");
    let copy = dir.path().join("local.md");
    work.save_conflict_copy(&copy).unwrap();
    assert_eq!(fs::read_to_string(&copy).unwrap(), "本地正文");
    assert!(work.external_conflict().is_some());
    work.overwrite_external().unwrap();
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "本地正文");
    assert_eq!(
        fs::read_to_string(work.last_conflict_backup().unwrap().join("disk.bin")).unwrap(),
        "外部二稿"
    );
    assert!(work.external_conflict().is_none());
    fs::remove_file(work.path()).unwrap();
    work.check_external().unwrap();
    work.overwrite_external().unwrap();
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "本地正文");
    drop(work);
    assert_eq!(Workspace::open_project(&root).unwrap().text(), "本地正文");
}

#[test]
fn revisiting_cached_chapters_and_readonly_instances_refresh_from_disk() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut writer = Workspace::create_project(&root, "书").unwrap();
    let first = writer.active_chapter().unwrap().id.clone();
    let first_path = writer.path().to_owned();
    let volume = writer.project().unwrap().volumes[0].id.clone();
    writer.dispatch(Action::Insert("缓存旧稿".into())).unwrap();
    let second = writer.create_chapter(&volume, "第二章").unwrap();
    writer.switch_chapter(&second).unwrap();
    fs::write(&first_path, "外部更新缓存稿").unwrap();
    writer.switch_chapter(&first).unwrap();
    assert_eq!(writer.text(), "外部更新缓存稿");
    let mut reader = Workspace::open_project(&root).unwrap();
    assert!(reader.is_read_only());
    writer.dispatch(Action::SelectAll).unwrap();
    writer
        .dispatch(Action::Insert("写实例新稿".into()))
        .unwrap();
    writer.dispatch(Action::Save).unwrap();
    reader.check_external().unwrap();
    assert_eq!(reader.text(), "写实例新稿");
    assert!(reader.dispatch(Action::Insert("禁止写入".into())).is_err());
    reader.dispatch(Action::Quit).unwrap();
}

#[test]
fn background_poll_refreshes_idle_editor_without_treating_own_saves_as_conflicts() {
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "书").unwrap();
    work.dispatch(Action::Insert("后台原稿".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    fs::write(work.path(), "后台外部更新").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while work.text() != "后台外部更新" {
        work.tick(Instant::now()).unwrap();
        assert!(Instant::now() < deadline, "idle editor did not refresh");
        std::thread::sleep(Duration::from_millis(5));
    }
    work.dispatch(Action::Insert("本地".into())).unwrap();
    work.request_save().unwrap();
    work.poll_save().unwrap();
    work.dispatch(Action::Save).unwrap();
    work.check_external().unwrap();
    assert!(work.external_conflict().is_none());
}

#[test]
fn conflict_commands_default_to_cancel_and_require_typed_overwrite_confirmation() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "书").unwrap();
    work.dispatch(Action::Insert("保留本地".into())).unwrap();
    fs::write(work.path(), "保留外部").unwrap();
    work.check_external().unwrap();
    work.execute_command("conflicts").unwrap();
    assert!(work.navigation().unwrap().entries[0].label.contains("取消"));
    work.handle_event(enter.clone()).unwrap();
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "保留外部");
    work.execute_command("conflicts").unwrap();
    work.handle_event(Event::Paste("明确覆盖".into())).unwrap();
    work.handle_event(enter.clone()).unwrap();
    assert!(work.prompt().unwrap().title.contains("覆盖"));
    work.handle_event(Event::Paste("覆盖".into())).unwrap();
    work.handle_event(enter).unwrap();
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "保留本地");
    assert!(work.external_conflict().is_none());
}

#[test]
fn save_worker_stops_a_late_external_edit_and_loading_disk_abandons_only_backed_up_journal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("本地候选".into())).unwrap();
    fs::write(work.path(), "保存前外部稿").unwrap();
    work.request_save().unwrap(); // bypass polling, so the worker must guard the actual write
    assert!(work.dispatch(Action::Save).is_err());
    assert!(work.external_conflict().is_some());
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "保存前外部稿");
    assert!(root.join(".novelite-journal.json").exists());
    work.load_external().unwrap();
    assert!(
        work.last_conflict_backup()
            .unwrap()
            .join("abandoned-journal.json")
            .is_file()
    );
    let backup_name = work
        .last_conflict_backup()
        .unwrap()
        .file_name()
        .unwrap()
        .to_owned();
    let copy = dir.path().join("copy");
    work.save_as(&copy).unwrap();
    assert_eq!(
        fs::read_to_string(copy.join("conflicts").join(backup_name).join("local.md")).unwrap(),
        "本地候选"
    );
    drop(work);
    assert_eq!(
        Workspace::open_project(&root).unwrap().text(),
        "保存前外部稿"
    );
}

#[test]
fn loading_external_structure_after_current_chapter_removal_preserves_removed_local_text() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.dispatch(Action::Insert("被移除章的本地稿".into()))
        .unwrap();
    let second = work.create_chapter(&volume, "现存章节").unwrap();
    let manifest = root.join("novelite.json");
    let mut data: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    data["volumes"][0]["chapters"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    fs::write(&manifest, serde_json::to_vec(&data).unwrap()).unwrap();
    fs::remove_file(root.join(format!("chapters/{first}.md"))).unwrap();
    work.check_external().unwrap();
    assert!(work.external_conflict().is_some());
    work.load_external().unwrap();
    assert_eq!(work.active_chapter().unwrap().id, second);
    assert_eq!(work.text(), "");
    assert_eq!(
        fs::read_to_string(work.last_conflict_backup().unwrap().join("local.md")).unwrap(),
        "被移除章的本地稿"
    );
}

#[test]
fn partial_own_transaction_is_not_external_but_later_foreign_edit_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("本地已写入部分事务".into()))
        .unwrap();
    let manifest = root.join("novelite.json");
    let original = fs::metadata(&manifest).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&manifest, readonly).unwrap();
    assert!(work.dispatch(Action::Save).is_err());
    assert!(root.join(".novelite-journal.json").exists());
    work.check_external().unwrap();
    assert!(work.external_conflict().is_none());
    fs::set_permissions(&manifest, original).unwrap();
    fs::write(work.path(), "事务失败后的外部稿").unwrap();
    work.check_external().unwrap();
    assert!(work.external_conflict().is_some());
    work.load_external().unwrap();
    drop(work);
    assert_eq!(
        Workspace::open_project(&root).unwrap().text(),
        "事务失败后的外部稿"
    );
}
