use novelite_tui::{Action, ChapterPlan, Workspace};

#[test]
fn chapter_plans_and_book_goal_survive_switch_copy_and_reopen_without_changing_body() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "计划").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.dispatch(Action::Insert("只有正文".into())).unwrap();
    work.update_chapter_plan(ChapterPlan {
        outline: "遇见故人\n留下悬念 👩‍💻".into(),
        target_words: Some(3000),
    })
    .unwrap();
    work.set_book_goal(Some(100000)).unwrap();
    let second = work.create_chapter(&volume, "第二章").unwrap();
    work.switch_chapter(&second).unwrap();
    assert_eq!(work.chapter_plan().unwrap(), &ChapterPlan::default());
    work.update_chapter_plan(ChapterPlan {
        outline: "重新出发".into(),
        target_words: None,
    })
    .unwrap();
    let copy = work.copy_chapter(&first, &volume, "副本").unwrap();
    work.switch_chapter(&copy).unwrap();
    assert_eq!(
        work.chapter_plan().unwrap().outline,
        "遇见故人\n留下悬念 👩‍💻"
    );
    work.switch_chapter(&first).unwrap();
    work.dispatch(Action::Quit).unwrap();
    let body = work.path().to_path_buf();
    drop(work);
    assert_eq!(std::fs::read_to_string(body).unwrap(), "只有正文");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("novelite.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["volumes"][0]["chapters"][0]["plan"]["outline"],
        "遇见故人\n留下悬念 👩‍💻"
    );
    let reopened = Workspace::open_project(root).unwrap();
    assert_eq!(reopened.chapter_plan().unwrap().target_words, Some(3000));
    assert_eq!(reopened.project().unwrap().target_words, Some(100000));
}

#[test]
fn counts_match_desktop_samples_and_update_across_word_joins_undo_and_replacement() {
    let dir = tempfile::tempdir().unwrap();
    for (index, (sample, expected)) in [
        ("中文 Hello world 123 abc_def", 6),
        ("𠀀𠮷😀👩‍💻 e\u{301} café", 2),
        ("〇㐀豈汉字", 5),
        ("foo-bar don't 3.14", 6),
        ("A中文B", 4),
        ("甲\u{3000}乙\tABC\r\n__42", 4),
        ("# 标题\n**Hello**", 3),
    ]
    .into_iter()
    .enumerate()
    {
        let mut work = Workspace::open(dir.path().join(format!("{index}.md"))).unwrap();
        work.dispatch(Action::Insert(sample.into())).unwrap();
        assert_eq!(work.word_progress().chapter_words, expected, "{sample}");
        work.dispatch(Action::Undo).unwrap();
        assert_eq!(work.word_progress().chapter_words, 0);
        work.dispatch(Action::Redo).unwrap();
        assert_eq!(work.word_progress().chapter_words, expected);
    }
    let mut work = Workspace::open(dir.path().join("join.md")).unwrap();
    work.dispatch(Action::Insert("ab cd".into())).unwrap();
    for _ in 0..2 {
        work.dispatch(Action::Move(novelite_tui::Direction::Left))
            .unwrap();
    }
    work.dispatch(Action::Backspace).unwrap();
    assert_eq!(work.text(), "abcd");
    assert_eq!(work.word_progress().chapter_words, 1);
    work.dispatch(Action::Insert("中文".into())).unwrap();
    assert_eq!(work.word_progress().chapter_words, 4);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.word_progress().chapter_words, 1);
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("另一份 文稿".into())).unwrap();
    assert_eq!(work.word_progress().chapter_words, 5);
}

