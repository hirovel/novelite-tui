use novelite_tui::{Action, Workspace};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore = "owned subprocess of the recovery test"]
fn structure_child() {
    let root = PathBuf::from(std::env::var_os("NOVELITE_STRUCTURE_ROOT").unwrap());
    let operation = std::env::var("NOVELITE_STRUCTURE_OPERATION").unwrap();
    let phase = std::env::var("NOVELITE_STRUCTURE_PHASE").unwrap();
    let mut work = Workspace::open_project(&root).unwrap();
    let first = work.active_chapter().unwrap().id.clone();
    let target = work.project().unwrap().volumes[1].id.clone();
    let blocked = match phase.as_str() {
        "journal" => Some(work.path().to_owned()),
        "body" => Some(root.join("novelite.json")),
        _ => None,
    };
    let original = blocked
        .as_ref()
        .map(|path| fs::metadata(path).unwrap().permissions());
    if let Some(path) = &blocked {
        let mut readonly = original.as_ref().unwrap().clone();
        readonly.set_readonly(true);
        fs::set_permissions(path, readonly).unwrap();
    }
    if phase != "before" {
        let result = match operation.as_str() {
            "move" => work.move_chapter(&first, &target, 0),
            "rename" => {
                work.rename_chapter(&first, "新标题").unwrap();
                work.dispatch(Action::Save).map(|_| ())
            }
            "copy" => {
                let source = work.project().unwrap().volumes[1].chapters[0].id.clone();
                work.copy_chapter(&source, &target, "副本").map(|_| ())
            }
            "recycle" => {
                let id = work.project().unwrap().volumes[1].chapters[0].id.clone();
                work.recycle_chapter(&id).map(|_| ())
            }
            "restore" => {
                let id = work.project().unwrap().recycle_bin[0].id.clone();
                work.restore_recycled(&id, &target, 0)
            }
            _ => panic!("unknown operation"),
        };
        if blocked.is_some() {
            assert!(result.is_err());
            assert!(root.join(".novelite-journal.json").is_file());
        } else {
            result.unwrap();
        }
    }
    if let Some(path) = &blocked {
        fs::set_permissions(path, original.unwrap()).unwrap();
    }
    fs::write(root.join("test-ready"), "ready").unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn killed_structure_transactions_recover_complete_mappings_and_bodies() {
    for operation in ["move", "rename", "copy", "recycle", "restore"] {
        for phase in ["before", "journal", "body", "committed"] {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("book");
            let mut work = Workspace::create_project(&root, "书").unwrap();
            let first = work.active_chapter().unwrap().id.clone();
            work.dispatch(Action::Insert("原稿完整正文".into()))
                .unwrap();
            let target = work.create_volume("下卷").unwrap();
            let second = work.create_chapter(&target, "第二章").unwrap();
            work.switch_chapter(&second).unwrap();
            work.dispatch(Action::Insert("另一章完整正文".into()))
                .unwrap();
            work.switch_chapter(&first).unwrap();
            let recycled = work.recycle_chapter(&second).unwrap();
            if operation != "restore" {
                work.restore_recycled(&recycled, &target, 0).unwrap();
            }
            drop(work);
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "structure_child", "--ignored", "--nocapture"])
                .env("NOVELITE_STRUCTURE_ROOT", &root)
                .env("NOVELITE_STRUCTURE_OPERATION", operation)
                .env("NOVELITE_STRUCTURE_PHASE", phase)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !root.join("test-ready").exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("{operation}/{phase} exited: {status}");
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("{operation}/{phase} timed out");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            child.kill().unwrap();
            child.wait().unwrap();
            let work = Workspace::open_project(&root).unwrap();
            assert!(!work.is_read_only());
            let project = work.project().unwrap();
            let all: Vec<_> = project
                .volumes
                .iter()
                .flat_map(|v| &v.chapters)
                .chain(project.recycle_bin.iter().flat_map(|r| &r.chapters))
                .collect();
            let ids: std::collections::HashSet<_> = all.iter().map(|c| &c.id).collect();
            assert_eq!(ids.len(), all.len(), "{operation}/{phase}");
            assert_eq!(
                all.len(),
                if operation == "copy" && phase != "before" {
                    3
                } else {
                    2
                }
            );
            for chapter in all {
                assert_eq!(
                    fs::read_to_string(root.join(&chapter.path)).unwrap(),
                    if chapter.id == first {
                        "原稿完整正文"
                    } else {
                        "另一章完整正文"
                    },
                    "{operation}/{phase}"
                );
            }
            let changed = phase != "before";
            match operation {
                "move" => assert_eq!(
                    project.volumes[1].chapters.iter().any(|c| c.id == first),
                    changed
                ),
                "rename" => assert_eq!(
                    project.volumes[0].chapters[0].title,
                    if changed { "新标题" } else { "第一章" }
                ),
                "copy" => assert_eq!(
                    project.volumes[1].chapters.len(),
                    if changed { 2 } else { 1 }
                ),
                "recycle" => assert_eq!(project.recycle_bin.len(), usize::from(changed)),
                "restore" => assert_eq!(project.recycle_bin.len(), usize::from(!changed)),
                _ => unreachable!(),
            }
            assert!(!root.join(".novelite-journal.json").exists());
        }
    }
}
