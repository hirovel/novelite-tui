use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};
use std::fs;

fn submit(
    work: &mut Workspace,
    path: &std::path::Path,
) -> std::io::Result<Option<novelite_tui::Outcome>> {
    work.execute_command("open-project")?;
    work.handle_event(Event::Paste(path.to_str().unwrap().into()))?;
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
}

#[test]
fn command_opens_existing_unicode_manuscript_and_preserves_current_draft_and_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let next = dir.path().join("雨夜 手稿.MD");
    fs::write(&first, "原文").unwrap();
    fs::write(&next, "下一篇 / 正文").unwrap();
    let mut w = Workspace::open(&first).unwrap();
    w.configure_mouse(true);
    w.set_design_motion(false);
    w.dispatch(Action::Insert("修改".into())).unwrap();
    submit(&mut w, &next).unwrap();
    assert_eq!(w.text(), "下一篇 / 正文");
    assert_eq!(fs::read_to_string(&first).unwrap(), "修改原文");
    assert!(w.mouse_enabled());
    assert!(!w.design_motion_enabled());
    assert!(w.prompt().is_none());
    assert_eq!(w.path(), fs::canonicalize(&next).unwrap());
    let book = dir.path().join("book");
    drop(Workspace::create_project(&book, "作品").unwrap());
    submit(&mut w, &book).unwrap();
    assert_eq!(w.project().unwrap().title, "作品");
}

#[test]
fn failed_open_or_save_preserves_current_buffer_and_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let next = dir.path().join("next.md");
    fs::write(&first, "原文").unwrap();
    fs::write(&next, "新文件").unwrap();
    let mut w = Workspace::open(&first).unwrap();
    w.dispatch(Action::Insert("未保存".into())).unwrap();
    assert!(submit(&mut w, &dir.path().join("missing.txt")).is_err());
    assert_eq!(w.text(), "未保存原文");
    assert!(w.prompt().is_some());
    assert!(!dir.path().join("missing.txt").exists());
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    fs::rename(&first, dir.path().join("backup.txt")).unwrap();
    fs::create_dir(&first).unwrap();
    assert!(submit(&mut w, &next).is_err());
    assert_eq!(w.text(), "未保存原文");
    assert!(w.prompt().is_some());
    assert_eq!(fs::read_to_string(&next).unwrap(), "新文件");
}

#[test]
fn standalone_open_cannot_bypass_managed_chapter_lock_or_edit_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let managed = Workspace::create_project(&root, "锁保护").unwrap();
    let first = dir.path().join("first.txt");
    fs::write(&first, "当前文稿").unwrap();
    let mut w = Workspace::open(&first).unwrap();
    assert!(
        w.switch_path(managed.path())
            .unwrap_err()
            .to_string()
            .contains("作品内")
    );
    assert!(w.switch_path(root.join("novelite.json")).is_err());
    assert_eq!(w.text(), "当前文稿");
    assert!(w.project().is_none());
    let mut managed = managed;
    managed
        .dispatch(Action::Insert("未保存章节".into()))
        .unwrap();
    let same = managed.path().to_owned();
    managed.switch_path(&same).unwrap();
    assert_eq!(managed.text(), "未保存章节");
    assert!(managed.is_dirty());
}
