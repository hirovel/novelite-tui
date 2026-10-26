use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
};
use novelite_tui::{Outcome, StartOutcome, StartScreen};
use std::{
    io::{self, IsTerminal},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Novelite: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut path = None;
    let mut create_project = false;
    let mut mouse_enabled = true;
    let mut bracketed_paste = true;
    let mut enhanced_keyboard = true;
    let mut keymap = default_keymap();
    let mut layout = default_keymap().map(|p| p.with_file_name("workspace.json"));
    let mut state = default_keymap().map(|path| path.with_file_name("recent-projects.json"));
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            help();
            return Ok(());
        }
        if arg == "--version" {
            println!("Novelite TUI {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        if arg == "--keymap" || arg == "--state" || arg == "--layout" {
            let value =
                Some(std::path::PathBuf::from(args.next().ok_or_else(|| {
                    io::Error::other("Option needs a JSON path")
                })?));
            if arg == "--keymap" {
                keymap = value;
            } else if arg == "--layout" {
                layout = value;
            } else {
                state = value;
            }
        } else if arg == "--no-mouse" {
            mouse_enabled = false;
        } else if arg == "--no-bracketed-paste" {
            bracketed_paste = false;
        } else if arg == "--no-enhanced-keyboard" {
            enhanced_keyboard = false;
        } else if arg == "--new" {
            create_project = true;
        } else if arg.to_string_lossy().starts_with('-') || path.is_some() {
            return Err(io::Error::other("Unexpected argument; use --help"));
        } else {
            path = Some(std::path::PathBuf::from(arg));
        }
    }
    if create_project && path.is_none() {
        return Err(io::Error::other("--new needs a new project folder path"));
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "Interactive terminal required; no file was changed. Use --help.",
        ));
    }
    let mut start = StartScreen::new(state);
    for reserved in [&keymap, &layout].into_iter().flatten() {
        start.reserve(reserved)?;
    }
    let initial = if let Some(path) = path {
        Some(start.open_requested(path, create_project)?)
    } else {
        start.resume()
    };
    // Construct before initialization so partially initialized terminals are restored too.
    let mut guard = TerminalGuard {
        enhanced: false,
        bracketed_paste,
    };
    let mut terminal = ratatui::try_init()?;
    let mut message = String::new();
    if bracketed_paste && let Err(error) = execute!(io::stdout(), EnableBracketedPaste) {
        message = format!("粘贴分隔启用失败：{error}；请用 F1 → paste 粘贴");
    }
    let mut mouse_captured = false;
    if mouse_enabled {
        match execute!(io::stdout(), EnableMouseCapture) {
            Ok(()) => mouse_captured = true,
            Err(error) => message = format!("鼠标报告启用失败，键盘仍可用：{error}"),
        }
    }
    let mut keyboard_enabled = false;
    if enhanced_keyboard && crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false) {
        guard.enhanced = true;
        keyboard_enabled = execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )
        .is_ok();
    }
    let mut work = if let Some(work) = initial {
        work
    } else {
        start.configure_mouse(mouse_captured);
        crossterm::style::force_color_output(
            novelite_tui::detect_color_support() != novelite_tui::ColorMode::Monochrome,
        );
        if !message.is_empty() {
            start.set_message(format!("{} · {message}", start.message()));
        }
        loop {
            terminal.draw(|frame| start.render(frame))?;
            let event = event::read()?;
            if matches!(
                event,
                event::Event::Mouse(event::MouseEvent {
                    kind: event::MouseEventKind::Moved,
                    ..
                })
            ) {
                continue;
            }
            match start.handle_event(event) {
                Ok(StartOutcome::Open(work)) => {
                    terminal.clear()?;
                    break *work;
                }
                Ok(StartOutcome::Exit) => return Ok(()),
                Ok(StartOutcome::Stay) => {}
                Err(error) => start.set_message(error.to_string()),
            }
        }
    };
    if let Some(path) = keymap {
        work.load_keymap(path);
    }
    if let Some(error) = layout.and_then(|path| work.load_layout(path).err()) {
        message = format!("布局未载入：{error}");
    }
    work.configure_color_support(novelite_tui::detect_color_support());
    work.configure_mouse(mouse_captured);
    work.configure_keyboard(Ok(keyboard_enabled));
    let mut output_colors = None;
    let mut redraw = true;
    loop {
        let before = work.save_state();
        let external_before = work.external_generation();
        let mouse_before = work.mouse_generation();
        let clipboard_before = work.clipboard_generation();
        let search_before = work.book_search_generation();
        let counts_before = work.word_count_generation();
        let import_before = work.import_generation();
        let export_before = work.export_generation();
        if let Err(error) = work.tick(std::time::Instant::now()) {
            message = format!("后台操作失败，正文保留：{error}；F1 查看恢复操作");
            redraw = true;
        }
        redraw |= before != work.save_state();
        redraw |= external_before != work.external_generation();
        redraw |= mouse_before != work.mouse_generation();
        redraw |= clipboard_before != work.clipboard_generation();
        redraw |= search_before != work.book_search_generation();
        redraw |= counts_before != work.word_count_generation();
        redraw |= import_before != work.import_generation();
        redraw |= export_before != work.export_generation();
        if work.mouse_enabled() != mouse_captured {
            let result = if work.mouse_enabled() {
                execute!(io::stdout(), EnableMouseCapture)
            } else {
                execute!(io::stdout(), DisableMouseCapture)
            };
            if let Err(error) = result {
                work.configure_mouse(false);
                message = format!("鼠标模式切换失败：{error}");
            }
            mouse_captured = work.mouse_enabled();
            redraw = true;
        }
        redraw |= work.design_needs_frame();
        if redraw {
            // Include reset/clear in the synchronized frame when switching color modes.
            let _ = execute!(io::stdout(), crossterm::terminal::BeginSynchronizedUpdate);
            let colors = work.prepare_terminal_output();
            if output_colors != Some(colors) {
                execute!(io::stdout(), crossterm::style::ResetColor)?;
                terminal.clear()?;
                output_colors = Some(colors);
            }
            let result = terminal
                .draw(|frame| work.render_design(frame, &message, std::time::Instant::now()));
            let _ = execute!(io::stdout(), crossterm::terminal::EndSynchronizedUpdate);
            result?;
            redraw = false;
        }
        if !event::poll(std::time::Duration::from_millis(
            if work.design_needs_frame() { 16 } else { 50 },
        ))? {
            continue;
        }
        let event = event::read()?;
        if matches!(
            event,
            event::Event::Mouse(event::MouseEvent {
                kind: event::MouseEventKind::Moved,
                ..
            })
        ) {
            continue;
        }
        match work.handle_event(event) {
            Ok(Some(Outcome::Exit)) => return Ok(()),
            Ok(Some(Outcome::Continue)) => message.clear(),
            Ok(None) => {}
            Err(error) => message = format!("操作失败，正文保留：{error}；F1 查看命令并重试"),
        }
        redraw = true;
    }
}