#[test]
fn book_counts_ignore_late_results_and_scan_again_only_when_requested() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    struct Reader {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        calls: Arc<AtomicUsize>,
    }
    impl novelite_tui::WordCountReader for Reader {
        fn read(&self, root: &std::path::Path, relative: &str) -> std::io::Result<String> {
            let text = std::fs::read_to_string(root.join(relative))?;
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.entered.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(8))
                    .unwrap();
            }
            Ok(text)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "统计").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.dispatch(Action::Insert("中文 hello".into())).unwrap();
    let second = work.create_chapter(&volume, "下一章").unwrap();
    work.switch_chapter(&second).unwrap();
    work.dispatch(Action::Insert("你好".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut work = Workspace::open_project(&root).unwrap();
    work.switch_chapter(&first).unwrap();
    let (entered, wait) = mpsc::channel();
    let (release, go) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    work.set_word_count_reader(Arc::new(Reader {
        entered,
        release: Mutex::new(go),
        calls: calls.clone(),
    }));
    work.tick(std::time::Instant::now()).unwrap();
    wait.recv_timeout(std::time::Duration::from_secs(8))
        .unwrap();
    work.switch_chapter(&second).unwrap();
    work.dispatch(Action::Insert("新".into())).unwrap();
    release.send(()).unwrap();
    wait_counts(&mut work);
    assert_eq!(work.word_progress().book_words, Some(6));
    let reads = calls.load(Ordering::SeqCst);
    for _ in 0..20 {
        work.dispatch(Action::Insert("字".into())).unwrap();
        work.tick(std::time::Instant::now()).unwrap();
    }
    assert_eq!(work.word_progress().book_words, Some(26));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        reads,
        "typing must not reread the book"
    );
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.word_progress().book_words, Some(25));
    work.set_module_enabled(novelite_tui::ModuleId::Planning, false)
        .unwrap();
    work.set_module_enabled(novelite_tui::ModuleId::Planning, true)
        .unwrap();
    wait_counts(&mut work);
    assert_eq!(work.word_progress().book_words, Some(25));
}

fn wait_counts(work: &mut Workspace) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        work.tick(std::time::Instant::now()).unwrap();
        if work.word_progress().book_words.is_some() && !work.word_progress().running {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "word count timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn planning_commands_edit_multiline_outline_goals_and_preserve_body() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "设计").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    work.execute_command("planning").unwrap();
    assert!(work.module_focused(novelite_tui::ModuleId::Planning));
    work.execute_command("edit-outline").unwrap();
    work.handle_event(Event::Paste("遇见故人\n留下悬念 👩‍💻".into()))
        .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    work.handle_event(Event::Paste("结尾".into())).unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('y'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE)))
        .unwrap();
    assert_eq!(
        work.chapter_plan().unwrap().outline,
        "遇见故人\n留下悬念 👩‍💻\n结尾"
    );
    assert_eq!(work.text(), "正文");
    work.execute_command("chapter-goal").unwrap();
    work.handle_event(Event::Paste("3000".into())).unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    work.execute_command("book-goal").unwrap();
    work.handle_event(Event::Paste("100000".into())).unwrap();
    work.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(work.word_progress().chapter_goal, Some(3000));
    assert_eq!(work.word_progress().book_goal, Some(100000));
    let panel = work.module_panel(novelite_tui::ModuleId::Planning, 20);
    assert!(panel.headers.iter().any(|s| s.contains("3000")));
    assert!(
        panel.headers.iter().any(|s| s.contains("100000")),
        "the book goal must stay visible after setting it"
    );
    assert!(panel.headers.iter().any(|s| s.contains("遇见故人")));
    work.execute_command("toggle-planning-module").unwrap();
    assert!(work.execute_command("edit-outline").is_err());
    work.execute_command("toggle-planning-module").unwrap();
    assert_eq!(
        work.chapter_plan().unwrap().outline,
        "遇见故人\n留下悬念 👩‍💻\n结尾"
    );
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "正文");
}

#[test]
fn outline_uses_clipboard_selection_mouse_and_rejects_stale_chapter_submission() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "编辑").unwrap();
    work.dispatch(Action::Insert("正文保留".into())).unwrap();
    work.execute_command("edit-outline").unwrap();
    work.handle_event(Event::Paste("abc def".into())).unwrap();
    work.execute_command("select-all").unwrap();
    work.execute_command("copy-internal").unwrap();
    work.handle_event(Event::Paste("替换".into())).unwrap();
    work.execute_command("paste-internal").unwrap();
    work.configure_mouse(true);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    work.handle_event(Event::Paste("开头".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    assert_eq!(work.chapter_plan().unwrap().outline, "开头替换abc def");
    assert_eq!(work.text(), "正文保留");
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "第二章").unwrap();
    work.execute_command("chapter-goal").unwrap();
    work.handle_event(Event::Paste("300".into())).unwrap();
    work.switch_chapter(&second).unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    assert_eq!(work.chapter_plan().unwrap().target_words, None);
    work.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
        .unwrap();
    work.execute_command("chapter-goal").unwrap();
    work.handle_event(Event::Paste("-3".into())).unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    assert!(work.prompt().is_some());
}

