use novelite_tui::{Action, ExportFormat, Workspace};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn settle(w: &mut Workspace) -> std::io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while w.export_busy() {
        w.tick(Instant::now())?;
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn export(w: &mut Workspace, kind: ExportFormat, path: &Path) -> std::io::Result<()> {
    w.export_manuscript(kind, path)?;
    settle(w)
}
#[test]
fn raw_markdown_exports_the_requested_revision_without_saving_or_switching_the_draft() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(&path, "旧稿").unwrap();
    let mut w = Workspace::open(&path).unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Insert(
        "# 标题\r\n\r\n　正文 👩‍💻 e\u{301}\t\n".into(),
    ))
    .unwrap();
    let target = dir.path().join("export.md");
    w.export_manuscript(ExportFormat::ChapterMarkdown, &target)
        .unwrap();
    assert!(w.export_busy());
    w.dispatch(Action::Insert("稍后修改".into())).unwrap();
    settle(&mut w).unwrap();
    assert_eq!(
        std::fs::read(&target).unwrap(),
        "# 标题\r\n\r\n　正文 👩‍💻 e\u{301}\t\n".as_bytes()
    );
    assert_eq!(w.path(), std::fs::canonicalize(&path).unwrap());
    assert!(w.text().ends_with("稍后修改"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "旧稿");
    assert!(w.export_message().unwrap().contains("已导出"));
}

#[test]
fn text_export_matches_desktop_title_rules_without_reformatting_the_body() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("章节.md")).unwrap();
    for (i, (raw, expected)) in [
        (
            "  \n##  雨夜  \r\n\r\n　　对白。\r\n\n尾\t",
            "雨夜\n\n　　对白。\r\n\n尾\t",
        ),
        ("章节\r\n　不重复标题", "章节\r\n　不重复标题"),
        ("　正文 👩‍💻\r\n", "章节\n\n　正文 👩‍💻\r\n"),
        ("\r\n　 ", "章节\n"),
        ("##", "章节\n\n"),
        ("", "章节\n"),
    ]
    .into_iter()
    .enumerate()
    {
        w.dispatch(Action::SelectAll).unwrap();
        w.dispatch(Action::Insert(raw.into())).unwrap();
        let target = dir.path().join(format!("{i}.txt"));
        export(&mut w, ExportFormat::ChapterText, &target).unwrap();
        assert_eq!(std::fs::read(target).unwrap(), expected.as_bytes());
        assert_eq!(w.text(), raw);
    }
    let empty = dir.path().join("empty.md");
    export(&mut w, ExportFormat::ChapterMarkdown, &empty).unwrap();
    assert_eq!(std::fs::read(empty).unwrap(), b"");
}
#[test]
fn whole_book_text_keeps_volume_order_author_and_empty_chapters() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "夜航").unwrap();
    w.dispatch(Action::Insert("# 开篇\r\n\r\n　第一段\r\n".into()))
        .unwrap();
    let volume = w.create_volume("雨卷").unwrap();
    let chapter = w.create_chapter(&volume, "空章").unwrap();
    w.switch_chapter(&chapter).unwrap();
    let target = dir.path().join("book.txt");
    export(
        &mut w,
        ExportFormat::BookText {
            author: "小雨".into(),
        },
        &target,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(target).unwrap(),
        "《夜航》\n作者：小雨\n\n====================================\n\n\n【第一卷】\n\n\n开篇\n\n　第一段\r\n\n\n\n【雨卷】\n\n\n空章\n\n\n"
    );
    let target = dir.path().join("anonymous.txt");
    export(
        &mut w,
        ExportFormat::BookText {
            author: String::new(),
        },
        &target,
    )
    .unwrap();
    assert!(
        std::fs::read_to_string(target)
            .unwrap()
            .starts_with("《夜航》\n作者：佚名\n")
    );
}

