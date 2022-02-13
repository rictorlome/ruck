use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::path::PathBuf;
use tokio::fs::File;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size: u64,
}

pub struct FileHandle {
    pub file: File,
    pub md: Metadata,
    pub path: PathBuf,
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
            path: self.path.clone(),
            size: self.md.len(),
        }
    }
}

const SUFFIX: [&'static str; 9] = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
// Stolen: https://gitlab.com/forkbomb9/human_bytes-rs/-/blob/master/src/lib.rs
pub fn to_size_string(size: u64) -> String {
    let size = size as f64;
    let base = size.log10() / 1024_f64.log10();
    let mut result = format!("{:.1}", 1024_f64.powf(base - base.floor()),)
        .trim_end_matches(".0")
        .to_owned();
    // Add suffix
    result.push(' ');
    result.push_str(SUFFIX[base.floor() as usize]);

    result
}
