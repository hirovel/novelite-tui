use novelite_tui::{Action, SaveState, Workspace};
use std::fs;

#[test]
fn saving_after_undo_redo_and_a_new_branch_tracks_the_exact_manuscript() {
    for project in [false, true] {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("book");
        let mut work = if project {
            Workspace::create_project(&root, "书").unwrap()
        } else {
            let path = folder.path().join("draft.md");
            fs::write(&path, "").unwrap();
            Workspace::open(path).unwrap()
        };

        work.dispatch(Action::Insert("甲\r\n".into())).unwrap();
        work.dispatch(Action::Save).unwrap();
        work.dispatch(Action::Insert("乙".into())).unwrap();
        work.dispatch(Action::Save).unwrap();
        assert_eq!(fs::read_to_string(work.path()).unwrap(), "甲\r\n乙");

        work.dispatch(Action::Undo).unwrap();
        assert_eq!(work.text(), "甲\r\n");
        assert_eq!(work.save_state(), SaveState::Pending);
        work.dispatch(Action::Save).unwrap();
        assert_eq!(work.save_state(), SaveState::Saved);
        assert_eq!(fs::read_to_string(work.path()).unwrap(), "甲\r\n");

        work.dispatch(Action::Redo).unwrap();
        assert_eq!(work.save_state(), SaveState::Pending);
        work.dispatch(Action::Save).unwrap();
        assert_eq!(work.save_state(), SaveState::Saved);
        assert_eq!(fs::read_to_string(work.path()).unwrap(), "甲\r\n乙");

        work.dispatch(Action::Undo).unwrap();
        work.dispatch(Action::Insert("丙".into())).unwrap();
        work.dispatch(Action::Quit).unwrap();
        assert_eq!(work.save_state(), SaveState::Saved);
        let path = work.path().to_owned();
        drop(work);
        let reopened = if project {
            Workspace::open_project(root).unwrap()
        } else {
            Workspace::open(&path).unwrap()
        };
        assert_eq!(reopened.text(), "甲\r\n丙");
        assert_eq!(fs::read_to_string(path).unwrap(), "甲\r\n丙");
        assert!(!reopened.is_dirty());
    }
}

#[test]
fn save_as_drains_the_old_snapshot_before_confirming_the_new_destination() {
    let folder = tempfile::tempdir().unwrap();
    let original = folder.path().join("original.md");
    let copy = folder.path().join("copy.md");
    fs::write(&original, "旧稿").unwrap();
    let mut work = Workspace::open(&original).unwrap();
    work.dispatch(Action::Insert("临时".into())).unwrap();
    work.request_save().unwrap();
    work.dispatch(Action::Undo).unwrap();

    work.save_as(&copy).unwrap();
    assert_eq!(work.save_state(), SaveState::Saved);
    assert_eq!(work.text(), "旧稿");
    assert_eq!(work.path(), fs::canonicalize(&copy).unwrap());
    assert_eq!(fs::read_to_string(&original).unwrap(), "临时旧稿");
    assert_eq!(fs::read_to_string(&copy).unwrap(), "旧稿");

    work.dispatch(Action::Insert("新".into())).unwrap();
    work.dispatch(Action::Quit).unwrap();
    drop(work);
    assert_eq!(Workspace::open(&copy).unwrap().text(), "新旧稿");
    assert_eq!(fs::read_to_string(original).unwrap(), "临时旧稿");
}
