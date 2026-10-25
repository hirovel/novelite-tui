use novelite_tui::{Action, Workspace};
use ratatui::{Terminal, backend::TestBackend, style::Modifier};

#[test]
fn paragraph_focus_changes_only_display_and_remembers_the_preference() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let config = dir.path().join("layout.json");
    let original = "首段。\n次段。";
    std::fs::write(&path, original).unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.load_layout(&config).unwrap();
    work.execute_command("style-compact").unwrap();
    work.execute_command("color-mono").unwrap();
    work.execute_command("focus-paragraph").unwrap();
    let mut terminal = Terminal::new(TestBackend::new(50, 12)).unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    let b = terminal.backend().buffer();
    assert!(b[(0, 1)].modifier.contains(Modifier::BOLD));
    assert!(b[(0, 2)].modifier.contains(Modifier::DIM));
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    drop(work);
    let mut work = Workspace::open(&path).unwrap();
    work.load_layout(config).unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    assert!(
        terminal.backend().buffer()[(0, 2)]
            .modifier
            .contains(Modifier::DIM)
    );
    work.execute_command("focus-off").unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    assert!(
        !terminal.backend().buffer()[(0, 2)]
            .modifier
            .contains(Modifier::DIM)
    );
    assert_eq!(work.text(), original);
}

#[test]
fn typewriter_tail_padding_uses_the_same_bounds_for_wheel_and_drag() {
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    let text = (0..30).map(|i| format!("第{i}行\n")).collect::<String>();
    work.dispatch(Action::Insert(text)).unwrap();
    work.configure_mouse(true);
    work.execute_command("toggle-typewriter").unwrap();
    let area = Rect::new(0, 0, 40, 9);
    let view = work.editor_view(area);
    assert_eq!(view.cursor.unwrap().1, 4);
    assert_eq!(view.lines[0], "第26行");
    for (kind, row) in [
        (MouseEventKind::Down(MouseButton::Left), 1),
        (MouseEventKind::Drag(MouseButton::Left), 2),
    ] {
        work.handle_event(Event::Mouse(MouseEvent {
            kind,
            column: 0,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }
    assert_eq!(work.editor_view(area).lines, view.lines);
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    assert_eq!(work.editor_view(area).lines[0], "第29行");
}

#[test]
fn writing_modes_survive_splits_resize_and_module_reenable_without_changing_history() {
    use novelite_tui::ModuleId;
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let config = dir.path().join("layout.json");
    let mut work = Workspace::create_project(&root, "双窗格").unwrap();
    work.load_layout(&config).unwrap();
    let original = "# 原稿\r\n甲说：“你好。”\r\n👩‍💻 e\u{301}\r\n";
    work.dispatch(Action::Insert(original.into())).unwrap();
    work.execute_command("focus-paragraph").unwrap();
    work.execute_command("toggle-dialogue").unwrap();
    work.execute_command("toggle-typewriter").unwrap();
    work.execute_command("split-right").unwrap();
    work.execute_command("focus-other-pane").unwrap();
    let area = Rect::new(0, 0, 120, 24);
    let panes = work.pane_views(area);
    assert_eq!(panes.len(), 2);
    assert!(panes.iter().all(|p| !p.view.dialogue.is_empty()));
    let caret = work.cursor();
    for (width, height) in [(30, 8), (120, 36), (1, 1), (160, 50)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| work.render(f, "")).unwrap();
        assert_eq!(work.cursor(), caret);
    }
    work.set_module_enabled(ModuleId::Immersion, false).unwrap();
    assert!(work.view(80, 24).focus.is_empty());
    assert!(work.view(80, 24).dialogue.is_empty());
    assert!(work.execute_command("focus-sentence").is_err());
    work.set_module_enabled(ModuleId::Immersion, true).unwrap();
    assert!(!work.view(80, 24).dialogue.is_empty());
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "");
    work.dispatch(Action::Redo).unwrap();
    assert_eq!(work.text(), original);
    work.dispatch(Action::Quit).unwrap();
    let body = work.path().to_owned();
    drop(work);
    assert_eq!(std::fs::read_to_string(body).unwrap(), original);
    let mut work = Workspace::open_project(root).unwrap();
    work.load_layout(config).unwrap();
    assert!(!work.zen_mode());
    assert!(!work.view(80, 24).dialogue.is_empty());
}

#[test]
fn failed_preference_write_and_zen_request_keep_an_outline_draft_intact() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "安全").unwrap();
    let config = dir.path().join("layout.json");
    work.load_layout(&config).unwrap();
    std::fs::create_dir(&config).unwrap();
    assert!(work.execute_command("focus-paragraph").is_err());
    assert!(work.view(80, 24).focus.is_empty());
    work.execute_command("edit-outline").unwrap();
    work.dispatch(Action::Insert("待确认大纲".into())).unwrap();
    assert!(work.execute_command("toggle-zen").is_err());
    assert!(work.prompt().is_some());
    assert!(!work.zen_mode());
    work.dispatch(Action::Save).unwrap();
    assert_eq!(work.chapter_plan().unwrap().outline, "待确认大纲");
}