#[test]
fn portable_directory_keeps_live_body_plans_history_and_unknown_files_without_switching() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut w = Workspace::create_project(&root, "迁移").unwrap();
    w.dispatch(Action::Insert("旧文稿".into())).unwrap();
    w.update_chapter_plan(novelite_tui::ChapterPlan {
        outline: "伏笔".into(),
        target_words: Some(2000),
    })
    .unwrap();
    w.dispatch(Action::Save).unwrap();
    w.execute_command("mark-snapshot").unwrap();
    std::fs::create_dir_all(root.join("legacy")).unwrap();
    std::fs::write(root.join("legacy/notes.json"), b"{\"unknown\":true}").unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Insert("新文稿\r\n👩‍💻".into())).unwrap();
    let target = dir.path().join("portable");
    export(&mut w, ExportFormat::ProjectDirectory, &target).unwrap();
    assert!(w.path().starts_with(std::fs::canonicalize(&root).unwrap()));
    assert!(!target.join(".novelite-importing").exists());
    assert_eq!(
        std::fs::read(target.join("legacy/notes.json")).unwrap(),
        b"{\"unknown\":true}"
    );
    let copy = Workspace::open_project(&target).unwrap();
    assert_eq!(copy.text(), "新文稿\r\n👩‍💻");
    assert_eq!(copy.chapter_plan().unwrap().outline, "伏笔");
    assert_eq!(copy.chapter_plan().unwrap().target_words, Some(2000));
    assert_eq!(copy.project().unwrap().id, w.project().unwrap().id);
    let chapter = w.active_chapter().unwrap().id.clone();
    let entries = std::fs::read_dir(root.join("history").join(&chapter)).unwrap();
    let mut count = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_file() {
            assert_eq!(
                std::fs::read(&path).unwrap(),
                std::fs::read(
                    target
                        .join("history")
                        .join(&chapter)
                        .join(path.file_name().unwrap())
                )
                .unwrap()
            );
            count += 1;
        }
    }
    assert!(count > 0);
}

#[test]
fn export_commands_select_scope_and_format_and_restore_focus_on_cancel() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    fn enter(w: &mut Workspace, text: &str) {
        w.handle_event(Event::Paste(text.into())).unwrap();
        w.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )))
        .unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "夜航").unwrap();
    w.execute_command("chapters").unwrap();
    w.execute_command("export-book-txt").unwrap();
    assert!(w.prompt().unwrap().title.contains("作者"));
    enter(&mut w, "");
    assert!(w.prompt().unwrap().title.contains("TXT"));
    let target = dir.path().join("ui.txt");
    enter(&mut w, target.to_str().unwrap());
    settle(&mut w).unwrap();
    assert!(w.module_focused(novelite_tui::ModuleId::Chapters));
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .starts_with("《夜航》\n作者：佚名")
    );
    w.execute_command("export-chapter-md").unwrap();
    w.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    assert!(w.module_focused(novelite_tui::ModuleId::Chapters));
    w.execute_command("edit-outline").unwrap();
    w.dispatch(Action::Insert("未提交大纲".into())).unwrap();
    assert!(w.execute_command("export-chapter-txt").is_err());
    w.dispatch(Action::Save).unwrap();
    assert_eq!(w.chapter_plan().unwrap().outline, "未提交大纲");
}

