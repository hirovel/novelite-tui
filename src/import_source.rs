use crate::importing::Source;
use std::{
    cmp::Ordering,
    fs,
    io::{self, Read},
    path::Path,
};

pub(crate) fn destination(path: &Path) -> io::Result<std::path::PathBuf> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(io::Error::other("目标不能包含 ..，请提供明确的新路径"));
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io::Error::other("请输入新的目标名称"))?;
    let stem = name.split('.').next().unwrap().to_ascii_uppercase();
    if name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&stem.as_str())
    {
        return Err(io::Error::other("目标名称含跨平台保留名称或字符"));
    }
    let absolute = std::path::absolute(path)?;
    reject_link_chain(absolute.parent().unwrap())?;
    let parent = fs::canonicalize(absolute.parent().unwrap())?;
    let destination = parent.join(name);
    match fs::symlink_metadata(&destination) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "目标必须是尚不存在的新路径",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(destination),
        Err(e) => Err(e),
    }
}
pub(crate) fn reject_link_chain(path: &Path) -> io::Result<()> {
    let path = std::path::absolute(path)?;
    for ancestor in path.ancestors() {
        #[cfg(target_os = "macos")]
        if macos_system_alias(ancestor) {
            continue;
        }
        reject_link(ancestor)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_system_alias(path: &Path) -> bool {
    // macOS exposes these system directories through root-level aliases. Do not
    // canonicalize the user path before checking its other ancestors for links.
    let name = match path.to_str() {
        Some("/var") => "var",
        Some("/tmp") => "tmp",
        Some("/etc") => "etc",
        _ => return false,
    };
    let target = Path::new("/private").join(name);
    fs::read_link(path).is_ok_and(|link| {
        (link == target || link == Path::new("private").join(name))
            && fs::canonicalize(path).is_ok_and(|resolved| resolved == target)
    })
}

pub(crate) fn reject_link(path: &Path) -> io::Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    let link = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let link = meta.file_type().is_symlink();
    if link {
        return Err(io::Error::other("来源或目标不能经过符号链接或目录联接"));
    }
    Ok(meta)
}
pub(crate) fn capture(source: &Path) -> io::Result<Vec<Source>> {
    capture_inner(source, false)
}
pub(crate) fn capture_project(source: &Path) -> io::Result<Vec<Source>> {
    reject_link_chain(source)?;
    for marker in [".novelite-journal.json", ".novelite-importing"] {
        if source.join(marker).try_exists()? {
            return Err(io::Error::other("作品有未完成事务，请恢复后再导出目录"));
        }
    }
    capture_inner(source, true)
}
fn capture_inner(source: &Path, project: bool) -> io::Result<Vec<Source>> {
    let mut result = Vec::new();
    let mut total = 0;
    if reject_link(source)?.is_dir() {
        if !project && source.join("novelite.json").try_exists()? {
            return Err(io::Error::other(
                "这是作品目录；请使用打开作品，导入仅用于 TXT/Markdown 正文迁移",
            ));
        }
        visit(source, source, &mut result, &mut total, 0, project)?;
    } else {
        read(
            source,
            source
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| io::Error::other("来源文件名必须是 Unicode"))?,
            &mut result,
            &mut total,
        )?;
    }
    result.sort_by(|a, b| natural(&a.name, &b.name));
    Ok(result)
}
fn visit(
    root: &Path,
    dir: &Path,
    result: &mut Vec<Source>,
    total: &mut usize,
    depth: usize,
    project: bool,
) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::other("来源目录超过32层"));
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        // A save can create a journal after the initial check. Runtime coordination
        // files must never be replayed in the exported copy.
        if project
            && depth == 0
            && path.file_name().is_some_and(|n| {
                n == ".novelite.lock" || n == ".novelite-journal.json" || n == ".novelite-importing"
            })
        {
            continue;
        }
        let meta = reject_link(&path)?;
        if meta.is_dir() {
            visit(root, &path, result, total, depth + 1, project)?;
        } else {
            let name = path
                .strip_prefix(root)
                .unwrap()
                .iter()
                .map(|part| {
                    part.to_str()
                        .ok_or_else(|| io::Error::other("来源文件名必须是 Unicode"))
                })
                .collect::<io::Result<Vec<_>>>()?
                .join("/");
            read(&path, &name, result, total)?;
        }
    }
    Ok(())
}
fn read(path: &Path, name: &str, result: &mut Vec<Source>, total: &mut usize) -> io::Result<()> {
    const LIMIT: usize = 128 * 1024 * 1024;
    if !reject_link(path)?.is_file() || result.len() >= 4096 {
        return Err(io::Error::other("只接受普通文件，最多4096个"));
    }
    let remaining = LIMIT.saturating_sub(*total);
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(remaining as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > remaining {
        return Err(io::Error::other("来源总量超过128MiB，请分批处理"));
    }
    *total += bytes.len();
    result.push(Source {
        name: name.into(),
        bytes,
    });
    Ok(())
}
fn natural(a: &str, b: &str) -> Ordering {
    let (mut left, mut right) = (a.as_bytes(), b.as_bytes());
    while let (Some(&x), Some(&y)) = (left.first(), right.first()) {
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let an = left.iter().take_while(|c| c.is_ascii_digit()).count();
            let bn = right.iter().take_while(|c| c.is_ascii_digit()).count();
            let ax = left[..an].iter().position(|c| *c != b'0').unwrap_or(an);
            let bx = right[..bn].iter().position(|c| *c != b'0').unwrap_or(bn);
            let cmp = (an - ax)
                .cmp(&(bn - bx))
                .then_with(|| left[ax..an].cmp(&right[bx..bn]));
            if cmp != Ordering::Equal {
                return cmp;
            }
            left = &left[an..];
            right = &right[bn..];
        } else {
            if x != y {
                return x.cmp(&y);
            }
            left = &left[1..];
            right = &right[1..];
        }
    }
    left.len().cmp(&right.len()).then_with(|| a.cmp(b))
}
