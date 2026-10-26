use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{Action, Direction, Workspace};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
fn key(w: &mut Workspace, code: KeyCode) {
    w.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
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
fn draw(w: &mut Workspace, width: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(width, 32)).unwrap();
    t.draw(|f| w.render(f, "")).unwrap();
    t.backend().buffer().clone()
}
fn point(b: &Buffer, symbol: &str) -> (u16, u16) {
    let i = b.content.iter().position(|c| c.symbol() == symbol).unwrap();
    (
        (i % b.area.width as usize) as u16,
        (i / b.area.width as usize) as u16,
    )
}
fn line(b: &Buffer, y: u16) -> String {
    (0..b.area.width).map(|x| b[(x, y)].symbol()).collect()
}
#[test]
fn recent_uses_the_shared_dock_and_mouse_close_restores_the_editor() {
    let d = tempfile::tempdir().unwrap();
    let mut w = Workspace::open(d.path().join("稿.txt")).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("正文第一行\n第二行".into()))
        .unwrap();
    w.dispatch(Action::Select(Direction::Left)).unwrap();
    for width in [32, 80, 120, 180] {
        w.dispatch(Action::Move(Direction::End)).unwrap();
        w.dispatch(Action::Select(Direction::Left)).unwrap();
        key(&mut w, KeyCode::F(1));
        let commands = draw(&mut w, width);
        let title = point(&commands, "令");
        key(&mut w, KeyCode::Esc);
        w.execute_command("recent-projects").unwrap();
        let b = draw(&mut w, width);
        let recent = point(&b, "近");
        assert_eq!(recent, title, "recent must share command dock position");
        assert!(!b.content.iter().any(|c| c.symbol() == "╭"));
        let close = point(&b, "关");
        mouse(&mut w, MouseEventKind::Down(MouseButton::Left), close);
        assert!(w.navigation().is_none());
        assert_eq!(w.selected_text(), "行");
        draw(&mut w, width);
        mouse(&mut w, MouseEventKind::Down(MouseButton::Left), close);
        assert!(w.navigation().is_none());
        assert!(w.prompt().is_none());
    }
    assert_eq!(w.text(), "正文第一行\n第二行");
}
#[test]
fn recent_scrolls_and_opens_a_clicked_project_after_saving_the_manuscript() {
    let d = tempfile::tempdir().unwrap();
    let state = d.path().join("recent.json");
    let mut w = Workspace::open(d.path().join("draft.txt")).unwrap();
    let mut projects = vec![];
    for i in 0..9 {
        let root = d.path().join(format!("book{i}"));
        let p = Workspace::create_project(&root, &format!("作品{i}👩‍💻中文长标题")).unwrap();
        projects.push(serde_json::json!({"root":root,"title":format!("作品{i}👩‍💻中文长标题"),"chapter":p.active_chapter().unwrap().id,"positions":{}}));
    }
    std::fs::write(
        &state,
        serde_json::to_vec(&serde_json::json!({"version":1,"projects":projects})).unwrap(),
    )
    .unwrap();
    w.load_library(&state).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("必须保存".into())).unwrap();
    w.execute_command("recent-projects").unwrap();
    let b = draw(&mut w, 80);
    let marker = point(&b, "▎");
    for _ in 0..8 {
        mouse(&mut w, MouseEventKind::ScrollDown, marker);
        draw(&mut w, 80);
    }
    let selected = w.navigation().unwrap().selected;
    assert_eq!(selected, 8);
    let b = draw(&mut w, 80);
    let marker = point(&b, "▎");
    assert!(
        line(&b, marker.1).replace(' ', "").contains("作品5"),
        "{}",
        line(&b, marker.1)
    );
    mouse(
        &mut w,
        MouseEventKind::Down(MouseButton::Left),
        (marker.0 + 3, marker.1),
    );
    assert!(w.navigation().is_none());
    assert_eq!(w.project().unwrap().title, "作品5👩‍💻中文长标题");
    assert_eq!(
        std::fs::read_to_string(d.path().join("draft.txt")).unwrap(),
        "必须保存"
    );
}
#[test]
fn failed_recent_open_keeps_the_draft_and_record_until_explicitly_removed() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("locked");
    let locked = Workspace::create_project(&root, "锁定作品").unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let state = d.path().join("recent.json");
    std::fs::write(&state, serde_json::to_vec(&serde_json::json!({"version":1,"projects":[{"root":root,"title":"锁定作品","chapter":locked.active_chapter().unwrap().id,"positions":{}}]})).unwrap()).unwrap();
    drop(locked);
    std::fs::write(root.join("novelite.json"), "{").unwrap();
    let mut w = Workspace::open(d.path().join("draft.txt")).unwrap();
    w.load_library(&state).unwrap();
    w.configure_mouse(true);
    w.dispatch(Action::Insert("留下原稿".into())).unwrap();
    w.dispatch(Action::Select(Direction::Left)).unwrap();
    w.execute_command("recent-projects").unwrap();
    w.handle_event(Event::Paste("锁定".into())).unwrap();
    let b = draw(&mut w, 80);
    let marker = point(&b, "▎");
    assert!(
        w.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: marker.0 + 2,
            row: marker.1,
            modifiers: KeyModifiers::NONE
        }))
        .is_err()
    );
    assert_eq!(w.text(), "留下原稿");
    assert_eq!(w.selected_text(), "稿");
    assert_eq!(w.navigation().unwrap().entries.len(), 1);
    assert_eq!(w.recent_projects().len(), 2);
    w.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(w.recent_projects().len(), 1);
    assert_eq!(w.recent_projects()[0].kind, novelite_tui::RecentKind::File);
    assert!(root.join("novelite.json").exists());
    let b = draw(&mut w, 32);
    assert!(b.content.iter().any(|c| c.symbol() == "匹"));
    key(&mut w, KeyCode::Enter);
    assert!(w.navigation().is_some());
    key(&mut w, KeyCode::Esc);
    assert_eq!(w.selected_text(), "稿");
    w.dispatch(Action::Undo).unwrap();
    assert_eq!(w.text(), "");
}