#[test]
fn export_rejects_occupied_source_and_nested_paths_without_losing_the_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("unsaved.md");
    let mut w = Workspace::open(&fresh).unwrap();
    w.dispatch(Action::Insert("草稿".into())).unwrap();
    assert!(
        w.export_manuscript(ExportFormat::ChapterMarkdown, &fresh)
            .is_err()
    );
    assert!(!fresh.exists());
    let occupied = dir.path().join("occupied.md");
    std::fs::write(&occupied, "原文件").unwrap();
    assert!(
        w.export_manuscript(ExportFormat::ChapterMarkdown, &occupied)
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(occupied).unwrap(), "原文件");
    assert!(
        w.export_manuscript(ExportFormat::ChapterMarkdown, dir.path().join("CON.txt"))
            .is_err()
    );
    assert_eq!(w.text(), "草稿");
    let mut book = Workspace::create_project(dir.path().join("book"), "书").unwrap();
    assert!(
        book.export_manuscript(
            ExportFormat::ChapterMarkdown,
            dir.path().join("book/extra.md")
        )
        .is_err()
    );
    assert!(
        book.export_manuscript(
            ExportFormat::ProjectDirectory,
            dir.path().join("book/nested")
        )
        .is_err()
    );
}
#[test]
fn export_module_shutdown_and_quit_cannot_abandon_a_confirmed_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "书").unwrap();
    let target = dir.path().join("copy.md");
    w.export_manuscript(ExportFormat::ChapterMarkdown, &target)
        .unwrap();
    assert!(w.dispatch(Action::Quit).is_err());
    assert!(
        w.set_module_enabled(novelite_tui::ModuleId::Export, false)
            .is_err()
    );
    assert!(
        w.new_project(dir.path().join("unwanted"), "不能切换")
            .is_err()
    );
    assert!(!dir.path().join("unwanted").exists());
    settle(&mut w).unwrap();
    w.execute_command("chapters").unwrap();
    w.execute_command("export-chapter-md").unwrap();
    w.set_module_enabled(novelite_tui::ModuleId::Export, false)
        .unwrap();
    assert!(w.prompt().is_none());
    assert!(w.module_focused(novelite_tui::ModuleId::Chapters));
    assert!(
        w.export_manuscript(
            ExportFormat::ChapterMarkdown,
            dir.path().join("disabled.md")
        )
        .is_err()
    );
}

#[cfg(windows)]
#[test]
fn denied_writes_report_failure_and_directory_junctions_cannot_escape_the_source() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "书").unwrap();
    w.dispatch(Action::Insert("保留正文".into())).unwrap();
    let parent = dir.path().join("denied");
    std::fs::create_dir(&parent).unwrap();
    let account = String::from_utf8(Command::new("whoami").output().unwrap().stdout).unwrap();
    assert!(
        Command::new("icacls")
            .arg(&parent)
            .args(["/deny", &format!("{}:(OI)(CI)(WD)", account.trim())])
            .output()
            .unwrap()
            .status
            .success()
    );
    let target = parent.join("file.md");
    let result = export(&mut w, ExportFormat::ChapterMarkdown, &target);
    let failure = w.export_message().unwrap().to_owned();
    let folder = parent.join("copy");
    let folder_result = export(&mut w, ExportFormat::ProjectDirectory, &folder);
    let cleaned = !folder.exists();
    assert!(
        Command::new("icacls")
            .arg(&parent)
            .args(["/remove:d", account.trim()])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(result.is_err());
    assert!(folder_result.is_err());
    assert!(cleaned);
    assert!(!target.exists());
    assert!(failure.contains("导出失败"));
    assert_eq!(w.text(), "保留正文");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("keep.txt"), "外部数据").unwrap();
    let link = dir.path().join("book").join("linked");
    assert!(
        Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap()
            .status
            .success()
    );
    let target = dir.path().join("unsafe-copy");
    assert!(export(&mut w, ExportFormat::ProjectDirectory, &target).is_err());
    assert!(!target.exists());
    assert_eq!(
        std::fs::read_to_string(outside.join("keep.txt")).unwrap(),
        "外部数据"
    );
    std::fs::remove_dir(link).unwrap();
}
#[test]
fn book_export_uses_both_panes_and_does_not_include_recycled_chapters() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "双窗").unwrap();
    let first = w.active_chapter().unwrap().id.clone();
    let volume = w.project().unwrap().volumes[0].id.clone();
    w.dispatch(Action::Insert("甲稿".into())).unwrap();
    let second = w.create_chapter(&volume, "第二章").unwrap();
    w.execute_command("split-right").unwrap();
    w.switch_chapter(&second).unwrap();
    w.dispatch(Action::Insert("乙稿".into())).unwrap();
    w.focus_other_pane().unwrap();
    w.dispatch(Action::Insert("甲新".into())).unwrap();
    w.focus_other_pane().unwrap();
    w.dispatch(Action::Insert("乙新".into())).unwrap();
    let target = dir.path().join("panes.txt");
    export(
        &mut w,
        ExportFormat::BookText {
            author: "作者".into(),
        },
        &target,
    )
    .unwrap();
    let text = std::fs::read_to_string(target).unwrap();
    assert!(text.contains("第一章\n\n甲稿甲新"));
    assert!(text.contains("第二章\n\n乙稿乙新"));
    assert!(text.find("甲稿甲新").unwrap() < text.find("乙稿乙新").unwrap());
    w.recycle_chapter(&first).unwrap();
    let target = dir.path().join("live-only.txt");
    export(
        &mut w,
        ExportFormat::BookText {
            author: String::new(),
        },
        &target,
    )
    .unwrap();
    assert!(!std::fs::read_to_string(target).unwrap().contains("甲稿"));
}

