use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};
use std::fs;

fn enter(work: &mut Workspace) {
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
}
fn choose(work: &mut Workspace, query: &str) {
    work.handle_event(Event::Paste(query.into())).unwrap();
    enter(work);
}

#[test]
fn structure_commands_offer_destination_and_explicit_recycle_confirmation() {
    let folder = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    let id = work.active_chapter().unwrap().id.clone();
    work.dispatch(Action::Insert("保留原稿".into())).unwrap();
    let target = work.create_volume("尾卷").unwrap();
    work.execute_command("copy-chapter").unwrap();
    choose(&mut work, "尾卷");
    assert!(work.prompt().unwrap().title.contains("复制"));
    choose(&mut work, "副本");
    work.execute_command("move-chapter").unwrap();
    choose(&mut work, "尾卷");
    assert_eq!(work.project().unwrap().volumes[1].chapters[1].id, id);
    work.execute_command("chapter-up").unwrap();
    assert_eq!(work.project().unwrap().volumes[1].chapters[0].id, id);
    work.execute_command("recycle-chapter").unwrap();
    let view = work.navigation().unwrap();
    assert!(
        view.entries
            .iter()
            .any(|e| e.label.contains("第一章") && e.label.contains("1 章"))
    );
    enter(&mut work); // default is cancellation
    assert!(work.project().unwrap().recycle_bin.is_empty());
    work.execute_command("recycle-chapter").unwrap();
    choose(&mut work, "移入回收区");
    assert_eq!(work.project().unwrap().recycle_bin.len(), 1);
    work.execute_command("recycle-bin").unwrap();
    choose(&mut work, "第一章");
    choose(&mut work, "尾卷");
    assert!(work.project().unwrap().recycle_bin.is_empty());
    work.switch_chapter(&id).unwrap();
    assert_eq!(work.text(), "保留原稿");
    work.execute_command("number-chapters").unwrap();
    assert!(work.active_chapter().unwrap().title.starts_with("第2章"));
    // Choose the whole volume from the directory; its confirmation names its full scope.
    work.execute_command("chapters").unwrap();
    choose(&mut work, "尾卷");
    work.execute_command("recycle-volume").unwrap();
    assert!(
        work.navigation()
            .unwrap()
            .entries
            .iter()
            .any(|e| e.label.contains("尾卷") && e.label.contains("2 章"))
    );
    choose(&mut work, "移入回收区");
    assert!(
        work.project()
            .unwrap()
            .volumes
            .iter()
            .all(|v| v.id != target)
    );
}

#[test]
fn recycled_chapters_survive_reopen_restore_and_save_as_without_overwriting_names() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let original = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.dispatch(Action::Insert("不可丢失的正文".into()))
        .unwrap();
    let recycled = work.recycle_chapter(&original).unwrap();
    assert_ne!(work.active_chapter().unwrap().id, original);
    assert_eq!(work.text(), "");
    assert_eq!(
        work.project().unwrap().recycle_bin[0].chapters[0].id,
        original
    );
    assert_eq!(
        fs::read_to_string(root.join(format!("chapters/{original}.md"))).unwrap(),
        "不可丢失的正文"
    );
    let backup: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(".novelite-v1-backup.json")).unwrap()).unwrap();
    assert_eq!(backup["version"], 1);
    work.save_as(folder.path().join("copy")).unwrap();
    drop(work);
    let mut work = Workspace::open_project(folder.path().join("copy")).unwrap();
    assert_eq!(work.project().unwrap().version, 2);
    work.restore_recycled(&recycled, &volume, 0).unwrap();
    work.switch_chapter(&original).unwrap();
    assert_eq!(work.text(), "不可丢失的正文");
    assert_eq!(work.project().unwrap().volumes[0].chapters.len(), 2);
    assert!(work.project().unwrap().recycle_bin.is_empty());
    let removed_volume = work.recycle_volume(&volume).unwrap();
    let target = work.project().unwrap().volumes[0].id.clone();
    assert_ne!(target, volume);
    assert!(
        work.restore_recycled(&removed_volume, "missing", 0)
            .is_err()
    );
    assert_eq!(work.project().unwrap().recycle_bin.len(), 1);
    work.restore_recycled(&removed_volume, &target, usize::MAX)
        .unwrap();
    work.switch_chapter(&original).unwrap();
    assert_eq!(work.text(), "不可丢失的正文");
    assert_eq!(work.project().unwrap().volumes[0].chapters.len(), 3);
}

#[test]
fn moving_copying_and_numbering_preserve_identity_and_independent_chapter_contents() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let first_volume = work.project().unwrap().volumes[0].id.clone();
    let original = work.active_chapter().unwrap().id.clone();
    work.rename_chapter(&original, "初遇").unwrap();
    work.dispatch(Action::Insert("独立正文".into())).unwrap();
    let second_volume = work.create_volume("下卷").unwrap();
    let copy = work
        .copy_chapter(&original, &second_volume, "重逢")
        .unwrap();
    assert_ne!(copy, original);
    work.move_chapter(&original, &second_volume, 1).unwrap();
    work.move_volume(&second_volume, 0).unwrap();
    work.number_chapters().unwrap();
    work.switch_chapter(&copy).unwrap();
    work.dispatch(Action::Insert("副本：".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut reopened = Workspace::open_project(&root).unwrap();
    let project = reopened.project().unwrap();
    assert_eq!(project.volumes[0].id, second_volume);
    assert_eq!(project.volumes[1].id, first_volume);
    assert_eq!(project.volumes[0].chapters[0].id, copy);
    assert_eq!(project.volumes[0].chapters[0].title, "第1章 重逢");
    assert_eq!(project.volumes[0].chapters[1].id, original);
    assert_eq!(project.volumes[0].chapters[1].title, "第2章 初遇");
    assert_eq!(
        fs::read_to_string(reopened.path()).unwrap(),
        "副本：独立正文"
    );
    reopened.switch_chapter(&original).unwrap();
    assert_eq!(fs::read_to_string(reopened.path()).unwrap(), "独立正文");
}

#[test]
fn missing_recycled_body_cannot_be_restored_as_a_dangling_chapter() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let id = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let item = work.recycle_chapter(&id).unwrap();
    fs::remove_file(root.join(format!("chapters/{id}.md"))).unwrap();
    assert!(work.restore_recycled(&item, &volume, 0).is_err());
    assert_eq!(work.project().unwrap().recycle_bin.len(), 1);
    assert_eq!(work.project().unwrap().volumes[0].chapters.len(), 1);
}

#[test]
fn failed_last_chapter_recycling_can_save_later_edits_and_reopen() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let id = work.active_chapter().unwrap().id.clone();
    work.dispatch(Action::Insert("原稿".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    let manifest = root.join("novelite.json");
    let original = fs::metadata(&manifest).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&manifest, readonly).unwrap();
    assert!(work.recycle_chapter(&id).is_err());
    work.dispatch(Action::Insert("失败后继续写作".into()))
        .unwrap();
    fs::set_permissions(&manifest, original).unwrap();
    work.dispatch(Action::Save).unwrap();
    drop(work);
    let work = Workspace::open_project(&root).unwrap();
    assert_eq!(work.text(), "失败后继续写作");
    assert_eq!(
        fs::read_to_string(root.join(format!("chapters/{id}.md"))).unwrap(),
        "原稿"
    );
}
