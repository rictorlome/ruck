use anyhow::Result;
use futures::future::try_join_all;

use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::path::PathBuf;

use bytes::BytesMut;
use std::io::{Read, Seek, SeekFrom};

use tokio::fs::File;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkHeader {
    pub id: u8,
    pub start: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileOffer {
    pub id: u8,
    pub path: PathBuf,
    pub size: u64,
}

pub struct StdFileHandle {
    pub id: u8,
    pub file: std::fs::File,
    pub start: u64,
}

pub struct FileHandle {
    pub id: u8,
    pub file: File,
    pub md: Metadata,
    pub path: PathBuf,
}

impl FileHandle {
    pub async fn new(id: u8, path: PathBuf) -> Result<FileHandle> {
        let file = File::open(&path).await?;
        let md = file.metadata().await?;
        let fh = FileHandle { id, file, md, path };
        return Ok(fh);
    }

    pub async fn to_stds(
        file_handles: Vec<FileHandle>,
        chunk_headers: Vec<ChunkHeader>,
    ) -> Vec<StdFileHandle> {
        let mut ret = Vec::new();
        for handle in file_handles {
            let chunk = chunk_headers.iter().find(|chunk| handle.id == chunk.id);
            match chunk {
                Some(chunk) => {
                    match handle.to_std(chunk).await {
                        Ok(std_file_handle) => {
                            ret.push(std_file_handle);
                        }
                        _ => println!("Error seeking in file"),
                    };
                }
                None => {
                    println!("Skipping file b/c not in requested chunks");
                }
            }
        }
        ret
    }

    async fn to_std(self, chunk_header: &ChunkHeader) -> Result<StdFileHandle> {
        let mut std_file = self.file.into_std().await;
        std_file.seek(SeekFrom::Start(chunk_header.start))?;
        Ok(StdFileHandle {
            id: self.id,
            file: std_file,
            start: chunk_header.start,
        })
    }

    pub fn to_file_offer(&self) -> FileOffer {
        FileOffer {
            id: self.id,
            path: self.path.clone(),
            size: self.md.len(),
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

    pub fn to_message(
        id: u8,
        file: &mut std::fs::File,
        buffer: &mut BytesMut,
        start: u64,
    ) -> Result<u64> {
        // reads the next chunk of the file
        // packs it into the buffer, with the header taking up the first X bytes
        let chunk_header = ChunkHeader { id, start };
        let chunk_bytes = bincode::serialize(&chunk_header)?;
        println!(
            "chunk_bytes = {:?}, len = {:?}",
            chunk_bytes.clone(),
            chunk_bytes.len()
        );
        buffer.extend_from_slice(&chunk_bytes[..]);
        let n = file.read(buffer)? as u64;
        Ok(n)
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
