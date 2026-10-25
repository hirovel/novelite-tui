use novelite_tui::{Action, Workspace};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

// The parent owns expected text. This helper only performs user operations and injects
// real filesystem failures between transaction writes; it never exits normally.
#[test]
#[ignore = "invoked only by the parent crash test"]
fn persistence_child() {
    let root = PathBuf::from(std::env::var_os("NOVELITE_TEST_PROJECT").unwrap());
    let stage = std::env::var("NOVELITE_TEST_STAGE").unwrap();
    let mut work = Workspace::open_project(&root).unwrap();
    work.dispatch(Action::SelectAll).unwrap();
    work.dispatch(Action::Insert("新正文\n完整的一章。".into()))
        .unwrap();
    work.rename_project("新书名").unwrap();
    match stage.as_str() {
        "before-save" => {}
        "journal" | "body" => {
            let blocked = if stage == "journal" {
                work.path().to_owned()
            } else {
                root.join("novelite.json")
            };
            let original = fs::metadata(&blocked).unwrap().permissions();
            let mut readonly = original.clone();
            readonly.set_readonly(true);
            fs::set_permissions(&blocked, readonly).unwrap();
            assert!(work.dispatch(Action::Save).is_err());
            assert!(root.join(".novelite-journal.json").is_file());
            fs::set_permissions(&blocked, original).unwrap();
        }
        "committed" => {
            work.dispatch(Action::Save).unwrap();
            work.dispatch(Action::Insert("尚未保存的最后输入".into()))
                .unwrap();
        }
        "new-chapter" => {
            work.dispatch(Action::Save).unwrap();
            let volume = work.project().unwrap().volumes[0].id.clone();
            let manifest = root.join("novelite.json");
            let original = fs::metadata(&manifest).unwrap().permissions();
            let mut readonly = original.clone();
            readonly.set_readonly(true);
            fs::set_permissions(&manifest, readonly).unwrap();
            assert!(work.create_chapter(&volume, "中断时的新章").is_err());
            assert!(root.join(".novelite-journal.json").is_file());
            fs::set_permissions(&manifest, original).unwrap();
        }
        _ => panic!("Unknown checkpoint"),
    }
    fs::write(root.join("test-ready"), b"ready").unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn killed_process_recovers_old_or_new_complete_project_and_releases_writer_lock() {
    for stage in ["before-save", "journal", "body", "committed", "new-chapter"] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("book");
        let mut work = Workspace::create_project(&root, "旧书名").unwrap();
        work.dispatch(Action::Insert("旧正文".into())).unwrap();
        work.dispatch(Action::Save).unwrap();
        drop(work);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "persistence_child", "--ignored", "--nocapture"])
            .env("NOVELITE_TEST_PROJECT", &root)
            .env("NOVELITE_TEST_STAGE", stage)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("test-ready").exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("Child exited at {stage}: {status}");
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Checkpoint timed out: {stage}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let reopened = Workspace::open_project(&root).unwrap();
        assert!(!reopened.is_read_only(), "lock at {stage}");
        let (body, title) = if stage == "before-save" {
            ("旧正文", "旧书名")
        } else {
            ("新正文\n完整的一章。", "新书名")
        };
        assert_eq!(reopened.text(), body, "session at {stage}");
        assert_eq!(
            fs::read_to_string(reopened.path()).unwrap(),
            body,
            "disk at {stage}"
        );
        assert_eq!(
            reopened.project().unwrap().title,
            title,
            "metadata at {stage}"
        );
        assert!(!root.join(".novelite-journal.json").exists());
        if stage == "new-chapter" {
            let chapters = &reopened.project().unwrap().volumes[0].chapters;
            assert_eq!(chapters.len(), 2);
            assert_eq!(chapters[1].title, "中断时的新章");
            assert_ne!(chapters[0].id, chapters[1].id);
            assert_eq!(
                fs::read_to_string(root.join(&chapters[1].path)).unwrap(),
                ""
            );
        }
    }
}
