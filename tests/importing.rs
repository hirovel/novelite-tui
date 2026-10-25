use novelite_tui::{Action, ImportEncoding, Workspace};

fn settle(work: &mut Workspace) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while work.import_busy() {
        work.tick(std::time::Instant::now())?;
        assert!(
            std::time::Instant::now() < deadline,
            "import did not complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Ok(())
}
fn prepare(
    work: &mut Workspace,
    source: impl AsRef<std::path::Path>,
    encoding: ImportEncoding,
) -> std::io::Result<()> {
    work.prepare_import(source, encoding)?;
    settle(work)
}
fn confirm(work: &mut Workspace, target: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    work.confirm_import(target)?;
    settle(work)
}

#[test]
fn preview_then_confirm_creates_a_new_book_and_keeps_every_source_character() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("旧稿.txt");
    let original = "前言\r\n第一卷 雨夜\r\n第一章 来信\r\n　　正文。\r\n第二章 离开\r\n尾声";
    std::fs::write(&source, original).unwrap();
    let mut work = Workspace::open(dir.path().join("current.md")).unwrap();
    work.dispatch(Action::Insert("当前草稿".into())).unwrap();
    let destination = dir.path().join("imported");
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    let preview = work.import_preview().unwrap();
    assert_eq!(preview.title, "旧稿");
    assert_eq!(
        preview
            .chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["旧稿", "卷首", "第一章 来信", "第二章 离开"]
    );
    assert_eq!(
        preview
            .chapters
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>(),
        original
    );
    assert!(!destination.exists());
    assert_eq!(work.text(), "当前草稿");
    confirm(&mut work, &destination).unwrap();
    assert!(work.import_preview().is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("current.md")).unwrap(),
        "当前草稿"
    );
    let chapters = work
        .project()
        .unwrap()
        .volumes
        .iter()
        .flat_map(|v| v.chapters.iter())
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(chapters.len(), 4);
    let disk = chapters
        .iter()
        .map(|c| std::fs::read_to_string(destination.join(&c.path)).unwrap())
        .collect::<String>();
    assert_eq!(disk, original);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), original);
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let reopened = Workspace::open_project(destination).unwrap();
    assert_eq!(reopened.text(), "前言\r\n");
}

#[test]
fn incomplete_import_is_never_opened_as_a_complete_book() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let work = Workspace::create_project(&root, "完整书").unwrap();
    drop(work);
    std::fs::write(root.join(".novelite-importing"), "interrupted").unwrap();
    assert!(Workspace::open_project(&root).is_err());
    assert!(root.join("novelite.json").exists());
}

#[cfg(windows)]
#[test]
fn directory_junctions_cannot_import_or_publish_outside_the_selected_tree() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("outside");
    let source = dir.path().join("source");
    std::fs::create_dir(&outside).unwrap();
    std::fs::create_dir(&source).unwrap();
    std::fs::write(outside.join("private.txt"), "保留").unwrap();
    let link = source.join("linked");
    let output = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut work = Workspace::open(dir.path().join("current.md")).unwrap();
    assert!(prepare(&mut work, &source, ImportEncoding::Utf8).is_err());
    prepare(&mut work, outside.join("private.txt"), ImportEncoding::Utf8).unwrap();
    assert!(confirm(&mut work, link.join("book")).is_err());
    assert!(!outside.join("book").exists());
    assert_eq!(
        std::fs::read_to_string(outside.join("private.txt")).unwrap(),
        "保留"
    );
    std::fs::remove_dir(link).unwrap();
}

#[cfg(windows)]
#[test]
fn a_real_write_denial_after_directory_creation_rolls_back_only_the_new_root() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let parent = dir.path().join("denied");
    std::fs::create_dir(&parent).unwrap();
    let source = dir.path().join("source.txt");
    std::fs::write(&source, "源稿").unwrap();
    let mut work = Workspace::open(dir.path().join("current.md")).unwrap();
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    let account = String::from_utf8(Command::new("whoami").output().unwrap().stdout).unwrap();
    let denied = Command::new("icacls")
        .arg(&parent)
        .args(["/deny", &format!("{}:(OI)(CI)(WD)", account.trim())])
        .output()
        .unwrap();
    assert!(denied.status.success());
    let probe = parent.join("directory-creation-allowed");
    let can_create_directory = std::fs::create_dir(&probe).is_ok();
    let _ = std::fs::remove_dir(&probe);
    let target = parent.join("new-book");
    let result = confirm(&mut work, &target);
    let cleaned = !target.exists();
    let restored = Command::new("icacls")
        .arg(&parent)
        .args(["/remove:d", account.trim()])
        .output()
        .unwrap();
    assert!(restored.status.success());
    assert!(can_create_directory);
    assert!(result.is_err());
    assert!(cleaned);
    assert!(work.import_preview().is_some());
    assert_eq!(std::fs::read_to_string(source).unwrap(), "源稿");
}

