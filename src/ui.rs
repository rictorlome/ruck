use crate::file::{to_size_string, FileInfo};

use futures::prelude::*;

use tokio::io::{self};

use tokio_util::codec::{FramedRead, LinesCodec};

pub async fn prompt_user_input(
    stdin: &mut FramedRead<io::Stdin, LinesCodec>,
    file_info: &FileInfo,
) -> Option<bool> {
    let prompt_name = file_info.path.file_name().unwrap();
    println!(
        "Accept {:?}? ({:?}). (Y/n)",
        prompt_name,
        to_size_string(file_info.chunk_header.end)
    );
    match stdin.next().await {
        Some(Ok(line)) => match line.as_str() {
            "" | "Y" | "y" | "yes" | "Yes" | "YES" => Some(true),
            "N" | "n" | "NO" | "no" | "No" => Some(false),
            _ => {
                println!("Invalid input. Please enter one of the following characters: [YyNn]");
                return None;
            }
        },
        _ => None,
    }
}