fn help() {
    println!(
        "Novelite TUI {}\n\nUsage: novelite-tui [file.txt|file.md|project-folder] [--keymap keys.json] [--state recent.json] [--layout workspace.json]\n       novelite-tui --new <new-project-folder>\n\nWithout a path, resume the last manuscript or project; otherwise show the start screen.\nCtrl+S / F2: save; Ctrl+Q / F10: save and exit.\nArrow keys, Home, End, Backspace, Delete: edit.\nF1 / Ctrl+P: commands including chapters, recent-projects, new-volume, new-chapter.\nPath prompts: ~/ home folder, Tab completes files/folders.\nEsc: close the active panel. Plain / always remains writing input.\nIndependent terminal fallbacks: --no-mouse, --no-enhanced-keyboard, --no-bracketed-paste.\nWithout bracketed paste, use the paste command for a safe single transaction.\nAutosave: 300ms idle / 2s continuous input. Unpersisted input can be lost on forced exit.",
        env!("CARGO_PKG_VERSION")
    );
}

fn default_keymap() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(std::path::PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        });
    base.map(|base| base.join("novelite-tui").join("keymap.json"))
}

struct TerminalGuard {
    enhanced: bool,
    bracketed_paste: bool,
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.enhanced {
            let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
        // Try each cleanup independently, including after partially successful setup.
        let _ = execute!(io::stdout(), DisableMouseCapture);
        if self.bracketed_paste {
            let _ = execute!(io::stdout(), DisableBracketedPaste);
        }
        let _ = execute!(io::stdout(), crossterm::terminal::EndSynchronizedUpdate);
        ratatui::restore();
    }
}