#[test]
fn import_module_cancellation_keeps_drafts_and_restores_navigation() {
    use novelite_tui::ModuleId;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.txt");
    std::fs::write(&source, "# 原稿\n正文").unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "现有书").unwrap();
    work.execute_command("chapters").unwrap();
    work.execute_command("import-manuscript").unwrap();
    work.handle_event(crossterm::event::Event::Paste(
        source.to_str().unwrap().into(),
    ))
    .unwrap();
    work.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
    ))
    .unwrap();
    work.set_module_enabled(ModuleId::Import, false).unwrap();
    settle(&mut work).unwrap();
    assert!(work.import_preview().is_none());
    assert!(work.module_focused(ModuleId::Chapters));
    assert!(work.execute_command("import-manuscript").is_err());
    work.set_module_enabled(ModuleId::Import, true).unwrap();
    work.execute_command("edit-outline").unwrap();
    work.dispatch(Action::Insert("未提交的大纲".into()))
        .unwrap();
    assert!(prepare(&mut work, source, ImportEncoding::Utf8).is_err());
    assert!(work.prompt().is_some());
    work.dispatch(Action::Save).unwrap();
    assert_eq!(work.chapter_plan().unwrap().outline, "未提交的大纲");
}

#[test]
fn terminal_commands_require_encoding_preview_and_a_new_destination_before_import() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};
    fn key(w: &mut Workspace, code: KeyCode) {
        w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
            .unwrap();
    }
    fn submit(w: &mut Workspace, text: &str) {
        w.handle_event(Event::Paste(text.into())).unwrap();
        key(w, KeyCode::Enter);
        settle(w).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("old.txt");
    std::fs::write(&source, [0xd6, 0xd0, 0xce, 0xc4]).unwrap();
    let mut work = Workspace::open(dir.path().join("current.md")).unwrap();
    work.execute_command("import-manuscript").unwrap();
    submit(&mut work, source.to_str().unwrap());
    assert!(work.prompt().unwrap().title.contains("编码"));
    submit(&mut work, "gb18030");
    assert!(work.prompt().is_none());
    assert_eq!(work.import_preview().unwrap().chapters[0].text, "中文");
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    let symbols: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(symbols.replace(' ', "").contains("仅迁移正文"));
    assert!(symbols.contains("GB18030"));
    key(&mut work, KeyCode::Enter);
    let target = dir.path().join("confirmed");
    assert!(!target.exists());
    submit(&mut work, target.to_str().unwrap());
    assert!(work.import_preview().is_none());
    assert_eq!(work.text(), "中文");
    work.execute_command("import-manuscript").unwrap();
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.text(), "中文");
}

#[test]
fn stale_sources_nested_targets_and_existing_destinations_never_publish_a_book() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("稿.txt"), "预览稿").unwrap();
    let mut work = Workspace::open(dir.path().join("current.md")).unwrap();
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    assert!(confirm(&mut work, source.join("nested")).is_err());
    assert!(!source.join("nested").exists());
    let occupied = dir.path().join("occupied");
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("keep"), "用户文件").unwrap();
    assert!(confirm(&mut work, &occupied).is_err());
    assert_eq!(
        std::fs::read_to_string(occupied.join("keep")).unwrap(),
        "用户文件"
    );
    std::fs::write(source.join("稿.txt"), "后来修改").unwrap();
    let target = dir.path().join("new");
    assert!(confirm(&mut work, &target).is_err());
    assert!(!target.exists());
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    assert!(confirm(&mut work, dir.path().join("CON")).is_err());
    assert!(confirm(&mut work, dir.path().join("source/../escape")).is_err());
    assert!(!dir.path().join("escape").exists());
}

#[test]
fn directory_import_orders_chapters_naturally_and_preserves_unknown_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("旧书");
    std::fs::create_dir_all(source.join("第一卷")).unwrap();
    for (name, text) in [
        ("10.md", "# 十章\n正文十"),
        ("2.md", "# 二章\n```\n# 代码不是章\n```\n正文二"),
        ("1.txt", "正文一"),
    ] {
        std::fs::write(source.join("第一卷").join(name), text).unwrap();
    }
    std::fs::write(source.join("随手记.json"), b"{\"unknown\":true}").unwrap();
    let mut work = Workspace::open(dir.path().join("current.md")).unwrap();
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    let preview = work.import_preview().unwrap();
    assert_eq!(
        preview
            .chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["1", "二章", "十章"]
    );
    assert!(preview.chapters.iter().all(|c| c.volume == "第一卷"));
    assert_eq!(
        preview.chapters[1].text,
        "# 二章\n```\n# 代码不是章\n```\n正文二"
    );
    let root = dir.path().join("new-book");
    confirm(&mut work, &root).unwrap();
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("source-originals/index.json")).unwrap())
            .unwrap();
    assert_eq!(index.as_array().unwrap().len(), 4);
    let note = index
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["original"] == "随手记.json")
        .unwrap();
    assert_eq!(
        std::fs::read(root.join(note["path"].as_str().unwrap())).unwrap(),
        b"{\"unknown\":true}"
    );
}

