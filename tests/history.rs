use novelite_tui::{Action, Workspace};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[test]
fn history_is_separate_from_saving_deduplicates_and_restores_with_a_current_backup() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let clock = Arc::new(AtomicU64::new(1_000));
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let clock_ref = clock.clone();
    work.set_history_clock(Arc::new(move || clock_ref.load(Ordering::SeqCst)));
    work.history_tick().unwrap();
    work.dispatch(Action::Insert("第一稿".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    assert!(work.history_entries().unwrap().is_empty());
    clock.store(1_299, Ordering::SeqCst);
    work.history_tick().unwrap();
    assert!(work.history_entries().unwrap().is_empty());
    clock.store(1_300, Ordering::SeqCst);
    work.history_tick().unwrap();
    let entries = work.history_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let old = entries[0].id.clone();
    assert_eq!(work.preview_snapshot(&old).unwrap(), "第一稿");
    clock.store(1_600, Ordering::SeqCst);
    work.history_tick().unwrap();
    assert_eq!(work.history_entries().unwrap().len(), 1);
    work.mark_snapshot().unwrap();
    assert!(work.history_entries().unwrap()[0].marked);
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("恢复前当前稿".into()))
        .unwrap();
    work.restore_snapshot(&old).unwrap();
    assert_eq!(work.text(), "第一稿");
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "第一稿");
    let backup = work
        .history_entries()
        .unwrap()
        .into_iter()
        .find(|e| e.id != old)
        .unwrap();
    assert_eq!(work.preview_snapshot(&backup.id).unwrap(), "恢复前当前稿");
    drop(work);
    let mut reopened = Workspace::open_project(&root).unwrap();
    assert_eq!(reopened.preview_snapshot(&old).unwrap(), "第一稿");
    reopened.restore_snapshot(&backup.id).unwrap();
    assert_eq!(fs::read_to_string(reopened.path()).unwrap(), "恢复前当前稿");
}

#[test]
fn switching_and_closing_capture_changes_and_save_as_keeps_history() {
    let folder = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    let original = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let other = work.create_chapter(&volume, "另一章").unwrap();
    work.dispatch(Action::Insert("切章前正文".into())).unwrap();
    work.switch_chapter(&other).unwrap();
    work.switch_chapter(&original).unwrap();
    let snapshot = work.history_entries().unwrap()[0].id.clone();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("关闭前正文".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(work.history_entries().unwrap().len(), 2);
    work.save_as(folder.path().join("copy")).unwrap();
    drop(work);
    let mut work = Workspace::open_project(folder.path().join("copy")).unwrap();
    work.restore_snapshot(&snapshot).unwrap();
    assert_eq!(work.text(), "切章前正文");
    let entries = work.history_entries().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| work.preview_snapshot(&e.id).unwrap() == "关闭前正文")
    );
    assert_eq!(entries.len(), 2);
}

#[test]
fn retention_preserves_marked_and_daily_points_and_capacity_failure_keeps_manuscript() {
    use novelite_tui::HistoryPolicy;
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    let clock = Arc::new(AtomicU64::new(86_400));
    let c = clock.clone();
    work.set_history_clock(Arc::new(move || c.load(Ordering::SeqCst)));
    work.set_history_policy(HistoryPolicy {
        recent: 2,
        days: 2,
        max_bytes: 1_000_000,
    })
    .unwrap();
    work.dispatch(Action::Insert("永久标记".into())).unwrap();
    work.mark_snapshot().unwrap();
    let marked = work.history_entries().unwrap()[0].id.clone();
    for (time, text) in [
        (172_800, "昨天"),
        (259_200, "今天一"),
        (259_201, "今天二"),
        (259_202, "今天三"),
    ] {
        clock.store(time, Ordering::SeqCst);
        work.dispatch(Action::SelectAll).unwrap();
        work.dispatch(Action::Insert(text.into())).unwrap();
        work.snapshot_before_change().unwrap();
    }
    let entries = work.history_entries().unwrap();
    let texts: Vec<_> = entries
        .iter()
        .map(|e| work.preview_snapshot(&e.id).unwrap())
        .collect();
    assert_eq!(texts, ["今天三", "今天二", "昨天", "永久标记"]);
    assert!(entries.iter().find(|e| e.id == marked).unwrap().marked);
    assert!(work.history_usage().unwrap() > 0);
    work.set_history_policy(HistoryPolicy {
        recent: 2,
        days: 2,
        max_bytes: 1,
    })
    .unwrap();
    work.dispatch(Action::Insert("容量不足时输入".into()))
        .unwrap();
    assert!(work.restore_snapshot(&marked).is_err());
    assert_eq!(work.text(), "今天三容量不足时输入");
    assert!(work.history_warning().unwrap().contains("容量"));
    work.dispatch(Action::Save).unwrap();
    assert_eq!(
        fs::read_to_string(work.path()).unwrap(),
        "今天三容量不足时输入"
    );
    drop(work);
    let mut work = Workspace::open_project(&root).unwrap();
    assert_eq!(work.history_policy().unwrap().max_bytes, 1);
    assert_eq!(work.preview_snapshot(&marked).unwrap(), "永久标记");
}

