use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use novelite_tui::{Action, Workspace};

fn key(work: &mut Workspace, code: KeyCode) {
    work.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .unwrap();
}
fn format(work: &mut Workspace, command: &str) {
    work.execute_command(command).unwrap();
    key(work, KeyCode::Enter);
}

#[test]
fn paragraph_format_names_the_chapter_snapshots_and_undoes_as_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "排版").unwrap();
    let original = "\n  第一段  \n\n\n\t第二段\n# 标题\n> 引文";
    work.dispatch(Action::Insert(original.into())).unwrap();
    work.execute_command("format-paragraphs").unwrap();
    assert!(work.prompt().unwrap().title.contains("第一章"));
    assert_eq!(work.text(), original);
    key(&mut work, KeyCode::Enter);
    assert_eq!(work.text(), "\n　　第一段\n\n　　第二段\n# 标题\n> 引文");
    let entry = work.history_entries().unwrap().remove(0);
    assert_eq!(work.preview_snapshot(&entry.id).unwrap(), original);
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), original);
    work.dispatch(Action::Redo).unwrap();
    assert_eq!(work.text(), "\n　　第一段\n\n　　第二段\n# 标题\n> 引文");
    assert_eq!(work.word_progress().chapter_words, 10);
    work.dispatch(Action::Save).unwrap();
    assert_eq!(
        std::fs::read_to_string(work.path()).unwrap(),
        "\n　　第一段\n\n　　第二段\n# 标题\n> 引文"
    );
}

#[test]
fn manual_tools_match_desktop_prose_and_preserve_markdown_and_crlf() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "工具").unwrap();
    for (command, input, expected) in [
        ("remove-indents", "　　甲\n\n\n  乙  ", "甲\n\n乙"),
        (
            "clean-punctuation",
            "他说:\"你好,世界!\"\n价格3.14,正常... -- (附注)",
            "他说：“你好，世界！”\n价格3.14，正常…… —— （附注）",
        ),
        (
            "pangu-spacing",
            "中文ABC123测试 A中文 B2。",
            "中文 ABC123 测试 A 中文 B2 。",
        ),
        (
            "format-paragraphs",
            "# 标题\r\n\r\n\r\n  正文  \r\n",
            "# 标题\r\n\r\n　　正文\r\n",
        ),
    ] {
        work.dispatch(Action::SelectAll).unwrap();
        work.dispatch(Action::Insert(input.into())).unwrap();
        format(&mut work, command);
        assert_eq!(work.text(), expected, "{command}");
        work.dispatch(Action::Undo).unwrap();
        assert_eq!(work.text(), input);
    }
    let markdown = "# 标题\r\n- 列表\r\n  1. 嵌套\r\n> 引文\r\n---\r\n[链接](https://example.com?a=1)\r\n文字`a--b`示例\r\n|列A|列B|\r\n```rust\r\n  a--b!\r\n\r\n\r\n```\r\n";
    for command in [
        "format-paragraphs",
        "remove-indents",
        "clean-punctuation",
        "pangu-spacing",
    ] {
        work.dispatch(Action::SelectAll).unwrap();
        work.dispatch(Action::Insert(markdown.into())).unwrap();
        format(&mut work, command);
        assert_eq!(
            work.text(),
            markdown,
            "{command} must preserve Markdown structure"
        );
    }
}

#[test]
fn formatting_failure_stale_confirmation_and_cancel_leave_the_original_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let mut work = Workspace::create_project(&root, "安全").unwrap();
    let original = "  原稿\n\n\n尾段 ";
    work.dispatch(Action::Insert(original.into())).unwrap();
    work.dispatch(Action::Save).unwrap();
    std::fs::write(root.join("history"), "占用历史路径").unwrap();
    work.execute_command("format-paragraphs").unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    assert_eq!(work.text(), original);
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), original);
    key(&mut work, KeyCode::Esc);
    std::fs::remove_file(root.join("history")).unwrap();
    work.execute_command("clean-punctuation").unwrap();
    key(&mut work, KeyCode::Esc);
    assert!(work.history_entries().unwrap().is_empty());
    work.execute_command("format-paragraphs").unwrap();
    work.dispatch(Action::Insert("后来".into())).unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    assert_eq!(work.text(), "  原稿\n\n\n尾段 后来");
    key(&mut work, KeyCode::Esc);
    let volume = work.project().unwrap().volumes[0].id.clone();
    let other = work.create_chapter(&volume, "另章").unwrap();
    work.execute_command("format-paragraphs").unwrap();
    work.switch_chapter(&other).unwrap();
    assert!(
        work.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE
        )))
        .is_err()
    );
    assert_eq!(work.text(), "");
    key(&mut work, KeyCode::Esc);
    let mut reader = Workspace::open_project(&root).unwrap();
    assert!(reader.execute_command("format-paragraphs").is_err());
}