#[test]
fn legacy_encoding_is_explicit_strict_and_the_original_bytes_are_archived() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("旧稿.txt");
    // Independent GBK bytes for 中文, CRLF; no decoder shares the expected literal.
    let original = [0xd6, 0xd0, 0xce, 0xc4, 13, 10];
    std::fs::write(&source, original).unwrap();
    let mut work = Workspace::open(dir.path().join("scratch.md")).unwrap();
    assert!(prepare(&mut work, &source, ImportEncoding::Utf8).is_err());
    assert!(work.import_preview().is_none());
    prepare(&mut work, &source, ImportEncoding::Gb18030).unwrap();
    assert_eq!(work.import_preview().unwrap().chapters[0].text, "中文\r\n");
    let root = dir.path().join("converted");
    confirm(&mut work, &root).unwrap();
    assert_eq!(work.text(), "中文\r\n");
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("source-originals/index.json")).unwrap())
            .unwrap();
    assert_eq!(
        std::fs::read(root.join(index[0]["path"].as_str().unwrap())).unwrap(),
        original
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    std::fs::write(&source, [0xff, 0xfe, 0x2d, 0x4e, 0x87, 0x65, 13, 0, 10, 0]).unwrap();
    assert!(prepare(&mut work, &source, ImportEncoding::Utf8).is_err());
    prepare(&mut work, &source, ImportEncoding::Utf16Le).unwrap();
    assert_eq!(work.import_preview().unwrap().chapters[0].text, "中文\r\n");
    assert!(prepare(&mut work, &source, ImportEncoding::Utf16Be).is_err());
    std::fs::write(&source, [0xff, 0xfe, 0x00, 0xd8]).unwrap();
    assert!(prepare(&mut work, &source, ImportEncoding::Utf16Le).is_err());
}

#[test]
fn gb18030_prefix_is_text_and_preview_shortcuts_do_not_edit_the_hidden_draft() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.txt");
    std::fs::write(&source, [0xef, 0xbb, 0xbf, 0x41]).unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("当前草稿".into())).unwrap();
    prepare(&mut work, &source, ImportEncoding::Gb18030).unwrap();
    assert_eq!(work.import_preview().unwrap().chapters[0].text, "锘緼");
    for code in ['z', 'a', 'x', 'v'] {
        let _ = work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char(code),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(work.text(), "当前草稿");
    }
    assert!(work.execute_command("undo").is_err());
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert!(work.palette().is_some());
    work.handle_event(Event::Paste("import-encoding".into()))
        .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert!(work.prompt().unwrap().title.contains("编码"));
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    prepare(&mut work, &source, ImportEncoding::Gb18030).unwrap();
    work.handle_event(Event::Paste("误粘贴".into())).unwrap();
    assert_eq!(work.text(), "当前草稿");
}

#[test]
fn background_preview_is_single_flight_and_cancelled_results_cannot_reopen_it() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.txt");
    std::fs::write(&source, "第一章 开始\n正文\n".repeat(20000)).unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("保留稿".into())).unwrap();
    work.prepare_import(&source, ImportEncoding::Utf8).unwrap();
    assert!(work.import_busy());
    assert!(work.import_preview().is_none());
    assert!(work.prepare_import(&source, ImportEncoding::Utf8).is_err());
    // Cancellation is handled before any completion is polled, without joining the worker.
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    // Before the cancelled result is collected, focus is already usable again.
    work.handle_event(Event::Paste("继续".into())).unwrap();
    assert_eq!(work.text(), "保留稿继续");
    settle(&mut work).unwrap();
    assert!(work.import_preview().is_none());
    assert_eq!(work.text(), "保留稿继续");
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    let target = dir.path().join("new-book");
    // Use a small source for publication; source comparison still happens in the worker.
    std::fs::write(&source, "第一章 开始\n正文\n").unwrap();
    prepare(&mut work, &source, ImportEncoding::Utf8).unwrap();
    work.confirm_import(&target).unwrap();
    assert!(work.import_busy());
    assert!(work.dispatch(Action::Quit).is_err());
    assert!(work.execute_command("import-manuscript").is_err());
    settle(&mut work).unwrap();
    assert_eq!(work.text(), "第一章 开始\n正文\n");
    assert!(target.join("novelite.json").exists());
}
