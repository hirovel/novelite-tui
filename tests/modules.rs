use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, ModuleId, Workspace};

fn key(work: &mut Workspace, code: KeyCode) {
    work.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}

#[test]
fn exact_command_id_precedes_module_management_substring_matches() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "命令").unwrap();
    key(&mut work, KeyCode::F(1));
    work.handle_event(Event::Paste("chapters".into())).unwrap();
    key(&mut work, KeyCode::Enter);
    assert!(work.module_enabled(ModuleId::Chapters));
    assert!(work.navigation().is_some());
}

#[test]
fn nested_history_search_keeps_new_focus_and_restores_sources_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "嵌套").unwrap();
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Paste("第一".into())).unwrap();
    work.execute_command("history").unwrap();
    work.execute_command("search-book").unwrap();
    work.handle_event(Event::Paste("正文".into())).unwrap();
    key(&mut work, KeyCode::Enter);
    assert!(work.book_search_focused());
    key(&mut work, KeyCode::Esc);
    assert!(work.navigation().unwrap().title.starts_with("历史"));
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.navigation().unwrap().query, "第一");
}

#[test]
fn mouse_targets_the_visible_overlay_above_a_pinned_panel() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "覆盖").unwrap();
    work.configure_mouse(true);
    work.execute_command("pin-search").unwrap();
    work.execute_command("chapters").unwrap();
    work.workspace_layout(ratatui::layout::Rect::new(0, 1, 120, 30));
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 100,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert!(work.prompt().is_none());
    assert!(work.module_focused(ModuleId::Chapters));
}

#[test]
fn a_conflict_copy_cannot_be_overwritten_by_later_layout_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "救援").unwrap();
    let layout = dir.path().join("reserved.md");
    work.load_layout(&layout).unwrap();
    work.dispatch(Action::Insert("本地稿".into())).unwrap();
    std::fs::write(work.path(), "外部稿").unwrap();
    work.check_external().unwrap();
    assert!(work.save_conflict_copy(&layout).is_err());
    let rescue = dir.path().join("rescued.md");
    work.save_conflict_copy(&rescue).unwrap();
    work.execute_command("pin-chapters").unwrap();
    assert_eq!(std::fs::read_to_string(rescue).unwrap(), "本地稿");
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "外部稿");
}

#[test]
fn invalid_or_unwritable_layout_preserves_preferences_and_manuscript() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let config = dir.path().join("settings").join("layout.json");
    let mut work = Workspace::create_project(&root, "安全配置").unwrap();
    let manuscript = work.path().to_path_buf();
    assert!(work.load_layout(manuscript).is_err());
    work.load_layout(&config).unwrap();
    std::fs::write(dir.path().join("settings"), "阻塞目录").unwrap();
    assert!(work.set_module_enabled(ModuleId::Search, false).is_err());
    assert!(work.module_enabled(ModuleId::Search));
    assert!(work.execute_command("pin-chapters").is_err());
    assert!(!work.panel_preferences(ModuleId::Chapters).unwrap().pinned);
    let invalid = dir.path().join("invalid.json");
    std::fs::write(&invalid, "{malformed}").unwrap();
    assert!(work.load_layout(&invalid).is_err());
    assert_eq!(std::fs::read_to_string(&invalid).unwrap(), "{malformed}");
    let valid = dir.path().join("layout.json");
    work.load_layout(&valid).unwrap();
    work.load_keymap(&valid);
    assert!(work.configuration_warning().is_some());
    assert!(work.load_library(&valid).is_err());
    work.handle_event(Event::Paste("保留手稿".into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "保留手稿");
    work.execute_command("toggle-search-module").unwrap();
    let other = dir.path().join("other");
    work.new_project(&other, "新作品").unwrap();
    assert!(!work.module_enabled(ModuleId::Search));
    assert!(work.execute_command("find").is_err());
}

#[test]
fn closing_history_restores_live_module_but_never_a_restarted_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "焦点").unwrap();
    work.dispatch(Action::Insert("正文".into())).unwrap();
    work.execute_command("chapters").unwrap();
    work.handle_event(Event::Paste("第一".into())).unwrap();
    work.execute_command("history").unwrap();
    key(&mut work, KeyCode::Esc);
    assert_eq!(work.navigation().unwrap().query, "第一");
    work.execute_command("find").unwrap();
    work.handle_event(Event::Paste("正文".into())).unwrap();
    key(&mut work, KeyCode::Enter);
    work.set_module_enabled(ModuleId::Chapters, false).unwrap();
    work.set_module_enabled(ModuleId::Chapters, true).unwrap();
    key(&mut work, KeyCode::Esc);
    assert!(work.navigation().is_none());
    work.handle_event(Event::Paste("新稿".into())).unwrap();
    assert_eq!(work.text(), "新稿");
}