#[test]
fn zen_hides_pinned_panels_and_restores_only_live_focus_without_rewriting_layout() {
    use novelite_tui::ModuleId;
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "禅").unwrap();
    let config = dir.path().join("layout.json");
    work.load_layout(&config).unwrap();
    work.execute_command("pin-chapters").unwrap();
    work.execute_command("pin-planning").unwrap();
    work.execute_command("chapters").unwrap();
    let area = Rect::new(0, 1, 150, 30);
    assert_eq!(work.workspace_layout(area).panels.len(), 2);
    let before = std::fs::read(&config).unwrap();
    work.execute_command("toggle-zen").unwrap();
    assert!(work.workspace_layout(area).panels.is_empty());
    assert!(!work.module_focused(ModuleId::Chapters));
    work.dispatch(Action::Insert("原文".into())).unwrap();
    work.workspace_layout(Rect::new(0, 1, 30, 8));
    work.execute_command("toggle-zen").unwrap();
    assert_eq!(work.workspace_layout(area).panels.len(), 2);
    assert!(work.module_focused(ModuleId::Chapters));
    assert_eq!(std::fs::read(&config).unwrap(), before);
    work.execute_command("toggle-zen").unwrap();
    work.set_module_enabled(ModuleId::Chapters, false).unwrap();
    work.execute_command("toggle-immersion-module").unwrap();
    assert!(!work.module_focused(ModuleId::Chapters));
    assert_eq!(work.workspace_layout(area).panels.len(), 1);
    assert!(work.execute_command("toggle-zen").is_err());
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "");
}

#[test]
fn typewriter_centers_edits_but_not_mouse_placement_or_repeated_frames() {
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use novelite_tui::Direction;
    use ratatui::layout::Rect;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let original = (0..30).map(|i| format!("第{i}行\n")).collect::<String>();
    std::fs::write(&path, &original).unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.configure_mouse(true);
    let area = Rect::new(0, 0, 40, 9);
    work.editor_view(area);
    for _ in 0..20 {
        work.dispatch(Action::Move(Direction::Down)).unwrap();
    }
    assert_eq!(work.editor_view(area).cursor.unwrap().1, 8);
    work.execute_command("toggle-typewriter").unwrap();
    work.dispatch(Action::Insert("续".into())).unwrap();
    assert_eq!(work.editor_view(area).cursor.unwrap().1, 4);
    work.dispatch(Action::Newline).unwrap();
    let view = work.editor_view(area);
    assert_eq!(view.cursor.unwrap().1, 4);
    assert_eq!(work.editor_view(area).lines, view.lines);
    work.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
    let clicked = work.editor_view(area);
    assert_eq!(clicked.cursor.unwrap().1, 2);
    assert_eq!(clicked.lines, view.lines);
    work.dispatch(Action::Insert("改".into())).unwrap();
    assert_eq!(work.editor_view(area).cursor.unwrap().1, 4);
    work.dispatch(Action::Undo).unwrap();
    work.editor_view(Rect::new(0, 0, 20, 5));
    work.dispatch(Action::Undo).unwrap();
    work.dispatch(Action::Undo).unwrap();
    work.execute_command("toggle-typewriter").unwrap();
    assert_eq!(work.text(), original);
}

#[test]
fn dialogue_highlights_quotes_across_wraps_and_resets_at_hard_newlines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(&path, "旁白“你好。”后文\n「未完\n正常").unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.execute_command("toggle-dialogue").unwrap();
    let view = work.view(4, 12);
    assert!(!view.dialogue.contains(&(0, 0, 2)));
    assert!(view.dialogue.contains(&(0, 1, 1)));
    // Closing curly quote follows the fullwidth period across a soft wrap.
    assert!(!view.dialogue.is_empty());
    work.execute_command("style-compact").unwrap();
    work.execute_command("color-mono").unwrap();
    let mut terminal = Terminal::new(TestBackend::new(50, 12)).unwrap();
    terminal.draw(|f| work.render(f, "")).unwrap();
    let b = terminal.backend().buffer();
    assert!(b[(4, 1)].modifier.contains(Modifier::UNDERLINED));
    assert!(!b[(0, 3)].modifier.contains(Modifier::UNDERLINED));
    work.execute_command("toggle-dialogue").unwrap();
    assert!(work.view(4, 12).dialogue.is_empty());
    assert_eq!(work.text(), "旁白“你好。”后文\n「未完\n正常");
}

#[test]
fn sentence_and_nearby_rows_follow_the_caret_across_soft_wraps() {
    use novelite_tui::Direction;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(&path, "甲。乙！\n丙\n丁\n戊").unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.execute_command("focus-sentence").unwrap();
    let view = work.view(4, 8);
    assert!(view.focus.contains(&(0, 0, 2, true)));
    assert!(view.focus.contains(&(2, 0, 2, true)));
    assert!(view.focus.contains(&(0, 1, 2, false)));
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    let view = work.view(4, 8);
    assert!(view.focus.contains(&(0, 0, 2, false)));
    assert!(view.focus.contains(&(0, 1, 2, true)));
    work.execute_command("focus-nearby").unwrap();
    let view = work.view(4, 8);
    assert!(view.focus.contains(&(0, 0, 2, true)));
    assert!(view.focus.contains(&(0, 2, 2, true)));
    assert!(view.focus.contains(&(0, 3, 2, false)));
    assert_eq!(work.text(), "甲。乙！\n丙\n丁\n戊");
}