#[test]
fn bom_heading_and_nested_export_cancel_match_desktop_and_restore_the_original_panel() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "书").unwrap();
    w.dispatch(Action::Insert("\u{feff}# 标题\n正文".into()))
        .unwrap();
    let path = dir.path().join("bom.txt");
    export(&mut w, ExportFormat::ChapterText, &path).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "标题\n\n正文");
    w.execute_command("chapters").unwrap();
    w.execute_command("history").unwrap();
    w.execute_command("export-chapter-md").unwrap();
    for _ in 0..2 {
        w.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
            .unwrap();
    }
    assert!(w.module_focused(novelite_tui::ModuleId::Chapters));
}
#[test]
fn completed_export_notice_does_not_hide_later_clipboard_failures() {
    struct Denied;
    impl novelite_tui::Clipboard for Denied {
        fn read_text(&mut self) -> std::io::Result<String> {
            Err(std::io::Error::other("拒绝读取"))
        }
        fn write_text(&mut self, _: &str) -> std::io::Result<()> {
            Err(std::io::Error::other("拒绝写入"))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    w.set_clipboard(Box::new(Denied)).unwrap();
    export(
        &mut w,
        ExportFormat::ChapterMarkdown,
        &dir.path().join("export.md"),
    )
    .unwrap();
    w.execute_command("paste").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while w.clipboard_busy() {
        w.tick(Instant::now()).unwrap();
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    let display: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(display.replace(' ', "").contains("系统剪贴板读取失败"));
    assert!(w.export_message().is_none());
    assert!(!display.contains("已导出"));
}

#[test]
fn directory_export_during_saves_never_replays_a_later_transaction_in_the_copy() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut w = Workspace::create_project(&root, "并发保存").unwrap();
    // A real ancillary payload gives the worker and the save transaction overlapping work.
    std::fs::write(root.join("attachment.bin"), vec![0x5a; 16 * 1024 * 1024]).unwrap();
    w.dispatch(Action::Insert("确认稿".into())).unwrap();
    for attempt in 0..4 {
        let expected = w.text().to_owned();
        let target = dir.path().join(format!("copy-{attempt}"));
        w.export_manuscript(ExportFormat::ProjectDirectory, &target)
            .unwrap();
        for _ in 0..4 {
            w.dispatch(Action::Insert("后续稿".into())).unwrap();
            w.dispatch(Action::Save).unwrap();
        }
        match settle(&mut w) {
            Ok(()) => {
                assert!(!target.join(".novelite-journal.json").exists());
                let copy = Workspace::open_project(&target).unwrap();
                assert_eq!(copy.text(), expected);
            }
            Err(_) => {
                // Capturing an active source transaction may refuse the export, never succeed partially.
                assert!(!target.exists());
                assert!(w.export_message().unwrap().contains("导出失败"));
            }
        }
    }
    let target = dir.path().join("settled-copy");
    export(&mut w, ExportFormat::ProjectDirectory, &target).unwrap();
    let copy = Workspace::open_project(&target).unwrap();
    assert_eq!(copy.text(), w.text());
}

#[test]
fn occupied_export_target_keeps_the_failure_visible_after_key_release() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("occupied.md");
    std::fs::write(&target, "原文件").unwrap();
    let mut w = Workspace::open(dir.path().join("draft.md")).unwrap();
    w.execute_command("export-chapter-md").unwrap();
    w.handle_event(Event::Paste(target.to_str().unwrap().into()))
        .unwrap();
    assert!(
        w.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    w.handle_event(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Release,
    )))
    .unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(110, 30)).unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    let display: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(display.replace(' ', "").contains("导出失败"));
    assert!(w.export_message().unwrap().contains("目标必须"));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "原文件");
}
