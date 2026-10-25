use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Fingerprint {
    Missing,
    Content { length: u64, checksum: u64 },
}
pub(crate) fn ensure(path: &Path, expected: Fingerprint, written: Fingerprint) -> io::Result<()> {
    let current = Fingerprint::read(path)?;
    if current == expected || current == written {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "磁盘内容在保存前改变；保留双方，需处理外部冲突",
        ))
    }
}
impl Fingerprint {
    pub fn bytes(bytes: &[u8]) -> Self {
        let checksum = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
        Self::Content {
            length: bytes.len() as u64,
            checksum,
        }
    }
    pub fn read(path: &Path) -> io::Result<Self> {
        match fs::read(path) {
            Ok(bytes) => Ok(Self::bytes(&bytes)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::Missing),
            Err(e) => Err(e),
        }
    }
}
