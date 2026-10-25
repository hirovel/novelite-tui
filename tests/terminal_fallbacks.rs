use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use novelite_tui::{ColorMode, Outcome, Workspace};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn basic_keyboard_without_mouse_paste_or_focus_reports_can_write_split_save_and_exit() {
    let dir = tempfile::tempdir().unwrap();
    for color in [
        ColorMode::TrueColor,
        ColorMode::Basic,
        ColorMode::Monochrome,
    ] {
        let path = dir.path().join(format!("{color:?}.md"));
        let mut work = Workspace::open(&path).unwrap();
        assert!(!work.configure_keyboard(Ok(false)));
        work.configure_mouse(false);
        work.configure_color_support(color);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let text = "中文👩‍💻e\u{301}";
        // Plain committed characters: this intentionally does not simulate an IME.
        for ch in text.chars() {
            work.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )))
            .unwrap();
        }
        work.execute_command("split-right").unwrap();
        terminal.draw(|f| work.render(f, "")).unwrap();
        let before = work.cursor();
        work.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 3,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
        assert_eq!(work.cursor(), before);
        work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)))
            .unwrap();
        assert_eq!(work.active_pane(), 1);
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('续'),
            KeyModifiers::NONE,
        )))
        .unwrap();
        work.handle_event(Event::Resize(30, 8)).unwrap();
        terminal
            .resize(ratatui::layout::Rect::new(0, 0, 30, 8))
            .unwrap();
        terminal.draw(|f| work.render(f, "")).unwrap();
        work.handle_event(Event::Key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE)))
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "中文👩‍💻e\u{301}续");
        assert_eq!(
            work.handle_event(Event::Key(KeyEvent::new(
                KeyCode::F(10),
                KeyModifiers::NONE
            )))
            .unwrap(),
            Some(Outcome::Exit)
        );
    }
}

#[test]
fn cli_fallbacks_are_independent_and_noninteractive_runs_leave_files_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.md");
    for args in [
        vec!["--no-mouse"],
        vec!["--no-enhanced-keyboard"],
        vec!["--no-bracketed-paste"],
        vec![
            "--no-mouse",
            "--no-enhanced-keyboard",
            "--no-bracketed-paste",
        ],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_novelite-tui"))
            .args(args)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("Interactive terminal required")
        );
        assert!(output.stdout.is_empty());
        assert!(!path.exists());
    }
}
