use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn draw(w: &mut Workspace, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| w.render(f, "")).unwrap();
    terminal.backend().buffer().clone()
}
fn point(b: &Buffer, symbol: &str) -> (u16, u16) {
    let index = b
        .content
        .iter()
        .position(|cell| cell.symbol() == symbol)
        .unwrap_or_else(|| panic!("missing {symbol}"));
    (index as u16 % b.area.width, index as u16 / b.area.width)
}
fn mouse(w: &mut Workspace, kind: MouseEventKind, p: (u16, u16)) {
    w.handle_event(Event::Mouse(MouseEvent {
        kind,
        column: p.0,
        row: p.1,
        modifiers: KeyModifiers::NONE,
    }))
    .unwrap();
}
fn click(w: &mut Workspace, p: (u16, u16)) {
    mouse(w, MouseEventKind::Down(MouseButton::Left), p);
}

#[test]
fn history_shares_the_command_dock_and_clicked_preview_has_reading_space() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    let old = (0..30).map(|n| format!("旧稿{n:02}\n")).collect::<String>();
    w.dispatch(Action::Insert(old.clone())).unwrap();
    w.mark_snapshot().unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Insert("当前稿".into())).unwrap();
    key(&mut w, KeyCode::F(1));
    let command_y = point(&draw(&mut w, 80, 32), "令").1;
    key(&mut w, KeyCode::Esc);
    w.execute_command("history").unwrap();
    let list = draw(&mut w, 80, 32);
    assert_eq!(point(&list, "历").1, command_y);
    assert!(!list.content.iter().any(|c| c.symbol() == "╭"));
    click(&mut w, point(&list, "▎"));
    assert!(w.navigation().is_none());
    let preview = draw(&mut w, 80, 32);
    let visible_lines = (0..32)
        .filter(|&y| (0..80).any(|x| preview[(x, y)].symbol() == "旧"))
        .count();
    assert!(
        visible_lines > 8,
        "history preview must retain reading height"
    );
    click(&mut w, point(&preview, "令"));
    assert!(
        w.palette().is_some(),
        "the visible command entry must work in a preview"
    );
    key(&mut w, KeyCode::Esc);
    let preview = draw(&mut w, 80, 32);
    click(&mut w, point(&preview, "关"));
    assert!(w.history_preview(80, 24).is_none());
    assert_eq!(w.text(), "当前稿");
    w.execute_command("history").unwrap();
    let list = draw(&mut w, 80, 32);
    click(&mut w, point(&list, "▎"));
    let preview = draw(&mut w, 80, 32);
    click(&mut w, point(&preview, "恢"));
    assert_eq!(w.text(), old);
    assert_eq!(std::fs::read_to_string(w.path()).unwrap(), old);
}
#[test]
fn narrow_confirmation_keeps_the_action_count_and_default_cancel_visible() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    let volume = w.project().unwrap().volumes[0].id.clone();
    let chapter = w.active_chapter().unwrap().id.clone();
    let long_name = "长章卷名称".repeat(25);
    w.rename_chapter(&chapter, &long_name).unwrap();
    w.rename_volume(&volume, &long_name).unwrap();
    w.create_chapter(&volume, "另一章").unwrap();
    for (command, scope) in [
        ("recycle-chapter", "回收1章"),
        ("recycle-volume", "回收全部2章"),
    ] {
        w.execute_command(command).unwrap();
        let b = draw(&mut w, 32, 32);
        let visible = b
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
            .split_whitespace()
            .collect::<String>();
        assert!(visible.contains(scope), "{visible}");
        assert!(visible.contains("默认取消"));
        let cancel = point(&b, "▎");
        click(&mut w, (cancel.0 + 2, cancel.1));
        assert!(w.navigation().is_none());
        assert!(w.project().unwrap().recycle_bin.is_empty());
    }
    assert_eq!(w.project().unwrap().volumes[0].chapters.len(), 2);
}
#[test]
fn navigation_hits_expire_when_a_covering_tool_is_opened_and_closed() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("稿件".into())).unwrap();
    w.mark_snapshot().unwrap();
    w.execute_command("history").unwrap();
    let b = draw(&mut w, 80, 32);
    let old = point(&b, "▎");
    key(&mut w, KeyCode::F(1));
    key(&mut w, KeyCode::Esc);
    click(&mut w, old);
    assert!(
        w.navigation().is_some(),
        "a stale history hit must not open a preview"
    );
    assert!(w.history_preview(80, 20).is_none());
    let b = draw(&mut w, 80, 32);
    click(&mut w, point(&b, "▎"));
    assert!(w.history_preview(80, 20).is_some());
    assert_eq!(w.text(), "稿件");
}
#[test]
fn clicked_structure_tools_copy_move_recycle_and_restore_stable_chapters() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    let original = w.active_chapter().unwrap().id.clone();
    w.dispatch(Action::Insert("独立原稿".into())).unwrap();
    let destination = w.create_volume("目标卷").unwrap();
    w.execute_command("copy-chapter").unwrap();
    let b = draw(&mut w, 80, 32);
    let p = point(&b, "▎");
    click(&mut w, (p.0 + 2, p.1 + 1));
    assert!(w.prompt().unwrap().title.contains("复制"));
    w.handle_event(Event::Paste("副本".into())).unwrap();
    key(&mut w, KeyCode::Enter);
    assert_eq!(w.project().unwrap().volumes[1].chapters[0].title, "副本");
    let copy = w.project().unwrap().volumes[1].chapters[0].id.clone();
    w.execute_command("move-chapter").unwrap();
    let b = draw(&mut w, 80, 32);
    let p = point(&b, "▎");
    click(&mut w, (p.0 + 2, p.1 + 1));
    assert_eq!(w.project().unwrap().volumes[1].chapters[1].id, original);
    assert_eq!(w.project().unwrap().volumes[1].id, destination);
    w.execute_command("recycle-chapter").unwrap();
    let b = draw(&mut w, 80, 32);
    let p = point(&b, "▎");
    click(&mut w, (p.0 + 2, p.1 + 1));
    assert_eq!(w.project().unwrap().recycle_bin.len(), 1);
    w.execute_command("recycle-bin").unwrap();
    let b = draw(&mut w, 80, 32);
    let p = point(&b, "▎");
    click(&mut w, (p.0 + 2, p.1));
    assert!(w.navigation().unwrap().title.contains("选择目标卷"));
    click(&mut w, (p.0 + 2, p.1));
    assert_eq!(
        w.project().unwrap().recycle_bin.len(),
        1,
        "previous tool rows must not activate the new destination list"
    );
    let b = draw(&mut w, 80, 32);
    click(&mut w, point(&b, "▎"));
    assert!(w.project().unwrap().recycle_bin.is_empty());
    w.switch_chapter(&original).unwrap();
    assert_eq!(w.text(), "独立原稿");
    w.switch_chapter(&copy).unwrap();
    assert_eq!(w.text(), "独立原稿");
}