#[test]
fn optional_pairing_wraps_selection_skips_committed_closers_and_keeps_paste_literal() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "配对").unwrap();
    work.execute_command("toggle-pairing").unwrap();
    key(&mut work, KeyCode::Char('“'));
    assert_eq!(work.text(), "“”");
    assert_eq!(work.cursor(), "“".len());
    key(&mut work, KeyCode::Char('你'));
    key(&mut work, KeyCode::Char('”'));
    assert_eq!(work.text(), "“你”");
    assert_eq!(work.cursor(), work.text().len());
    work.dispatch(Action::SelectAll).unwrap();
    key(&mut work, KeyCode::Char('（'));
    assert_eq!(work.text(), "（“你”）");
    assert_eq!(work.selected_text(), "“你”");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "“你”");
    work.dispatch(Action::Redo).unwrap();
    assert_eq!(work.selected_text(), "“你”");
    work.dispatch(Action::SelectAll).unwrap();
    work.handle_event(Event::Paste("“已经成对”()".into()))
        .unwrap();
    assert_eq!(work.text(), "“已经成对”()");
    key(&mut work, KeyCode::Char('('));
    assert_eq!(work.text(), "“已经成对”()()");
    key(&mut work, KeyCode::Backspace);
    assert_eq!(work.text(), "“已经成对”()");
    work.dispatch(Action::Undo).unwrap();
    assert_eq!(work.text(), "“已经成对”()()");
    work.execute_command("toggle-pairing").unwrap();
    key(&mut work, KeyCode::Backspace);
    assert_eq!(work.text(), "“已经成对”())");
    work.dispatch(Action::SelectAll).unwrap();
    work.handle_event(Event::Paste("  原样...\n\n\n正文ABC".into()))
        .unwrap();
    work.dispatch(Action::Save).unwrap();
    assert_eq!(
        std::fs::read_to_string(work.path()).unwrap(),
        "  原样...\n\n\n正文ABC"
    );
}

#[test]
fn typography_module_has_no_panel_and_remembers_pairing_without_hidden_commands() {
    use novelite_tui::ModuleId;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("book");
    let layout = dir.path().join("layout.json");
    let mut work = Workspace::create_project(&root, "模块").unwrap();
    work.load_layout(&layout).unwrap();
    work.execute_command("toggle-pairing").unwrap();
    work.execute_command("format-paragraphs").unwrap();
    work.execute_command("toggle-typography-module").unwrap();
    assert!(work.prompt().is_none());
    assert!(!work.pairing_enabled());
    assert!(work.execute_command("format-paragraphs").is_err());
    assert!(work.panel_preferences(ModuleId::Typography).is_none());
    assert!(!work.module_focused(ModuleId::Typography));
    key(&mut work, KeyCode::Char('('));
    assert_eq!(work.text(), "(");
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    let mut work = Workspace::open_project(&root).unwrap();
    work.load_layout(layout).unwrap();
    assert!(!work.module_enabled(ModuleId::Typography));
    work.execute_command("toggle-typography-module").unwrap();
    assert!(work.pairing_enabled());
    work.dispatch(Action::SelectAll).unwrap();
    work.handle_event(Event::Paste("".into())).unwrap();
    key(&mut work, KeyCode::Char('「'));
    assert_eq!(work.text(), "「」");
    let layout = work.workspace_layout(ratatui::layout::Rect::new(0, 1, 120, 30));
    assert!(layout.panels.iter().all(|p| p.id != ModuleId::Typography));
}

#[test]
fn paragraph_cleanup_collapses_trailing_blanks_like_desktop_and_keeps_fenced_blocks_exact() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "空行").unwrap();
    for (input, expected) in [
        ("甲\n\n\n", "　　甲\n"),
        ("\r\n\r\n", ""),
        ("甲\r\n\r\n乙\r\n\r\n", "　　甲\r\n\r\n　　乙\r\n"),
        ("```\n\n\n", "```\n\n\n"),
    ] {
        work.dispatch(Action::SelectAll).unwrap();
        work.dispatch(Action::Insert(input.into())).unwrap();
        format(&mut work, "format-paragraphs");
        assert_eq!(work.text(), expected);
    }
}

#[test]
fn paired_backspace_does_not_split_decorated_bracket_graphemes() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "字素").unwrap();
    work.set_pairing_enabled(true).unwrap();
    for (input, expected) in [("()\u{301}", ")\u{301}"), ("(\u{301})", ")")] {
        work.dispatch(Action::SelectAll).unwrap();
        work.handle_event(Event::Paste(input.into())).unwrap();
        key(&mut work, KeyCode::Home);
        key(&mut work, KeyCode::Right);
        key(&mut work, KeyCode::Backspace);
        assert_eq!(work.text(), expected);
        work.dispatch(Action::Undo).unwrap();
        assert_eq!(work.text(), input);
    }
}

#[test]
fn manual_typography_preserves_reference_links_and_images() {
    let dir = tempfile::tempdir().unwrap();
    let mut work = Workspace::create_project(dir.path().join("book"), "引用").unwrap();
    let original = "[前文][ref]\n![图像][ref]\n[ref][]\n[ref]\n\n[ref]: chapter.md";
    work.dispatch(Action::Insert(original.into())).unwrap();
    for command in [
        "clean-punctuation",
        "format-paragraphs",
        "remove-indents",
        "pangu-spacing",
    ] {
        format(&mut work, command);
        assert_eq!(work.text(), original, "{command}");
    }
    work.dispatch(Action::Save).unwrap();
    assert_eq!(std::fs::read_to_string(work.path()).unwrap(), original);
}
