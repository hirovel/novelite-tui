# Third-party notices

Novelite TUI is licensed under GPL-3.0-only. Dependencies retain their respective licenses.

`vendor/crossterm` contains Crossterm 0.29.0, copyright (c) 2019 Timon, under the MIT license included in `vendor/crossterm/LICENSE`. Source: https://github.com/crossterm-rs/crossterm (crates.io release 0.29.0).

This copy changes Windows Alt-code text commits to produce a press event, matching the Unicode surrogate path, and adds a regression test. Ordinary key-release events are unchanged. It also bounds the Unix enhanced-keyboard response wait, including the optional device-attributes reply, to 250 milliseconds and stops retrying I/O errors indefinitely. The override is declared in `Cargo.toml`; review these changes when upgrading Crossterm.