#[test]
fn clicked_conflict_choices_keep_both_versions_and_require_typed_overwrite() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("本地稿".into())).unwrap();
    std::fs::write(w.path(), "磁盘稿").unwrap();
    w.execute_command("conflicts").unwrap();
    let b = draw(&mut w, 80, 32);
    click(&mut w, point(&b, "▎"));
    assert!(w.external_conflict().is_some());
    w.execute_command("conflicts").unwrap();
    let b = draw(&mut w, 80, 32);
    let p = point(&b, "▎");
    click(&mut w, (p.0 + 2, p.1 + 2));
    assert!(w.prompt().is_some());
    let rescue = folder.path().join("rescue.md");
    w.handle_event(Event::Paste(rescue.to_string_lossy().into()))
        .unwrap();
    key(&mut w, KeyCode::Enter);
    assert_eq!(std::fs::read_to_string(rescue).unwrap(), "本地稿");
    assert_eq!(std::fs::read_to_string(w.path()).unwrap(), "磁盘稿");
    w.execute_command("conflicts").unwrap();
    let b = draw(&mut w, 80, 32);
    let p = point(&b, "▎");
    click(&mut w, (p.0 + 2, p.1 + 3));
    assert!(w.prompt().unwrap().title.contains("覆盖"));
    assert_eq!(std::fs::read_to_string(w.path()).unwrap(), "磁盘稿");
    w.handle_event(Event::Paste("覆盖".into())).unwrap();
    key(&mut w, KeyCode::Enter);
    assert!(w.external_conflict().is_none());
    assert_eq!(std::fs::read_to_string(w.path()).unwrap(), "本地稿");
}
#[test]
fn failed_history_preview_and_restore_keep_the_current_manuscript_and_tool_context() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("book");
    let mut w = Workspace::create_project(&root, "书").unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("历史稿".into())).unwrap();
    w.mark_snapshot().unwrap();
    let snapshot = w.history_entries().unwrap()[0].id.clone();
    let chapter = w.active_chapter().unwrap().id.clone();
    let path = root.join(format!("history/{chapter}/{snapshot}.json"));
    let valid = std::fs::read(&path).unwrap();
    w.dispatch(Action::SelectAll).unwrap();
    w.dispatch(Action::Insert("当前稿必须保留".into())).unwrap();
    w.dispatch(Action::Save).unwrap();
    w.execute_command("history").unwrap();
    let b = draw(&mut w, 80, 32);
    let row = point(&b, "▎");
    std::fs::write(&path, "{").unwrap();
    let click_event = |p: (u16, u16)| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: p.0,
            row: p.1,
            modifiers: KeyModifiers::NONE,
        })
    };
    assert!(w.handle_event(click_event(row)).is_err());
    assert!(w.navigation().unwrap().title.contains("历史"));
    assert_eq!(w.navigation().unwrap().selected, 0);
    assert_eq!(w.text(), "当前稿必须保留");
    std::fs::write(&path, valid).unwrap();
    let b = draw(&mut w, 80, 32);
    click(&mut w, point(&b, "▎"));
    w.set_history_policy(novelite_tui::HistoryPolicy {
        recent: 50,
        days: 7,
        max_bytes: 1,
    })
    .unwrap();
    let b = draw(&mut w, 80, 32);
    assert!(w.handle_event(click_event(point(&b, "恢"))).is_err());
    assert!(w.history_preview(80, 20).is_some());
    assert_eq!(w.text(), "当前稿必须保留");
    assert_eq!(std::fs::read_to_string(w.path()).unwrap(), "当前稿必须保留");
}

