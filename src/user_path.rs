//! User-facing paths share validation; internal chapter reads keep their own Interface.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(crate) fn expand(path: &Path) -> io::Result<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    expand_home(path, home.as_deref())
}
fn expand_home(path: &Path, home: Option<&Path>) -> io::Result<PathBuf> {
    let value = path.to_string_lossy();
    if value == "~" || value.starts_with("~/") || cfg!(windows) && value.starts_with("~\\") {
        let home = home.ok_or_else(|| io::Error::other("无法确定主目录，请输入完整路径"))?;
        return Ok(if value.len() == 1 {
            home.to_owned()
        } else {
            home.join(&value[2..])
        });
    }
    Ok(path.to_owned())
}

pub(crate) fn resolve(path: &Path, allow_new_file: bool) -> io::Result<PathBuf> {
    let path = expand(path)?;
    let exists = path.try_exists()?;
    if !exists && !allow_new_file {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "路径不存在，请检查文件或作品文件夹",
        ));
    }
    let path = crate::keymap::resolve_destination(&path)?;
    if path.is_dir() {
        if !path.join("novelite.json").try_exists()?
            && !path.join(".novelite-journal.json").try_exists()?
        {
            return Err(io::Error::other(
                "这不是作品文件夹；请继续补全 TXT/Markdown 文件，或选择新建作品",
            ));
        }
        return Ok(path);
    }
    if (exists && !path.is_file()) || !manuscript(&path) {
        return Err(io::Error::other("请选择 TXT/Markdown 文件或作品文件夹"));
    }
    for parent in path.ancestors().skip(1) {
        if parent.join("novelite.json").try_exists()?
            || parent.join(".novelite-journal.json").try_exists()?
            || parent.join(".novelite-importing").try_exists()?
        {
            return Err(io::Error::other(
                "这是作品内的文件，请打开所属作品文件夹，再从章节目录选择",
            ));
        }
    }
    Ok(path)
}
fn manuscript(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("txt") || s.eq_ignore_ascii_case("md"))
}

#[derive(Default)]
pub(crate) struct Completion {
    candidates: Vec<String>,
    selected: usize,
    last: String,
    pub hint: String,
}
impl Completion {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn apply(&mut self, text: &mut String) {
        match self.next(text) {
            Ok(value) => {
                *text = value;
                self.last = text.clone();
            }
            Err(error) => {
                self.reset();
                self.hint = error.to_string();
            }
        }
    }
    fn next(&mut self, text: &str) -> io::Result<String> {
        if self.candidates.is_empty() || self.last != text {
            self.candidates = candidates(text)?;
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1) % self.candidates.len();
        }
        let Some(value) = self.candidates.get(self.selected).cloned() else {
            return Err(io::Error::other("没有匹配的文件或文件夹"));
        };
        self.hint = format!(
            "{} / {} · Tab 下一个 · 输入继续补全",
            self.selected + 1,
            self.candidates.len()
        );
        // A unique directory should allow the next Tab to enter it.
        if self.candidates.len() == 1 {
            self.candidates.clear();
        }
        Ok(value)
    }
}
fn candidates(text: &str) -> io::Result<Vec<String>> {
    let path = expand(Path::new(text))?;
    let ends_in_separator = text.ends_with('/') || cfg!(windows) && text.ends_with('\\');
    let (directory, prefix) = if text.is_empty() || ends_in_separator || text == "~" {
        (
            if text.is_empty() {
                PathBuf::from(".")
            } else {
                path.clone()
            },
            String::new(),
        )
    } else {
        (
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_owned(),
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        )
    };
    let mut entries = Vec::new();
    for (scanned, entry) in fs::read_dir(&directory)?.enumerate() {
        if scanned >= 4096 {
            return Err(io::Error::other("目录条目过多，请输入更完整的路径"));
        }
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&prefix) {
            continue;
        }
        let kind = entry.file_type()?;
        // Do not follow links while enumerating a directory. Opening still canonicalizes.
        if kind.is_dir() || kind.is_file() && manuscript(&entry.path()) {
            let mut value = entry.path().to_string_lossy().into_owned();
            if kind.is_dir() {
                value.push(std::path::MAIN_SEPARATOR);
            }
            if entries.len() >= 256 {
                return Err(io::Error::other("候选过多，请输入更完整的路径"));
            }
            entries.push((!kind.is_dir(), name, value));
        }
    }
    entries.sort();
    Ok(entries.into_iter().map(|(_, _, value)| value).collect())
}

impl crate::Workspace {
    pub(crate) fn protect_user_destination(&self, path: &Path, project: bool) -> io::Result<()> {
        self.library.protect(path, project)?;
        self.protect_layout(path, project)?;
        let destination = crate::keymap::resolve_destination(path)?;
        if let Some(keys) = self
            .commands
            .keymap_path()
            .and_then(|p| crate::keymap::resolve_destination(p).ok())
            && (keys == destination || project && keys.starts_with(&destination))
        {
            return Err(io::Error::other("目标与键位配置冲突"));
        }
        Ok(())
    }
    /// Open a user-requested file/project; unlike internal chapter reads this enforces project ownership.
    pub fn open_user_path(path: impl AsRef<Path>, allow_new_file: bool) -> io::Result<Self> {
        let path = resolve(path.as_ref(), allow_new_file)?;
        if path.is_dir() {
            Self::open_project(path)
        } else {
            Self::open(path)
        }
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn home_expansion_is_explicit_and_does_not_change_process_environment() {
        let home = std::path::Path::new("/test-home");
        assert_eq!(
            super::expand_home(std::path::Path::new("~/雨夜.md"), Some(home)).unwrap(),
            home.join("雨夜.md")
        );
        assert!(super::expand_home(std::path::Path::new("~/a"), None).is_err());
        assert_eq!(
            super::expand_home(std::path::Path::new("~other/a"), None).unwrap(),
            std::path::Path::new("~other/a")
        );
    }
}
