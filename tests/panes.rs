use novelite_tui::{Action, Direction, Workspace};
use ratatui::layout::Rect;

#[test]
fn resized_panes_never_reuse_an_old_revision_after_edit_undo_or_resize() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    let original = "甲乙👩‍💻e\u{301}丙丁。".repeat(12);
    std::fs::write(&path, &original).unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.execute_command("split-right").unwrap();
    work.execute_command("toggle-dialogue").unwrap();
    for width in [120, 121, 160, 119, 120] {
        let area = Rect::new(0, 0, width, 40);
        for _ in 0..3 {
            for pane in work.pane_views(area) {
                assert_eq!(pane.view.lines.concat(), original);
            }
        }
        work.dispatch(Action::Insert("“新稿”".into())).unwrap();
        for _ in 0..3 {
            for pane in work.pane_views(area) {
                assert_eq!(pane.view.lines.concat(), format!("“新稿”{original}"));
                assert!(!pane.view.dialogue.is_empty());
            }
        }
        work.focus_other_pane().unwrap();
        work.dispatch(Action::Undo).unwrap();
        for pane in work.pane_views(area) {
            assert_eq!(pane.view.lines.concat(), original);
            assert!(pane.view.dialogue.is_empty());
        }
        work.dispatch(Action::Redo).unwrap();
        for pane in work.pane_views(area) {
            assert_eq!(pane.view.lines.concat(), format!("“新稿”{original}"));
        }
        work.dispatch(Action::Undo).unwrap();
    }
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[test]
fn side_by_side_panes_have_equal_text_widths_and_aligned_wrapping() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("draft.md");
    std::fs::write(&path, "甲乙丙丁戊己庚辛壬癸".repeat(20)).unwrap();
    let mut work = Workspace::open(&path).unwrap();
    work.execute_command("split-right").unwrap();
    for width in [81, 82, 119, 120, 159, 160] {
        let views = work.pane_views(Rect::new(3, 2, width, 30));
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].area.width, views[1].area.width);
        assert_eq!(views[0].view.lines, views[1].view.lines);
        let gap = views[1].area.x - views[0].area.right();
        assert!((1..=2).contains(&gap));
        assert_eq!(views[1].area.right(), width + 3);
    }
}

#[test]
fn same_chapter_panes_share_text_and_history_but_map_independent_positions() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "双窗").unwrap();
    work.dispatch(Action::Insert("甲乙丙丁".into())).unwrap();
    assert_eq!(work.pane_views(Rect::new(0, 0, 100, 24)).len(), 1);
    work.execute_command("split-right").unwrap();
    work.dispatch(Action::Move(Direction::Home)).unwrap();
    work.dispatch(Action::Insert("前".into())).unwrap();
    let views = work.pane_views(Rect::new(0, 0, 100, 24));
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].view.lines[0], "前甲乙丙丁");
    assert_eq!(views[1].view.lines[0], "前甲乙丙丁");
    work.execute_command("focus-other-pane").unwrap();
    assert_eq!(work.cursor(), "前甲乙丙丁".len());
    work.dispatch(Action::Insert("后".into())).unwrap();
    work.execute_command("focus-other-pane").unwrap();
    assert_eq!(work.cursor(), "前".len());
    assert_eq!(work.text(), "前甲乙丙丁后");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "前甲乙丙丁");
    assert!(work.pane_message().unwrap().contains("第一章"));
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), "前甲乙丙丁");
}

#[test]
fn different_chapters_keep_their_positions_and_both_bodies_survive_save_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "两章").unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    let first_path = work.path().to_owned();
    let volume = work.project().unwrap().volumes[0].id.clone();
    work.dispatch(Action::Insert("第一章原稿".into())).unwrap();
    let second = work.create_chapter(&volume, "第二章").unwrap();
    work.execute_command("split-right").unwrap();
    work.switch_chapter(&second).unwrap();
    let second_path = work.path().to_owned();
    work.dispatch(Action::Insert("第二章初稿".into())).unwrap();
    let views = work.pane_views(Rect::new(0, 0, 100, 24));
    assert_eq!(views[0].view.lines[0], "第二章初稿");
    assert_eq!(views[1].view.lines[0], "第一章原稿");
    work.focus_other_pane().unwrap();
    assert_eq!(work.active_chapter().unwrap().id, first);
    assert_eq!(work.cursor(), "第一章原稿".len());
    work.dispatch(Action::Insert("续写".into())).unwrap();
    work.focus_other_pane().unwrap();
    assert_eq!(work.active_chapter().unwrap().id, second);
    assert_eq!(work.cursor(), "第二章初稿".len());
    work.dispatch(Action::Insert("修改".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(
        std::fs::read_to_string(first_path).unwrap(),
        "第一章原稿续写"
    );
    assert_eq!(
        std::fs::read_to_string(second_path).unwrap(),
        "第二章初稿修改"
    );
    drop(work);
    let mut reopened = Workspace::open_project(&root).unwrap();
    assert_eq!(reopened.text(), "第一章原稿续写");
    reopened.switch_chapter(&second).unwrap();
    assert_eq!(reopened.text(), "第二章初稿修改");
}

#[test]
fn vertical_split_swap_collapse_and_close_preserve_focus_and_unsaved_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("中文👩‍💻长段落".into()))
        .unwrap();
    work.execute_command("split-down").unwrap();
    let large = Rect::new(0, 0, 80, 30);
    let views = work.pane_views(large);
    assert!(views[1].area.y > views[0].area.y);
    work.focus_other_pane().unwrap();
    work.dispatch(Action::Insert("续写".into())).unwrap();
    let small = work.pane_views(Rect::new(0, 0, 30, 8));
    assert_eq!(small.len(), 1);
    assert_eq!(small[0].index, 1);
    work.execute_command("swap-panes").unwrap();
    assert!(work.pane_views(large)[0].active);
    assert_eq!(work.text(), "中文👩‍💻长段落续写");
    work.execute_command("close-pane").unwrap();
    assert_eq!(work.pane_views(large).len(), 1);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "中文👩‍💻长段落");
    work.dispatch(Action::Quit).unwrap();
    assert_eq!(
        std::fs::read_to_string(work.path()).unwrap(),
        "中文👩‍💻长段落"
    );
}