#[test]
fn tool_geometry_is_modal_and_rebuilt_after_filter_resize_and_kind_changes() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("正文".into())).unwrap();
    w.mark_snapshot().unwrap();
    let cursor = w.cursor();
    for (color, background) in [
        ("color-auto", "background-terminal"),
        ("color-mono", "background-terminal"),
        ("color-truecolor", "background-dark"),
    ] {
        w.execute_command(color).unwrap();
        w.execute_command(background).unwrap();
        w.set_design_motion(false);
        for (width, height) in [(32, 24), (80, 32), (120, 32), (180, 32), (24, 8), (1, 1)] {
            w.execute_command("history").unwrap();
            let b = draw(&mut w, width, height);
            assert!(!b.content.iter().any(|c| c.symbol() == "╭"));
            if background == "background-terminal" {
                assert!(
                    b.content
                        .iter()
                        .all(|c| c.bg == ratatui::style::Color::Reset)
                );
            }
            click(&mut w, (0, 0));
            mouse(&mut w, MouseEventKind::ScrollDown, (0, 0));
            assert_eq!(w.cursor(), cursor);
            if width >= 32 && height >= 24 {
                let old = point(&b, "▎");
                w.handle_event(Event::Paste("无匹配".into())).unwrap();
                click(&mut w, old);
                assert!(w.navigation().is_some());
                assert!(w.history_preview(80, 20).is_none());
                let empty = draw(&mut w, width, height);
                assert_eq!(point(&empty, "历").1, point(&b, "历").1);
                key(&mut w, KeyCode::Enter);
                assert!(w.navigation().is_some());
                w.execute_command("recent-projects").unwrap();
                click(&mut w, old);
                assert!(w.prompt().is_none());
                assert!(w.navigation().unwrap().title.contains("最近"));
                draw(&mut w, width, height);
                w.handle_event(Event::Resize(width + 1, height)).unwrap();
                click(&mut w, old);
                assert!(w.prompt().is_none());
                assert!(w.navigation().is_some());
            }
            key(&mut w, KeyCode::Esc);
            assert!(w.navigation().is_none());
            assert_eq!(w.text(), "正文");
        }
    }
    draw(&mut w, 80, 32);
    key(&mut w, KeyCode::Char('/'));
    assert_eq!(w.text(), "正文/");
}

#[test]
fn wheel_and_paging_use_the_same_visible_tool_selection() {
    let folder = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(folder.path().join("book"), "书").unwrap();
    w.configure_mouse(true);
    for i in 0..12 {
        w.dispatch(Action::Insert(format!("稿{i}"))).unwrap();
        w.mark_snapshot().unwrap();
    }
    let body = w.text().to_owned();
    let cursor = w.cursor();
    w.execute_command("history").unwrap();
    draw(&mut w, 120, 32);
    key(&mut w, KeyCode::PageDown);
    assert_eq!(w.navigation().unwrap().selected, 5);
    let b = draw(&mut w, 120, 32);
    mouse(&mut w, MouseEventKind::ScrollDown, point(&b, "▎"));
    assert_eq!(w.navigation().unwrap().selected, 6);
    let b = draw(&mut w, 120, 32);
    mouse(&mut w, MouseEventKind::ScrollDown, (0, 0));
    assert_eq!(w.navigation().unwrap().selected, 6);
    mouse(&mut w, MouseEventKind::ScrollUp, point(&b, "▎"));
    assert_eq!(w.navigation().unwrap().selected, 5);
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.text(), body);
    assert_eq!(w.cursor(), cursor);
}
