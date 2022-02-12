use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::Metadata;
use std::path::PathBuf;
use tokio::fs::File;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileInfo {
    name: OsString,
    size: u64,
}

pub struct FileHandle {
    file: File,
    md: Metadata,
    path: PathBuf,
}

impl FileHandle {
    pub async fn new(path: PathBuf) -> Result<FileHandle> {
        let file = File::open(&path).await?;
        let md = file.metadata().await?;
        let fh = FileHandle { file, md, path };
        return Ok(fh);
    }

    pub fn to_file_info(&self) -> FileInfo {
        FileInfo {
            name: match self.path.file_name() {
                Some(s) => s.to_os_string(),
                None => OsString::from("Unknown"),
            },
            size: self.md.len(),
        }
    }
}

const SUFFIX: [&'static str; 9] = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
// Stolen: https://gitlab.com/forkbomb9/human_bytes-rs/-/blob/master/src/lib.rs
pub fn to_size_string(size: f64) -> String {
    let base = size.log10() / 1024_f64.log10();
    let mut result = format!("{:.1}", 1024_f64.powf(base - base.floor()),)
        .trim_end_matches(".0")
        .to_owned();
    // Add suffix
    result.push(' ');
    result.push_str(SUFFIX[base.floor() as usize]);

    result
}
