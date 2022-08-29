# Protocol

`ruck` is a command line tool used for hosting relay servers and sending end-to-end encrypted files between clients. This document describes the protocol `ruck` uses to support this functionality.

### Version

This document refers to version `0.1.0` of `ruck` as defined by the `Cargo.toml` file.

## Server

The server in `ruck` exposes a TCP port, typically port `8080`. Its only functions are to staple connections and shuttle bytes between stapled connections. The first 32 bytes sent from a new client are stored in a HashMap. If the same 32 bytes are already in the Hashmap, the connections are then stapled. This 32 byte key is defined by the [Spake2](https://docs.rs/spake2/0.3.1/spake2/) handshake algorithm which the clients employ to negotiate a single use password to encrypt all their messages. Although from the server's perspective, the clients can agree on these 32 bytes in any way.

Once the connection is stapled, all bytes are piped across until a client disconnects or times out. The time out is set to remove idle connections. Beyond stapling connections, the file negotiation aspect of the protocol is managed by the clients. For this reason, `ruck` servers are very resistant to updates and protocol updates typically do not necessitate new deployments.

The server does nothing else with the bytes, so the clients are free to end-to-end encrypt their messages, as long as the first 32 bytes sent over the wire match. Other than that, it is a private echo server.

## Client

There are two types of clients - `send` and `receive` clients. The following state machine describes the protocol. All the messages after the exchange of passwords are typically bzip compressed, encrypted with Aes256Gcm using a Spake2 key derived from the exchanged password. They are sent over the wire as bincode. Each message has a fixed size of 1024 \* 1024 bytes.

Message Types:

- Vec<File Info>
-

- Set a timeout for new messages.

Send or receive.

If send:

- Send message with file info

They exchange passwords.
Send offers a list of files.
Receive specifies which bytes it wants from these files.
Send sends the specified bytes and waits.
Receive sends heartbeats with progress updates.
Send hangs up once the heartbeats stop or received a successful heartbeat.
