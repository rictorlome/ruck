use crate::message::Message;

use futures::prelude::*;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

pub async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:8080".to_string();
    let server = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);
    loop {
        let (stream, _) = server.accept().await?;
        tokio::spawn(async move {
            match process(stream).await {
                Ok(_) => println!("ok"),
                Err(_) => println!("err"),
            }
        });
    }
}

pub async fn process(socket: TcpStream) -> Result<(), Box<dyn std::error::Error>> {
    let length_delimited = FramedRead::new(socket, LengthDelimitedCodec::new());
    let mut stream = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        tokio_serde::formats::SymmetricalBincode::<Message>::default(),
    );
    while let Some(message) = stream.try_next().await? {
        println!("GOT: {:?}", message);
    }
    Ok(())
}
