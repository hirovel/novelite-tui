use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(crate) enum WriteMode {
    CreateNew,
    Replace,
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8], mode: WriteMode) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    if matches!(mode, WriteMode::Replace) && path.try_exists()? {
        let permissions = fs::metadata(path)?.permissions();
        if permissions.readonly() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "目标文件只读；请重试或另存",
            ));
        }
        file.as_file().set_permissions(permissions)?;
    }
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    match mode {
        WriteMode::CreateNew => file.persist_noclobber(path),
        WriteMode::Replace => file.persist(path),
    }
    .map_err(|error| error.error)?;
    sync_directory(path.parent().unwrap())
}

pub(crate) fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
