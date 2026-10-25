use novelite_tui::{Action, ColorMode, ColorTheme, Workspace};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend, TestBackend},
};
#[test]
fn explicit_theme_emits_rgb_with_inherited_no_color() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "color_output_child", "--nocapture"])
        .env("NOVELITE_COLOR_PROBE", "explicit")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
#[test]
fn color_output_child() {
    let Ok(case) = std::env::var("NOVELITE_COLOR_PROBE") else {
        return;
    };
    if case == "detect-dumb" {
        assert_eq!(novelite_tui::detect_color_support(), ColorMode::Monochrome);
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut w = Workspace::create_project(dir.path().join("book"), "颜色复现").unwrap();
    w.set_color_theme(ColorTheme::Forest).unwrap();
    w.configure_color_support(novelite_tui::detect_color_support());
    if case.starts_with("auto") {
        w.set_color_mode(ColorMode::Auto).unwrap();
    }
    if case == "mono" {
        w.set_color_mode(ColorMode::Monochrome).unwrap();
    }
    if case == "basic" {
        w.set_color_mode(ColorMode::Basic).unwrap();
    }
    if case == "runtime" {
        w.set_color_mode(ColorMode::Monochrome).unwrap();
        w.prepare_terminal_output();
        assert!(crossterm::style::Colored::ansi_color_disabled_memoized());
        w.set_color_theme(ColorTheme::Forest).unwrap();
    }
    w.dispatch(Action::Insert("颜色应该真实输出".into()))
        .unwrap();
    let mut memory = Terminal::new(TestBackend::new(100, 30)).unwrap();
    memory.draw(|f| w.render(f, "")).unwrap();
    let buffer = memory.backend().buffer();
    w.prepare_terminal_output();
    let mut wire = Vec::new();
    let mut backend = CrosstermBackend::new(&mut wire);
    backend
        .draw(
            buffer
                .content
                .iter()
                .enumerate()
                .map(|(i, c)| ((i % 100) as u16, (i / 100) as u16, c)),
        )
        .unwrap();
    Backend::flush(&mut backend).unwrap();
    let ansi = String::from_utf8(wire).unwrap();
    let rgb = ansi.matches("38;2;").count() + ansi.matches("48;2;").count();
    println!(
        "actual CrosstermBackend bytes={}, rgb_sgr={rgb}",
        ansi.len()
    );
    let colored = ansi.matches("38;").count() + ansi.matches("48;").count();
    match case.as_str() {
        "mono" | "auto-no-color" => assert_eq!(colored, 0),
        "basic" => {
            assert_eq!(rgb, 0);
            assert!(colored > 0);
        }
        _ => assert!(
            rgb > 0,
            "explicit theme has RGB cells but no RGB terminal output"
        ),
    }
    if case == "runtime" {
        w.set_color_mode(ColorMode::Monochrome).unwrap();
        w.prepare_terminal_output();
        let encoded = format!(
            "{}",
            crossterm::style::SetForegroundColor(crossterm::style::Color::Rgb {
                r: 10,
                g: 20,
                b: 30
            })
        );
        assert!(!encoded.contains("38;"));
    }
}

#[test]
fn same_backend_emits_rgb_when_only_no_color_is_removed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "color_output_child", "--nocapture"])
        .env("NOVELITE_COLOR_PROBE", "explicit")
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color")
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(output.status.success());
}

#[test]
fn real_output_respects_auto_explicit_basic_mono_and_runtime_switches() {
    for case in [
        "auto-no-color",
        "auto-empty",
        "auto-windows",
        "auto-kitty",
        "basic",
        "mono",
        "runtime",
    ] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "color_output_child", "--nocapture"])
            .env("NOVELITE_COLOR_PROBE", case)
            .env("TERM", "xterm-256color")
            .env_remove("COLORTERM")
            .env_remove("WT_SESSION")
            .env_remove("TERM_PROGRAM")
            .env_remove("NO_COLOR");
        match case {
            "auto-empty" => {
                command
                    .env("NO_COLOR", "")
                    .env("WT_SESSION", "test-session");
            }
            "auto-windows" => {
                command.env("WT_SESSION", "test-session");
            }
            "auto-kitty" => {
                command.env("TERM", "xterm-kitty");
            }
            "auto-no-color" | "basic" | "runtime" => {
                command.env("NO_COLOR", "1");
            }
            _ => {}
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{case}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{case}: passed actual terminal bytes");
    }
}

#[test]
fn auto_dumb_keeps_monochrome_even_with_modern_terminal_hints() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "color_output_child", "--nocapture"])
        .env("NOVELITE_COLOR_PROBE", "detect-dumb")
        .env_remove("NO_COLOR")
        .env("TERM", "dumb")
        .env("WT_SESSION", "test-session")
        .env("COLORTERM", "truecolor")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
