use crate::connection::Connection;
use crate::file::{ChunkHeader, FileHandle, FileOffer, StdFileHandle};
use crate::handshake::Handshake;
use crate::message::{FileOfferPayload, FileRequestPayload, Message};
use crate::password::validate_generate_pw;
use crate::ui::prompt_user_for_file_confirmation;

use anyhow::{anyhow, Context, Result};

use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};

use tokio::net::TcpStream;
use tracing::{error, info, warn};

pub async fn send(file_paths: &Vec<PathBuf>, password: &Option<String>, relay: &str) -> Result<()> {
    // Fail early if there are problems generating file handles
    let handles = FileHandle::get_file_handles(file_paths).await?;

    // Establish connection to server
    let socket = TcpStream::connect(relay)
        .await
        .with_context(|| format!("Failed to connect to relay at {}", relay))?;
    socket.set_nodelay(true)?;

    let pw = validate_generate_pw(password.clone())?;
    info!(
        "Type `ruck receive {} --relay {}` on other end to receive file(s).",
        pw, relay
    );
    let (handshake, s1) = Handshake::from_password(&pw)?;
    // Complete handshake, returning key used for encryption
    let (socket, key) = handshake
        .negotiate(socket, s1)
        .await
        .map_err(|e| {
            error!("Connection lost during handshake. The server may have rejected the connection (at capacity) or peer matching timed out.");
            e
        })?;

    let mut connection = Connection::new(socket, key);
    // Offer files, wait for requested file response
    let requested_chunks = offer_files(&mut connection, &handles).await?;

    // Upload negotiated files
    let std_file_handles = FileHandle::to_stds(handles, requested_chunks).await;
    connection.upload_files(std_file_handles).await?;
    info!("Done uploading.");

    // Exit
    Ok(())
}

pub async fn receive(password: &String, relay: &str) -> Result<()> {
    // Establish connection to server
    let socket = TcpStream::connect(relay)
        .await
        .with_context(|| format!("Failed to connect to relay at {}", relay))?;
    socket.set_nodelay(true)?;
    let (handshake, s1) = Handshake::from_password(password)?;
    // Complete handshake, returning key used for encryption
    let (socket, key) = handshake
        .negotiate(socket, s1)
        .await
        .map_err(|e| {
            error!("Connection lost during handshake. The server may have rejected the connection (at capacity) or peer matching timed out.");
            e
        })?;
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
        let filename = desired_file.path;
        // Note: Resume is disabled because zstd compression breaks byte-offset resume.
        // Files always transfer from the beginning.
        let file = match OpenOptions::new().write(true).open(&filename).await {
            Ok(file) => {
                warn!(
                    file = ?filename,
                    "File already exists, overwriting (resume disabled with compression)"
                );
                // Truncate the file to start fresh
                file.set_len(0).await?;
                file
            }
            Err(_) => File::create(&filename).await?,
        };
        info!(
            file = ?filename,
            size = desired_file.size,
            "Preparing to receive file"
        );
        // Always start from 0 (no resume with compression)
        let std_file_handle = StdFileHandle::new(
            desired_file.id,
            filename,
            file,
            0, // Always start from beginning
            desired_file.size,
        )
        .await?;
        v.push(std_file_handle)
    }
    return Ok(v);
}
