use crate::connection::Connection;
use crate::file::{ChunkHeader, FileHandle, FileOffer, StdFileHandle};
use crate::handshake::Handshake;
use crate::message::{FileOfferPayload, FileRequestPayload, Message};
use crate::ui::prompt_user_for_file_confirmation;

use anyhow::{anyhow, Result};

use std::path::PathBuf;
use tokio::fs::File;

use tokio::net::TcpStream;

pub async fn send(file_paths: &Vec<PathBuf>, password: &String) -> Result<()> {
    // Fail early if there are problems generating file handles
    let handles = FileHandle::get_file_handles(file_paths).await?;

    // Establish connection to server
    let socket = TcpStream::connect("127.0.0.1:8080").await?;

    let (handshake, s1) = Handshake::from_password(password);
    // Complete handshake, returning key used for encryption
    let (socket, key) = handshake.negotiate(socket, s1).await?;

    let mut connection = Connection::new(socket, key);
    // Offer files, wait for requested file response
    let requested_chunks = offer_files(&mut connection, &handles).await?;

    // Upload negotiated files
    let std_file_handles = FileHandle::to_stds(handles, requested_chunks).await;
    connection.upload_files(std_file_handles).await?;
    println!("Done uploading.");

    // Exit
    Ok(())
}

pub async fn receive(password: &String) -> Result<()> {
    // Establish connection to server
    let socket = TcpStream::connect("127.0.0.1:8080").await?;
    let (handshake, s1) = Handshake::from_password(password);
    // Complete handshake, returning key used for encryption
    let (socket, key) = handshake.negotiate(socket, s1).await?;
    let mut connection = Connection::new(socket, key);
    // Wait for offered files, respond with desired files
    let std_file_handles = request_specific_files(&mut connection).await?;
    // Download them
    connection.download_files(std_file_handles).await?;
    return Ok(());
}

pub async fn offer_files(
    conn: &mut Connection,
    file_handles: &Vec<FileHandle>,
) -> Result<Vec<ChunkHeader>> {
    // Collect file offer
    let mut files = vec![];
    for handle in file_handles {
        files.push(handle.to_file_offer()?);
    }
    let msg = Message::FileOffer(FileOfferPayload { files });
    // Send file offer
    conn.send_msg(msg).await?;
    // Wait for reply
    let reply = conn.await_msg().await?;
    // Return requested chunks
    match reply {
        Message::FileRequest(file_request_payload) => Ok(file_request_payload.chunks),
        _ => Err(anyhow!("Expecting file request message back")),
    }
}

pub async fn request_specific_files(conn: &mut Connection) -> Result<Vec<StdFileHandle>> {
    // Wait for offer message
    let offer_message = conn.await_msg().await?;
    let offered_files: Vec<FileOffer> = match offer_message {
        Message::FileOffer(file_offer_payload) => file_offer_payload.files,
        _ => return Err(anyhow!("Expecting file offer message")),
    };
    // Prompt user for confirmation of files
    let desired_files = prompt_user_for_file_confirmation(offered_files).await;
    let std_file_handles = create_or_find_files(desired_files).await?;
    let file_request_msg = Message::FileRequest(FileRequestPayload {
        chunks: std_file_handles
            .iter()
            .map(|file| ChunkHeader {
                id: file.id,
                start: file.start,
            })
            .collect(),
    });
    conn.send_msg(file_request_msg).await?;
    Ok(std_file_handles)
}

pub async fn create_or_find_files(desired_files: Vec<FileOffer>) -> Result<Vec<StdFileHandle>> {
    let mut v = Vec::new();
    for desired_file in desired_files {
        let mut filename = desired_file.path;
        filename.push_str(".part");
        let file = match File::open(filename.clone()).await {
            Ok(file) => {
                println!(
                    "File {:?} already exists. Attempting to resume download.",
                    filename
                );
                file
            }
            Err(_) => File::create(filename).await?,
        };
        let metadata = file.metadata().await?;
        println!(
            "Current len: {:?}, Full Size: {:?}",
            metadata.len(),
            desired_file.size
        );
        let std_file_handle =
            StdFileHandle::new(desired_file.id, file, metadata.len(), desired_file.size).await?;
        v.push(std_file_handle)
    }
    return Ok(v);
}
