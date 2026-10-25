use novelite_tui::{Action, Workspace};
use std::time::{Duration, Instant};

fn complete(work: &mut Workspace) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        work.tick(Instant::now()).unwrap();
        if !work.book_search().unwrap().running {
            return;
        }
        assert!(Instant::now() < deadline, "search did not complete");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn book_search_reads_unloaded_chapters_and_opens_body_matches_in_the_active_pane() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut writer = Workspace::create_project(&root, "全书").unwrap();
    writer.dispatch(Action::Insert("开头星河".into())).unwrap();
    let volume = writer.project().unwrap().volumes[0].id.clone();
    let second = writer.create_chapter(&volume, "星河之夜").unwrap();
    writer.switch_chapter(&second).unwrap();
    writer
        .dispatch(Action::Insert("远处有星河，星河照耀。".into()))
        .unwrap();
    writer.dispatch(Action::Quit).unwrap();
    drop(writer);
    let mut work = Workspace::open_project(&root).unwrap();
    work.execute_command("split-right").unwrap();
    work.start_book_search("星河").unwrap();
    complete(&mut work);
    let results = work.book_search().unwrap();
    assert_eq!(results.total, 4);
    assert_eq!(results.chapters.len(), 2);
    assert_eq!(results.chapters[1].count, 3);
    assert!(results.entries[2].context.contains("远处有【星河】"));
    assert!(!results.entries[2].revision.is_empty());
    work.open_book_match(2).unwrap();
    assert_eq!(work.active_chapter().unwrap().id, second);
    assert_eq!(work.selected_text(), "星河");
    assert_eq!(work.cursor(), "远处有星河".len());
    assert_eq!(work.active_pane(), 0);
    work.focus_other_pane().unwrap();
    assert_eq!(work.text(), "开头星河");
    work.execute_command("close-book-search").unwrap();
    assert!(work.book_search().is_none());
}

struct GatedReader {
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    first: std::sync::atomic::AtomicBool,
}

#[test]
fn disabled_search_discards_late_results_after_reenable_and_layout_changes() {
    use novelite_tui::ModuleId;
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "模块任务").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.create_chapter(&volume, "后台章").unwrap();
    let (entered, blocked) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    work.set_search_reader(GatedReader {
        entered,
        release: std::sync::Mutex::new(resume),
        first: true.into(),
    });
    work.start_book_search("旧查询").unwrap();
    blocked.recv_timeout(Duration::from_secs(3)).unwrap();
    work.set_module_enabled(ModuleId::Search, false).unwrap();
    work.execute_command("pin-search").unwrap();
    work.set_module_enabled(ModuleId::Search, true).unwrap();
    work.handle_event(crossterm::event::Event::Paste("新查询".into()))
        .unwrap();
    work.dispatch(Action::Save).unwrap();
    work.start_book_search("新查询").unwrap();
    work.execute_command("focus-editor").unwrap();
    work.workspace_layout(ratatui::layout::Rect::new(0, 1, 50, 20));
    release.send(()).unwrap();
    complete(&mut work);
    assert_eq!(work.book_search().unwrap().query, "新查询");
    assert_eq!(work.book_search().unwrap().total, 1);
    assert!(!work.book_search_focused());
    work.set_module_enabled(ModuleId::Search, false).unwrap();
    work.poll_book_search();
    assert!(work.book_search().is_none());
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "新查询");
}
impl novelite_tui::SearchReader for GatedReader {
    fn read(&self, root: &std::path::Path, relative: &str) -> std::io::Result<String> {
        if self.first.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.entered.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(8))
                .unwrap();
        }
        std::fs::read_to_string(root.join(relative))
    }
}

#[test]
fn latest_query_wins_over_delayed_reads_while_editing_and_saving_remain_available() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "交错").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "后台章").unwrap();
    let chapter = &work.project().unwrap().volumes[0].chapters[1];
    let path = dir.path().join("book").join(&chapter.path);
    std::fs::write(&path, "旧查询 新查询").unwrap();
    let (entered, blocked) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    work.set_search_reader(GatedReader {
        entered,
        release: std::sync::Mutex::new(resume),
        first: true.into(),
    });
    work.start_book_search("旧查询").unwrap();
    blocked.recv_timeout(Duration::from_secs(3)).unwrap();
    let first = work.book_search().unwrap().generation;
    work.execute_command("focus-book-search").unwrap();
    work.handle_event(crossterm::event::Event::Paste("本地新查询".into()))
        .unwrap();
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "本地新查询");
    work.start_book_search("新查询").unwrap();
    assert!(work.book_search().unwrap().generation > first);
    release.send(()).unwrap();
    complete(&mut work);
    assert_eq!(work.book_search().unwrap().query, "新查询");
    assert_eq!(work.book_search().unwrap().total, 2);
    assert_eq!(work.book_search().unwrap().entries[1].chapter, second);
    work.execute_command("close-book-search").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while work.search_background_busy() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    work.poll_book_search();
    assert!(work.book_search().is_none());
}

#[test]
fn book_search_prompt_and_panel_return_focus_and_cancel_pending_work_on_close() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "面板").unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.create_chapter(&volume, "后台章").unwrap();
    let (entered, blocked) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    work.set_search_reader(GatedReader {
        entered,
        release: std::sync::Mutex::new(resume),
        first: true.into(),
    });
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Paste("第一章".into())).unwrap();
    work.execute_command("search-book").unwrap();
    work.handle_event(Event::Paste("查询".into())).unwrap();
    work.handle_event(key(KeyCode::Enter)).unwrap();
    blocked.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(work.navigation().is_none());
    assert!(work.book_search().unwrap().focused);
    work.handle_event(key(KeyCode::Esc)).unwrap();
    assert!(work.book_search().is_none());
    assert_eq!(work.navigation().unwrap().query, "第一章");
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while work.search_background_busy() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    work.poll_book_search();
    assert!(work.book_search().is_none());
    work.handle_event(key(KeyCode::Esc)).unwrap();
    work.handle_event(Event::Paste("继续写作".into())).unwrap();
    assert_eq!(work.text(), "继续写作");
}