#[test]
fn recent_filter_and_mouse_stay_modal_across_narrow_split_and_sidebar_layouts() {
    let d = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(d.path().join("book"), "作品").unwrap();
    w.dispatch(Action::Insert("正文第一行\n第二行".into()))
        .unwrap();
    w.execute_command("split-right").unwrap();
    w.execute_command("pin-chapters").unwrap();
    w.configure_mouse(true);
    for width in [32, 80, 120, 180] {
        w.dispatch(Action::Move(Direction::End)).unwrap();
        w.dispatch(Action::Select(Direction::Left)).unwrap();
        let pane = w.active_pane();
        w.execute_command("recent-projects").unwrap();
        let b = draw(&mut w, width);
        let title = point(&b, "近");
        assert!(title.1 >= 23);
        w.handle_event(Event::Paste("无👩‍💻e\u{301}\n匹配".into()))
            .unwrap();
        key(&mut w, KeyCode::Home);
        key(&mut w, KeyCode::Right);
        key(&mut w, KeyCode::Backspace);
        assert_eq!(w.navigation().unwrap().query, "👩‍💻e\u{301}匹配");
        let b = draw(&mut w, width);
        assert_eq!(point(&b, "近").1, title.1);
        mouse(&mut w, MouseEventKind::Down(MouseButton::Left), (0, 2));
        mouse(&mut w, MouseEventKind::ScrollDown, (0, 2));
        assert_eq!(w.selected_text(), "行");
        assert_eq!(w.active_pane(), pane);
        key(&mut w, KeyCode::Enter);
        assert!(w.navigation().is_some());
        key(&mut w, KeyCode::Esc);
        assert_eq!(w.selected_text(), "行");
        assert!(w.is_split());
        assert_eq!(w.active_pane(), pane);
    }
}

#[test]
fn recent_delete_edits_filter_and_removal_requires_an_explicit_shortcut() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("book");
    let state = d.path().join("recent.json");
    let mut w = Workspace::create_project(&root, "文字作品").unwrap();
    w.load_library(&state).unwrap();
    w.execute_command("recent-projects").unwrap();
    w.handle_event(Event::Paste("文字".into())).unwrap();
    key(&mut w, KeyCode::Home);
    key(&mut w, KeyCode::Delete);
    assert_eq!(w.navigation().unwrap().query, "字");
    assert_eq!(w.recent_projects().len(), 1);
    key(&mut w, KeyCode::End);
    key(&mut w, KeyCode::Delete);
    assert_eq!(w.recent_projects().len(), 1);
    for width in [32, 40, 80, 120] {
        let b = draw(&mut w, width);
        let all = (0..b.area.height).map(|y| line(&b, y)).collect::<String>();
        assert!(
            all.contains("Ctrl+D"),
            "removal must remain discoverable at {width} columns"
        );
    }
    w.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('d'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert!(w.recent_projects().is_empty());
    assert!(root.join("novelite.json").exists());
    assert_eq!(w.navigation().unwrap().query, "字");
}
