use crate::file::{to_size_string, FileOffer};

use futures::prelude::*;

use tokio::io::{self};

use tokio_util::codec::{FramedRead, LinesCodec};

pub async fn prompt_user_for_file_confirmation(file_offers: Vec<FileOffer>) -> Vec<FileOffer> {
    let mut stdin = FramedRead::new(io::stdin(), LinesCodec::new());
    let mut files = vec![];
    for file_offer in file_offers.into_iter() {
        let mut reply = prompt_user_input(&mut stdin, &file_offer).await;
        while reply.is_none() {
            reply = prompt_user_input(&mut stdin, &file_offer).await;
        }
        match reply {
            Some(true) => files.push(file_offer),
            _ => {}
        }
    }
    files
}

pub async fn prompt_user_input(
    stdin: &mut FramedRead<io::Stdin, LinesCodec>,
    file_offer: &FileOffer,
) -> Option<bool> {
    let prompt_name = &file_offer.path;
    println!(
        "Accept {:?}? ({:?}). (Y/n)",
        prompt_name,
        to_size_string(file_offer.size)
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
