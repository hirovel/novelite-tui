use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, ChapterPlan, Direction, ExportFormat, ImportEncoding, Workspace};
use std::time::{Duration, Instant};

fn settle(work: &mut Workspace) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        work.tick(Instant::now()).unwrap();
        if !work.import_busy()
            && !work.export_busy()
            && !work.book_search().is_some_and(|s| s.running)
        {
            return;
        }
        assert!(Instant::now() < deadline, "background work did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn append(work: &mut Workspace, text: &str) {
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Insert(text.into())).unwrap();
}

#[test]
fn imported_novel_survives_planning_dual_writing_format_search_restore_export_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("夜航.md");
    let original = "# 第一章 来信\n\n  雨夜来信。\n\n# 第二章 灯塔\n\n  他在等谁？\n";
    let first_original = "# 第一章 来信\n\n  雨夜来信。\n\n";
    let first_final = "# 第一章 来信\n\n　　雨夜来信。\n\n　　他收好信。\n";
    let second_final = "# 第二章 灯塔\n\n  他在等谁？\n　　灯亮了。\n";
    std::fs::write(&source, original).unwrap();
    let mut work = Workspace::open(dir.path().join("scratch.md")).unwrap();
    work.prepare_import(&source, ImportEncoding::Utf8).unwrap();
    settle(&mut work);
    assert_eq!(work.import_preview().unwrap().chapters.len(), 2);
    let root = dir.path().join("book");
    work.confirm_import(&root).unwrap();
    settle(&mut work);
    let first = work.active_chapter().unwrap().id.clone();
    let second = work.project().unwrap().volumes[0].chapters[1].id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.rename_volume(&volume, "第一卷").unwrap();
    assert_eq!(work.text(), first_original);
    work.update_chapter_plan(ChapterPlan {
        outline: "来信留下灯塔的线索".into(),
        target_words: Some(2000),
    })
    .unwrap();
    work.set_book_goal(Some(100_000)).unwrap();
    work.mark_snapshot().unwrap();
    let snapshot = work.history_entries().unwrap()[0].id.clone();
    work.execute_command("split-right").unwrap();
    work.switch_chapter(&second).unwrap();
    append(&mut work, "　　灯亮了。\n");
    work.focus_other_pane().unwrap();
    assert_eq!(work.active_chapter().unwrap().id, first);
    append(&mut work, "　　他收好信。\n");
    work.execute_command("format-paragraphs").unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(work.text(), first_final);
    work.start_book_search("灯亮了").unwrap();
    settle(&mut work);
    assert_eq!(work.book_search().unwrap().total, 1);
    work.open_book_match(0).unwrap();
    assert_eq!(work.selected_text(), "灯亮了");
    assert_eq!(work.text(), second_final);
    work.execute_command("close-book-search").unwrap();
    work.switch_chapter(&first).unwrap();
    work.restore_snapshot(&snapshot).unwrap();
    assert_eq!(work.text(), first_original);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), first_final);
    work.dispatch(Action::Save).unwrap();
    let chapter = dir.path().join("chapter.md");
    work.export_manuscript(ExportFormat::ChapterMarkdown, &chapter)
        .unwrap();
    settle(&mut work);
    assert_eq!(std::fs::read_to_string(chapter).unwrap(), first_final);
    let book_text = dir.path().join("book.txt");
    work.export_manuscript(
        ExportFormat::BookText {
            author: "测试作者".into(),
        },
        &book_text,
    )
    .unwrap();
    settle(&mut work);
    assert_eq!(
        std::fs::read_to_string(book_text).unwrap(),
        "《夜航》\n作者：测试作者\n\n====================================\n\n\n【第一卷】\n\n\n第一章 来信\n\n　　雨夜来信。\n\n　　他收好信。\n\n\n\n第二章 灯塔\n\n  他在等谁？\n　　灯亮了。\n\n\n"
    );
    let copy = dir.path().join("portable");
    work.export_manuscript(ExportFormat::ProjectDirectory, &copy)
        .unwrap();
    settle(&mut work);
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    for folder in [&root, &copy] {
        let mut reopened = Workspace::open_project(folder).unwrap();
        reopened.switch_chapter(&first).unwrap();
        assert_eq!(reopened.text(), first_final);
        assert_eq!(
            reopened.chapter_plan().unwrap().outline,
            "来信留下灯塔的线索"
        );
        assert_eq!(reopened.project().unwrap().target_words, Some(100_000));
        assert_eq!(
            reopened.preview_snapshot(&snapshot).unwrap(),
            first_original
        );
        reopened.switch_chapter(&second).unwrap();
        assert_eq!(reopened.text(), second_final);
    }
    assert_eq!(std::fs::read_to_string(source).unwrap(), original);
}
