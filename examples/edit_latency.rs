//! Measures session edit + layout, not terminal output or screen latency.
use novelite_tui::{Action, Workspace};
use std::{fs, time::Instant};

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.txt");
    let body: String = "中文段落与长句“雨夜”。👩‍💻e\u{301}\t"
        .chars()
        .cycle()
        .take(100_000)
        .collect();
    fs::write(&path, &body).unwrap();
    let mut work = Workspace::open(path).unwrap();
    work.view(80, 24);
    println!("sample,edit_layout_us,undo_layout_us");
    for sample in 0..1050 {
        let start = Instant::now();
        work.dispatch(Action::Insert("字".into())).unwrap();
        std::hint::black_box(work.view(80, 24));
        let edit = start.elapsed().as_micros();
        let start = Instant::now();
        work.dispatch(Action::Undo).unwrap();
        std::hint::black_box(work.view(80, 24));
        let undo = start.elapsed().as_micros();
        if sample >= 50 {
            println!("{},{edit},{undo}", sample - 50);
        }
    }
    assert_eq!(work.text(), body);
}