#[test]
fn clicking_the_other_pane_targets_its_cells_and_drag_does_not_cross_focus() {
    use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::open(dir.path().join("draft.md")).unwrap();
    work.dispatch(Action::Insert("甲👩‍💻乙丙".into())).unwrap();
    work.configure_mouse(true);
    work.execute_command("split-right").unwrap();
    let views = work.pane_views(Rect::new(0, 0, 100, 24));
    let right = views[1].area;
    let mouse = |kind, x, y| {
        Event::Mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    };
    work.handle_event(mouse(
        MouseEventKind::Down(MouseButton::Left),
        right.x + 3,
        right.y,
    ))
    .unwrap();
    assert_eq!(work.active_pane(), 1);
    assert_eq!(work.cursor(), "甲".len());
    work.handle_event(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        right.x + 4,
        right.y,
    ))
    .unwrap();
    assert_eq!(work.selected_text(), "👩‍💻");
    work.handle_event(mouse(
        MouseEventKind::Up(MouseButton::Left),
        right.x + 4,
        right.y,
    ))
    .unwrap();
    work.dispatch(Action::Insert("替".into())).unwrap();
    assert_eq!(work.text(), "甲替乙丙");
    work.focus_other_pane().unwrap();
    assert_eq!(work.cursor(), "甲替乙丙".len());
}

#[test]
fn failed_save_preserves_both_panes_and_save_as_keeps_both_manuscripts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "故障").unwrap();
    work.dispatch(Action::Insert("第一章安全稿".into()))
        .unwrap();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "第二章").unwrap();
    work.execute_command("split-right").unwrap();
    work.switch_chapter(&second).unwrap();
    let path = work.path().to_owned();
    let original = std::fs::metadata(&path).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&path, readonly).unwrap();
    work.dispatch(Action::Insert("第二章未保存稿".into()))
        .unwrap();
    assert!(work.execute_command("close-pane").is_err());
    assert!(work.is_split());
    assert_eq!(work.text(), "第二章未保存稿");
    let views = work.pane_views(Rect::new(0, 0, 100, 24));
    assert_eq!(views[1].view.lines[0], "第一章安全稿");
    let copy = dir.path().join("rescued");
    work.save_as(&copy).unwrap();
    work.focus_other_pane().unwrap();
    assert_eq!(work.text(), "第一章安全稿");
    std::fs::set_permissions(&path, original).unwrap();
    drop(work);
    let mut reopened = Workspace::open_project(&copy).unwrap();
    assert_eq!(reopened.text(), "第一章安全稿");
    reopened.switch_chapter(&second).unwrap();
    assert_eq!(reopened.text(), "第二章未保存稿");
}

#[test]
fn selection_in_other_pane_tracks_edits_and_recycled_chapter_has_no_dangling_pane() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "选区").unwrap();
    work.dispatch(Action::Insert("甲乙👩‍💻丙".into())).unwrap();
    work.dispatch(Action::Move(Direction::Home)).unwrap();
    work.dispatch(Action::Move(Direction::Right)).unwrap();
    work.execute_command("split-right").unwrap();
    work.dispatch(Action::Select(Direction::Right)).unwrap();
    work.dispatch(Action::Select(Direction::Right)).unwrap();
    work.focus_other_pane().unwrap();
    work.dispatch(Action::Move(Direction::Home)).unwrap();
    work.dispatch(Action::Insert("前".into())).unwrap();
    work.focus_other_pane().unwrap();
    assert_eq!(work.selected_text(), "乙👩‍💻");
    work.dispatch(Action::Insert("替".into())).unwrap();
    assert_eq!(work.text(), "前甲替丙");
    let first = work.active_chapter().unwrap().id.clone();
    let volume = work.project().unwrap().volumes[0].id.clone();
    let second = work.create_chapter(&volume, "第二章").unwrap();
    work.switch_chapter(&second).unwrap();
    work.recycle_chapter(&first).unwrap();
    assert!(!work.is_split());
    assert_eq!(work.text(), "");
}
