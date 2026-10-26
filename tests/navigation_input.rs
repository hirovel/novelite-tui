use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};

fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}

#[test]
fn chapter_filter_edits_at_its_grapheme_cursor_without_editing_the_manuscript() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.dispatch(Action::Insert("保留正文".into())).unwrap();
    w.execute_command("chapters").unwrap();
    w.handle_event(Event::Paste("中👩‍💻e\u{301}末".into()))
        .unwrap();
    key(&mut w, KeyCode::Left);
    w.handle_event(Event::Paste("间\n".into())).unwrap();
    assert_eq!(w.navigation().unwrap().query, "中👩‍💻e\u{301}间末");
    key(&mut w, KeyCode::Home);
    key(&mut w, KeyCode::Delete);
    assert_eq!(w.navigation().unwrap().query, "👩‍💻e\u{301}间末");
    key(&mut w, KeyCode::Right);
    key(&mut w, KeyCode::Delete);
    assert_eq!(w.navigation().unwrap().query, "👩‍💻间末");
    key(&mut w, KeyCode::Backspace);
    assert_eq!(w.navigation().unwrap().query, "间末");
    key(&mut w, KeyCode::End);
    key(&mut w, KeyCode::Backspace);
    assert_eq!(w.navigation().unwrap().query, "间");
    assert_eq!(w.text(), "保留正文");
    key(&mut w, KeyCode::Enter);
    assert!(
        w.navigation().is_some(),
        "empty results must not activate a target"
    );
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), "保留正文");
}
#[test]
fn navigation_pages_by_its_visible_rows_and_keeps_selection_on_caret_moves() {
    use novelite_tui::ModuleId;
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    let volume = w.project().unwrap().volumes[0].id.clone();
    for i in 0..18 {
        w.create_chapter(&volume, &format!("章节{i:02}")).unwrap();
    }
    w.execute_command("chapters").unwrap();
    w.handle_event(Event::Paste("章".into())).unwrap();
    let page = w.module_panel(ModuleId::Chapters, 8).entries.len();
    assert_eq!(page, 7);
    key(&mut w, KeyCode::PageDown);
    assert_eq!(w.navigation().unwrap().selected, 7);
    w.module_panel(ModuleId::Chapters, 8);
    key(&mut w, KeyCode::PageDown);
    assert_eq!(w.navigation().unwrap().selected, 14);
    let first = w.module_panel(ModuleId::Chapters, 8).first;
    key(&mut w, KeyCode::Up);
    assert_eq!(w.module_panel(ModuleId::Chapters, 8).first, first);
    key(&mut w, KeyCode::Left);
    key(&mut w, KeyCode::Backspace);
    assert_eq!(w.navigation().unwrap().selected, 13);
    assert_eq!(w.navigation().unwrap().query, "章");
    key(&mut w, KeyCode::Delete);
    assert_eq!(w.navigation().unwrap().selected, 0);
    assert_eq!(w.navigation().unwrap().query, "");
    assert_eq!(w.module_panel(ModuleId::Chapters, 8).first, 0);
    key(&mut w, KeyCode::PageUp);
    assert_eq!(w.navigation().unwrap().selected, 0);
}
#[test]
fn confirmation_lists_keep_cancel_and_the_full_scope_when_text_is_pasted() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    let chapter = w.active_chapter().unwrap().id.clone();
    w.dispatch(Action::Insert("原稿".into())).unwrap();
    w.execute_command("recycle-chapter").unwrap();
    w.handle_event(Event::Paste("移入回收区".into())).unwrap();
    let view = w.navigation().unwrap();
    assert_eq!(view.query, "");
    assert_eq!(view.entries[0].label, "取消");
    assert!(view.entries[1].label.contains("1 章"));
    key(&mut w, KeyCode::Enter);
    assert_eq!(w.active_chapter().unwrap().id, chapter);
    assert!(w.project().unwrap().recycle_bin.is_empty());
    w.dispatch(Action::Save).unwrap();
    w.dispatch(Action::Insert("本地".into())).unwrap();
    std::fs::write(w.path(), "磁盘").unwrap();
    w.execute_command("conflicts").unwrap();
    w.handle_event(Event::Paste("明确覆盖".into())).unwrap();
    assert_eq!(w.navigation().unwrap().query, "");
    assert!(w.navigation().unwrap().entries[0].label.contains("取消"));
    assert_eq!(w.navigation().unwrap().entries.len(), 4);
    key(&mut w, KeyCode::Enter);
    assert!(w.external_conflict().is_some());
    assert_eq!(w.text(), "原稿本地");
    assert_eq!(std::fs::read_to_string(w.path()).unwrap(), "磁盘");
}
#[test]
fn navigation_query_shows_the_whole_grapheme_under_its_caret() {
    use ratatui::{Terminal, backend::TestBackend};
    fn draw(w: &mut Workspace, width: u16) -> (String, ratatui::layout::Position) {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal.draw(|f| w.render(f, "")).unwrap();
        let buffer = terminal.backend().buffer();
        let lines = (0..24)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        (lines, terminal.get_cursor_position().unwrap())
    }
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.execute_command("history").unwrap();
    let query = format!("头{}尾", "中".repeat(40));
    w.handle_event(Event::Paste(query.clone())).unwrap();
    let (screen, end) = draw(&mut w, 32);
    assert!(
        screen.contains('尾'),
        "the end of a long query must be visible"
    );
    key(&mut w, KeyCode::Home);
    let (screen, home) = draw(&mut w, 32);
    assert!(screen.contains('头'));
    key(&mut w, KeyCode::Right);
    let (_, after_head) = draw(&mut w, 32);
    assert_eq!(after_head.x, home.x + 2);
    assert_eq!(after_head.y, home.y);
    assert!(end.x < 32);
    let (_, expanded) = draw(&mut w, 100);
    assert!(expanded.x < 100);
    key(&mut w, KeyCode::Char('界'));
    assert_eq!(
        w.navigation().unwrap().query,
        format!("头界{}尾", "中".repeat(40))
    );
    key(&mut w, KeyCode::Backspace);
    key(&mut w, KeyCode::End);
    let (screen, end) = draw(&mut w, 32);
    assert!(screen.contains('尾'));
    assert!(end.x < 32);
    assert_eq!(w.navigation().unwrap().query, query);
}
#[test]
fn focused_module_panels_render_the_same_editable_query_cursor() {
    use ratatui::{Terminal, backend::TestBackend};
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    for command in ["chapters", "planning"] {
        w.execute_command(command).unwrap();
        w.handle_event(Event::Paste(format!("头{}尾", "中".repeat(40))))
            .unwrap();
        let mut terminal = Terminal::new(TestBackend::new(80, 32)).unwrap();
        terminal.draw(|f| w.render(f, "")).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|c| c.symbol() == "尾"),
            "{command} must show the input around its caret"
        );
        key(&mut w, KeyCode::Home);
        terminal.draw(|f| w.render(f, "")).unwrap();
        let home = terminal.get_cursor_position().unwrap();
        assert_eq!(terminal.backend().buffer()[(home.x, home.y)].symbol(), "头");
        key(&mut w, KeyCode::Right);
        terminal.draw(|f| w.render(f, "")).unwrap();
        let moved = terminal.get_cursor_position().unwrap();
        assert_eq!(moved.x, home.x + 2);
        assert_eq!(moved.y, home.y);
        key(&mut w, KeyCode::Esc);
    }
}
#[test]
fn all_filterable_navigation_flows_share_insertion_deletion_and_empty_activation() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.dispatch(Action::Insert("正文".into())).unwrap();
    w.mark_snapshot().unwrap();
    for command in [
        "chapters",
        "recent-projects",
        "move-chapter",
        "copy-chapter",
        "recycle-bin",
        "history",
        "planning",
    ] {
        w.execute_command(command).unwrap();
        w.handle_event(Event::Paste("无👩‍💻e\u{301}匹配".into()))
            .unwrap();
        key(&mut w, KeyCode::Home);
        key(&mut w, KeyCode::Delete);
        key(&mut w, KeyCode::Right);
        key(&mut w, KeyCode::Delete);
        w.handle_event(Event::Paste("中\t文\n".into())).unwrap();
        assert_eq!(w.navigation().unwrap().query, "👩‍💻中文匹配", "{command}");
        assert!(w.navigation().unwrap().entries.is_empty(), "{command}");
        key(&mut w, KeyCode::Enter);
        assert!(w.navigation().is_some(), "{command}");
        assert_eq!(w.text(), "正文", "{command}");
        key(&mut w, KeyCode::Esc);
    }
}