#[test]
fn planning_panel_renders_outline_and_click_opens_the_visible_action() {
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "布局").unwrap();
    work.update_chapter_plan(ChapterPlan {
        outline: "情节梗概".into(),
        target_words: Some(800),
    })
    .unwrap();
    work.execute_command("planning").unwrap();
    work.configure_mouse(true);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        screen.replace(' ', "").contains("情节梗概"),
        "planning must not be covered by another navigation layer: {screen}"
    );
    let index = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .position(|cell| cell.symbol() == "编")
        .unwrap();
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (index % 100) as u16,
        row: (index / 100) as u16,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert!(work.prompt().unwrap().title.contains("大纲"));
}

#[test]
fn disabling_statistics_cancels_remaining_reads_then_reenable_recounts() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    struct Reader {
        entered: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
        calls: Arc<AtomicUsize>,
    }
    impl novelite_tui::WordCountReader for Reader {
        fn read(&self, root: &std::path::Path, relative: &str) -> std::io::Result<String> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.entered.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(8))
                    .unwrap();
            }
            std::fs::read_to_string(root.join(relative))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "取消").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.create_chapter(&volume, "二").unwrap();
    work.create_chapter(&volume, "三").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    let (entered, wait) = mpsc::channel();
    let (release, go) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    work.set_word_count_reader(Arc::new(Reader {
        entered,
        release: Mutex::new(go),
        calls: calls.clone(),
    }));
    work.tick(std::time::Instant::now()).unwrap();
    wait.recv_timeout(std::time::Duration::from_secs(8))
        .unwrap();
    work.set_module_enabled(novelite_tui::ModuleId::Planning, false)
        .unwrap();
    release.send(()).unwrap();
    work.set_module_enabled(novelite_tui::ModuleId::Planning, true)
        .unwrap();
    wait_counts(&mut work);
    assert_eq!(work.word_progress().book_words, Some(2));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "cancelled pass stops after the in-flight read"
    );
}

#[test]
fn disabling_planning_keeps_the_outline_draft_and_projects_keep_separate_counts() {
    use crossterm::event::Event;
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    let mut work = Workspace::create_project(&first, "一").unwrap();
    work.dispatch(Action::Insert("甲乙".into())).unwrap();
    work.execute_command("edit-outline").unwrap();
    work.handle_event(Event::Paste("尚未按保存的大纲".into()))
        .unwrap();
    work.set_module_enabled(novelite_tui::ModuleId::Planning, false)
        .unwrap();
    work.set_module_enabled(novelite_tui::ModuleId::Planning, true)
        .unwrap();
    assert_eq!(work.chapter_plan().unwrap().outline, "尚未按保存的大纲");
    work.set_book_goal(Some(400)).unwrap();
    work.new_project(&second, "二").unwrap();
    work.dispatch(Action::Insert("hello".into())).unwrap();
    wait_counts(&mut work);
    assert_eq!(work.word_progress().book_words, Some(1));
    assert_eq!(work.word_progress().book_goal, None);
    assert!(work.chapter_plan().unwrap().outline.is_empty());
    work.switch_project(&first).unwrap();
    wait_counts(&mut work);
    assert_eq!(work.word_progress().book_words, Some(2));
    assert_eq!(work.word_progress().book_goal, Some(400));
    assert_eq!(work.chapter_plan().unwrap().outline, "尚未按保存的大纲");
}

#[test]
fn failed_book_count_is_visible_in_planning_instead_of_staying_in_progress() {
    struct Unreadable;
    impl novelite_tui::WordCountReader for Unreadable {
        fn read(&self, _: &std::path::Path, _: &str) -> std::io::Result<String> {
            Err(std::io::Error::other("unavailable chapter"))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "统计反馈").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.create_chapter(&volume, "另一章").unwrap();
    work.set_word_count_reader(std::sync::Arc::new(Unreadable));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        work.tick(std::time::Instant::now()).unwrap();
        let progress = work.word_progress();
        if !progress.running && !progress.errors.is_empty() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "word count did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    work.execute_command("planning").unwrap();
    let panel = work.module_panel(novelite_tui::ModuleId::Planning, 20);
    let visible = panel.headers.join("\n");
    assert!(visible.contains("统计失败"), "{visible}");
    assert!(visible.contains("刷新字数"), "{visible}");
    assert!(
        !visible.contains("统计中"),
        "completed failure must not appear pending"
    );
    assert!(panel.entries.iter().any(|s| s.contains("刷新")));
}