#[test]
fn stale_body_results_are_refreshed_before_jumping_and_copying_a_book_discards_old_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "修订").unwrap();
    work.dispatch(Action::Insert("猫在窗前".into())).unwrap();
    work.start_book_search("猫").unwrap();
    complete(&mut work);
    work.dispatch(Action::Move(novelite_tui::Direction::Home))
        .unwrap();
    work.dispatch(Action::Insert("很长的序言".into())).unwrap();
    assert!(work.open_book_match(0).is_err());
    assert_eq!(work.cursor(), "很长的序言".len());
    complete(&mut work);
    work.open_book_match(0).unwrap();
    assert_eq!(work.selected_text(), "猫");
    assert_eq!(work.cursor(), "很长的序言猫".len());
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "下一章").unwrap();
    let path = dir
        .path()
        .join("book")
        .join(&work.project().unwrap().volumes[0].chapters[1].path);
    std::fs::write(&path, "猫在山上").unwrap();
    work.start_book_search("猫").unwrap();
    complete(&mut work);
    std::fs::write(&path, "山上只有狗").unwrap();
    assert!(work.open_book_match(1).is_err());
    assert_eq!(work.active_chapter().unwrap().id, second);
    assert_eq!(work.text(), "山上只有狗");
    assert_eq!(work.selected_text(), "");
    complete(&mut work);
    assert_eq!(work.book_search().unwrap().total, 1);
    work.save_as(dir.path().join("copy")).unwrap();
    assert!(work.book_search().is_none());
}

#[test]
fn a_million_character_book_search_allows_visible_edits_saves_and_project_switches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("long");
    let mut writer = Workspace::create_project(&root, "百万字").unwrap();
    let volume = writer.project().unwrap().volumes[0].id.clone();
    for i in 2..=10 {
        writer.create_chapter(&volume, &format!("第{i}章")).unwrap();
    }
    let paths: Vec<_> = writer.project().unwrap().volumes[0]
        .chapters
        .iter()
        .map(|c| root.join(&c.path))
        .collect();
    drop(writer);
    let text = format!("{}目标", "山".repeat(99_998));
    for path in paths {
        std::fs::write(path, &text).unwrap();
    }
    let mut work = Workspace::open_project(&root).unwrap();
    work.view(80, 24);
    let start = Instant::now();
    work.start_book_search("目标").unwrap();
    work.execute_command("focus-book-search").unwrap();
    let mut max_edit = Duration::ZERO;
    for _ in 0..12 {
        let edit = Instant::now();
        work.handle_event(crossterm::event::Event::Paste("续".into()))
            .unwrap();
        assert!(work.view(80, 24).lines[0].starts_with("续"));
        max_edit = max_edit.max(edit.elapsed());
    }
    work.dispatch(Action::Save).unwrap();
    assert_eq!(
        std::fs::read_to_string(work.path()).unwrap(),
        format!("{}{}", "续".repeat(12), text)
    );
    complete(&mut work);
    assert_eq!(work.book_search().unwrap().total, 10);
    eprintln!(
        "MILLION_SEARCH elapsed_ms={} max_edit_and_view_ms={}",
        start.elapsed().as_millis(),
        max_edit.as_millis()
    );
    work.start_book_search("山").unwrap();
    work.execute_command("close-book-search").unwrap();
    assert!(work.book_search().is_none());
    let (entered, blocked) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    work.set_search_reader(GatedReader {
        entered,
        release: std::sync::Mutex::new(resume),
        first: true.into(),
    });
    work.start_book_search("目标").unwrap();
    blocked.recv_timeout(Duration::from_secs(5)).unwrap();
    work.new_project(dir.path().join("other"), "新作品")
        .unwrap();
    release.send(()).unwrap();
    work.poll_book_search();
    assert!(work.book_search().is_none());
    assert_eq!(work.project().unwrap().title, "新作品");
    work.dispatch(Action::Insert("新作品内容".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "新作品内容");
}

#[test]
fn chapter_find_temporarily_yields_book_results_focus_and_restores_it_on_close() {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "焦点嵌套").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    for _ in 0..2 {
        work.execute_command("find").unwrap();
        work.handle_event(Event::Paste("正文".into())).unwrap();
        work.handle_event(key(KeyCode::Enter)).unwrap();
        assert!(!work.book_search_focused());
        if work.book_search().is_none() {
            work.start_book_search("正文").unwrap();
            complete(&mut work);
        } else {
            work.handle_event(key(KeyCode::Esc)).unwrap();
            assert!(work.book_search_focused());
        }
    }
    work.execute_command("chapters").unwrap();
    work.execute_command("focus-book-search").unwrap();
    assert!(work.navigation().is_none());
    assert!(work.book_search_focused());
    assert!(work.execute_command("cut-internal").is_err());
    work.handle_event(Event::Paste("不得写入".into())).unwrap();
    assert_eq!(work.text(), "正文");
}

#[test]
fn idle_search_completion_signals_a_terminal_redraw_without_an_input_event() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "静置刷新").unwrap();
    work.dispatch(Action::Insert("灯塔".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    work.start_book_search("灯塔").unwrap();
    let before = work.book_search_generation();
    complete(&mut work);
    assert!(work.book_search_generation() > before);
    assert_eq!(work.book_search().unwrap().total, 1);
}