#[test]
fn history_commands_preview_without_editing_then_restore_and_configure_policy() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let folder = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    work.dispatch(Action::Insert("旧稿\n第二行".into()))
        .unwrap();
    work.execute_command("mark-snapshot").unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("新稿".into())).unwrap();
    work.execute_command("history").unwrap();
    assert!(work.navigation().unwrap().title.contains("历史"));
    work.handle_event(key(KeyCode::Enter)).unwrap();
    assert_eq!(work.history_preview(40, 10).unwrap().lines[0], "旧稿");
    work.handle_event(Event::Paste("误粘贴".into())).unwrap();
    assert_eq!(work.text(), "新稿");
    work.handle_event(key(KeyCode::Enter)).unwrap();
    assert_eq!(work.text(), "旧稿\n第二行");
    assert!(work.history_preview(40, 10).is_none());
    work.execute_command("history-settings").unwrap();
    work.handle_event(Event::Paste("10,3,64".into())).unwrap();
    work.handle_event(key(KeyCode::Enter)).unwrap();
    assert_eq!(work.history_policy().unwrap().recent, 10);
    assert_eq!(work.history_policy().unwrap().days, 3);
    assert_eq!(work.history_policy().unwrap().max_bytes, 64 * 1024 * 1024);
}

#[test]
fn corrupt_snapshot_and_failed_history_write_preserve_current_text_and_saved_body() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("恢复点".into())).unwrap();
    work.mark_snapshot().unwrap();
    let id = work.history_entries().unwrap()[0].id.clone();
    let chapter = work.active_chapter().unwrap().id.clone();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("当前稿绝不丢失".into()))
        .unwrap();
    work.dispatch(Action::Save).unwrap();
    let path = root.join(format!("history/{chapter}/{id}.json"));
    let original = fs::read(&path).unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_slice(&original).unwrap();
    corrupt["text"] = "被损坏的文字".into();
    fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    assert!(work.restore_snapshot(&id).is_err());
    assert_eq!(work.text(), "当前稿绝不丢失");
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "当前稿绝不丢失");
    fs::write(&path, original).unwrap();
    fs::rename(root.join("history"), root.join("history-backup")).unwrap();
    fs::write(root.join("history"), "路径已占用").unwrap();
    assert!(work.mark_snapshot().is_err());
    assert!(work.history_warning().is_some());
    assert_eq!(fs::read_to_string(work.path()).unwrap(), "当前稿绝不丢失");
    fs::remove_file(root.join("history")).unwrap();
    fs::rename(root.join("history-backup"), root.join("history")).unwrap();
    work.mark_snapshot().unwrap();
    assert_eq!(work.history_entries().unwrap().len(), 2);
}

#[test]
fn undo_to_initial_revision_is_still_snapshotted_when_latest_history_differs() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut work = Workspace::create_project(&root, "书").unwrap();
    work.dispatch(Action::Insert("原始正文".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    drop(work);
    let mut work = Workspace::open_project(&root).unwrap();
    work.dispatch(Action::Insert("新版：".into())).unwrap();
    work.mark_snapshot().unwrap();
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "原始正文");
    work.dispatch(Action::Quit).unwrap();
    let latest = work.history_entries().unwrap()[0].id.clone();
    assert_eq!(work.preview_snapshot(&latest).unwrap(), "原始正文");
}

#[test]
fn switching_chapters_closes_old_history_preview_and_list() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let folder = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.create_chapter(&volume, "下一章").unwrap();
    work.dispatch(Action::Insert("旧章正文".into())).unwrap();
    work.mark_snapshot().unwrap();
    work.execute_command("history").unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    work.execute_command("next-chapter").unwrap();
    assert!(work.history_preview(40, 10).is_none());
    work.handle_event(Event::Paste("新章输入".into())).unwrap();
    assert_eq!(work.text(), "新章输入");
    work.switch_chapter(&first).unwrap();
    work.execute_command("history").unwrap();
    work.execute_command("next-chapter").unwrap();
    assert!(work.navigation().is_none());
}
