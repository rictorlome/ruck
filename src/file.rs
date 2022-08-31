use anyhow::Result;

use futures::future::try_join_all;

use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::path::PathBuf;

use flate2::bufread::GzEncoder;
use flate2::Compression;
use std::io::{copy, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use tokio::fs::File;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkHeader {
    pub id: u8,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub chunk_header: ChunkHeader,
}

pub struct FileHandle {
    pub id: u8,
    pub file: File,
    pub md: Metadata,
    pub path: PathBuf,
}

impl Read for FileHandle {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(2)
    }
}

impl FileHandle {
    pub async fn new(id: u8, path: PathBuf) -> Result<FileHandle> {
        let file = File::open(&path).await?;
        let md = file.metadata().await?;
        let fh = FileHandle { id, file, md, path };
        return Ok(fh);
    }

    pub fn to_file_info(&self) -> FileInfo {
        FileInfo {
            path: self.path.clone(),
            chunk_header: ChunkHeader {
                id: self.id,
                start: 0,
                end: self.md.len(),
            },
        }
    }

    pub async fn get_file_handles(file_paths: &Vec<PathBuf>) -> Result<Vec<FileHandle>> {
        let tasks = file_paths
            .into_iter()
            .enumerate()
            .map(|(idx, path)| FileHandle::new(idx.try_into().unwrap(), path.to_path_buf()));
        let handles = try_join_all(tasks).await?;
        Ok(handles)
    }

    async fn count_lines(self, socket: TcpStream) -> Result<TcpStream, std::io::Error> {
        let file = self.file.into_std().await;
        let mut socket = socket;
        tokio::task::spawn_blocking(move || {
            let reader = BufReader::new(file);
            let mut gz = GzEncoder::new(reader, Compression::fast());
            copy(&mut gz, &mut socket)?;
            Ok(socket)
        })
        .await?
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