#[test]
fn panel_commands_and_mouse_share_focus_with_editor_and_cancelled_popups() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "鼠标").unwrap();
    work.configure_mouse(true);
    work.execute_command("pin-chapters").unwrap();
    work.execute_command("chapters-wider").unwrap();
    work.execute_command("chapters-right").unwrap();
    let layout = work.workspace_layout(Rect::new(0, 1, 120, 30));
    work.pane_views(layout.editor);
    let panel = &layout.panels[0];
    assert_eq!(panel.area, Rect::new(92, 1, 28, 30));
    let content = work.module_panel(ModuleId::Chapters, 20);
    assert!(content.entries.iter().any(|row| row.contains("第一章")));
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 94,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert!(work.navigation().is_some());
    work.execute_command("find").unwrap();
    key(&mut work, KeyCode::Esc);
    assert!(work.navigation().is_some());
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert!(work.navigation().is_none());
    work.handle_event(Event::Paste("正文".into())).unwrap();
    assert_eq!(work.text(), "正文");
    work.execute_command("toggle-chapters-module").unwrap();
    work.execute_command("toggle-chapters-module").unwrap();
    key(&mut work, KeyCode::F(1));
    assert_eq!(
        work.palette()
            .unwrap()
            .entries
            .iter()
            .filter(|e| e.id == "chapters")
            .count(),
        1
    );
}

#[test]
fn pinned_panels_restore_and_temporary_narrowing_does_not_change_preferences() {
    use novelite_tui::{PanelPreferences, PanelSide};
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let config = dir.path().join("layout.json");
    let mut work = Workspace::create_project(&root, "布局").unwrap();
    work.load_layout(&config).unwrap();
    work.configure_panel(
        ModuleId::Chapters,
        PanelPreferences {
            pinned: true,
            width: 26,
            side: PanelSide::Left,
        },
    )
    .unwrap();
    work.configure_panel(
        ModuleId::Search,
        PanelPreferences {
            pinned: true,
            width: 32,
            side: PanelSide::Right,
        },
    )
    .unwrap();
    let plan = work.workspace_layout(Rect::new(0, 1, 140, 30));
    assert_eq!(plan.editor, Rect::new(27, 1, 80, 30));
    assert_eq!(plan.panels.len(), 2);
    assert_eq!(plan.panels[0].area, Rect::new(0, 1, 26, 30));
    assert_eq!(plan.panels[1].area, Rect::new(108, 1, 32, 30));
    let narrow = work.workspace_layout(Rect::new(0, 1, 50, 30));
    assert_eq!(narrow.editor.width, 50);
    assert!(narrow.panels.is_empty());
    work.execute_command("chapters").unwrap();
    let overlay = work.workspace_layout(Rect::new(0, 1, 50, 30));
    assert_eq!(overlay.panels.len(), 1);
    assert!(overlay.panels[0].focused);
    key(&mut work, KeyCode::Esc);
    work.set_module_enabled(ModuleId::Search, false).unwrap();
    drop(work);
    let mut reopened = Workspace::open_project(&root).unwrap();
    reopened.load_layout(&config).unwrap();
    assert!(!reopened.module_enabled(ModuleId::Search));
    assert_eq!(
        reopened.panel_preferences(ModuleId::Search).unwrap().width,
        32
    );
    reopened.set_module_enabled(ModuleId::Search, true).unwrap();
    assert_eq!(
        reopened
            .workspace_layout(Rect::new(0, 1, 140, 30))
            .panels
            .len(),
        2
    );
    assert!(reopened.navigation().is_none());
    reopened
        .handle_event(Event::Paste("正文仍可输入".into()))
        .unwrap();
    assert_eq!(reopened.text(), "正文仍可输入");
}

#[test]
fn disabling_modules_removes_commands_and_focus_but_preserves_text_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "模块").unwrap();
    work.dispatch(Action::Insert("保留正文".into())).unwrap();
    work.execute_command("chapters").unwrap();
    work.set_module_enabled(ModuleId::Chapters, false).unwrap();
    assert!(work.navigation().is_none());
    assert!(work.execute_command("chapters").is_err());
    work.start_book_search("正文").unwrap();
    work.set_module_enabled(ModuleId::Search, false).unwrap();
    assert!(work.book_search().is_none());
    assert!(work.execute_command("find").is_err());
    assert!(work.start_book_search("正文").is_err());
    key(&mut work, KeyCode::F(1));
    work.execute_command("browse-commands").unwrap();
    let palette = work.palette().unwrap();
    assert!(
        !palette
            .entries
            .iter()
            .any(|e| e.id == "search-book" || e.id == "chapters")
    );
    assert!(palette.entries.iter().any(|e| e.id == "save"));
    assert!(palette.entries.iter().any(|e| e.id == "history"));
    key(&mut work, KeyCode::Esc);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "");
    work.dispatch(Action::Redo).unwrap();
    for _ in 0..3 {
        work.set_module_enabled(ModuleId::Chapters, true).unwrap();
        work.execute_command("chapters").unwrap();
        key(&mut work, KeyCode::Esc);
        work.set_module_enabled(ModuleId::Chapters, false).unwrap();
    }
    work.execute_command("save").unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "保留正文");
}
