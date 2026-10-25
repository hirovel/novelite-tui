use crate::{
    importing::ImportPlan,
    project::{identity, managed_path},
    storage::{WriteMode, atomic_write, sync_directory},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Reserve a fresh root, write all payloads, then publish the manifest last.
pub(crate) fn create(destination: &Path, plan: &ImportPlan) -> io::Result<()> {
    let Bundle { manifest, files } = bundle(plan)?;
    publish(destination, manifest, files)
}
/// Used by imports and portable exports; every file belongs to this exclusive new root.
pub(crate) fn publish(
    destination: &Path,
    manifest: crate::Project,
    files: Vec<(String, Vec<u8>)>,
) -> io::Result<()> {
    fs::create_dir(destination)?;
    let root = fs::canonicalize(destination)?;
    let mut written: Vec<(PathBuf, crate::fingerprint::Fingerprint)> = Vec::new();
    let mut directories = Vec::new();
    let result = (|| {
        let marker = root.join(".novelite-importing");
        let bytes = b"Incomplete import. Original sources are unchanged.";
        atomic_write(&marker, bytes, WriteMode::CreateNew)?;
        written.push((
            marker.clone(),
            crate::fingerprint::Fingerprint::bytes(bytes),
        ));
        for (relative, bytes) in &files {
            let path = managed_path(&root, relative)?;
            let mut parents = Vec::new();
            let mut parent = path.parent().unwrap();
            while parent != root && !parent.try_exists()? {
                parents.push(parent.to_owned());
                parent = parent.parent().unwrap();
            }
            for parent in parents.into_iter().rev() {
                fs::create_dir(&parent)?;
                directories.push(parent);
            }
            atomic_write(&path, bytes, WriteMode::CreateNew)?;
            written.push((path, crate::fingerprint::Fingerprint::bytes(bytes)));
        }
        crate::project::validate(&root, &manifest)?;
        let path = managed_path(&root, "novelite.json")?;
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        atomic_write(&path, &bytes, WriteMode::CreateNew)?;
        written.push((path, crate::fingerprint::Fingerprint::bytes(&bytes)));
        fs::remove_file(&marker)?;
        sync_directory(&root)
    })();
    if result.is_err() {
        // Delete only files this attempt wrote, still carrying our bytes. Never recurse.
        for (path, fingerprint) in written.into_iter().rev() {
            let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
            if managed_path(&root, &relative).is_ok()
                && crate::fingerprint::Fingerprint::read(&path).ok() == Some(fingerprint)
            {
                let _ = fs::remove_file(path);
            }
        }
        for path in directories.into_iter().rev() {
            let _ = fs::remove_dir(path);
        }
        let _ = fs::remove_dir(&root);
    }
    result
}

struct Bundle {
    manifest: crate::Project,
    files: Vec<(String, Vec<u8>)>,
}
fn bundle(plan: &ImportPlan) -> io::Result<Bundle> {
    let mut project = crate::Project {
        version: 1,
        id: identity()?,
        title: plan.preview.title.clone(),
        target_words: None,
        volumes: vec![],
        recycle_bin: vec![],
    };
    let mut files = Vec::new();
    for chapter in &plan.preview.chapters {
        if project
            .volumes
            .last()
            .is_none_or(|v| v.title != chapter.volume)
        {
            project.volumes.push(crate::Volume {
                id: identity()?,
                title: chapter.volume.clone(),
                chapters: vec![],
            });
        }
        let id = identity()?;
        let path = format!("chapters/{id}.md");
        files.push((path.clone(), chapter.text.as_bytes().to_vec()));
        project
            .volumes
            .last_mut()
            .unwrap()
            .chapters
            .push(crate::Chapter {
                id,
                title: chapter.title.clone(),
                path,
                plan: Default::default(),
            });
    }
    let mut index = Vec::new();
    for source in &plan.sources {
        let path = format!("source-originals/{}.bin", identity()?);
        files.push((path.clone(), source.bytes.clone()));
        index.push(serde_json::json!({"original": source.name, "path": path}));
    }
    files.push((
        "source-originals/index.json".into(),
        serde_json::to_vec_pretty(&index)?,
    ));
    Ok(Bundle {
        manifest: project,
        files,
    })
}
