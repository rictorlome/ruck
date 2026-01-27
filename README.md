# ruck-relay

`ruck-relay` is a command line tool for hosting relay servers and sending end-to-end encrypted files between clients. It was heavily inspired by [croc](https://github.com/schollz/croc), one of the easiest ways to send files between peers.

A public relay server is available at `174.138.70.74:8080` (the default), so send and receive commands work without additional configuration.

## Installation

```bash
cargo install ruck-relay
```

## Usage

```bash
# Send a file (generates a password automatically)
ruck-relay send file.txt

# Send with a custom password
ruck-relay send --password mysecret file.txt

# Send to a different relay server
ruck-relay send --relay myserver.com:8080 file.txt

# Receive a file
ruck-relay receive <password>

# Receive from a different relay server
ruck-relay receive --relay myserver.com:8080 <password>

# Start a relay server
ruck-relay relay

# Start a relay server with custom settings
ruck-relay relay --bind 0.0.0.0:9000 --max-clients 500 --timeout 120
```

## Configuration

The relay server accepts the following options:

| Flag | Default | Description |
|------|---------|-------------|
| `--bind` | `0.0.0.0:8080` | Address to bind to |
| `--max-clients` | `1000` | Maximum pending connections |
| `--timeout` | `300` | Peer matching timeout (seconds) |

## Protocol

### Server

The server in `ruck-relay` exposes a TCP port.
Its only functions are to staple connections and shuttle bytes between stapled connections.
The first 32 bytes sent over the wire from a new client are used as its unique identifier.
When a new client joins, if the server has another open connection with the same identifier, the connections are then stapled.
The clients have some mechanism for agreeing on these identifiers, however, from the server's perspective it doesn't matter how they agree.

Once the connection is stapled, all bytes are piped across until a client disconnects or times out.
The time out is set to remove idle connections.
The server does nothing else with the bytes, so the clients are free to end-to-end encrypt their messages.
For this reason, updates to the `ruck-relay` protocol do not typically necessitate server redeployments.

### Client

There are two types of clients - `send` and `receive` clients.
Out of band, the clients agree on a relay server and password, from which they can derive the 32 byte identifier used by the server to staple their connections.
Clients have the option of using the single-use, automatically generated passwords which `ruck-relay` supplies by default.
Using the passwords per the [Spake2](https://docs.rs/spake2/0.3.1/spake2/) handshake algorithm, clients generate a symmetric key with which to encrypt their subsequent messages.
Once the handshake is complete, `send` and `receive` negotiate and exchange files per the following:

- `send` offers a list of files and waits.
- `receive` specifies which bytes it wants from these files.
- `send` sends the specified bytes, then a completion message and hangs up.
- `receive` hangs up once the downloads are complete.
