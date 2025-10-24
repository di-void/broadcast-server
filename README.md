# Broadcast Server

A small experimental WebSocket broadcast server CLI program written in Rust. The server accepts WebSocket connections and broadcasts text messages received from one client to all other connected clients.

This repository currently contains only the server module. A separate client implementation is planned for later. You can test the server now using a CLI WebSocket client such as `wscat`.

## What this project is

- Language: Rust
- Runtime: smol
- WebSocket library: async-tungstenite
- Purpose: simple broadcast server useful for learning or small demos; accepts connections, relays text messages between clients, and gracefully shuts down on Ctrl+C.

## Current status

- Server module implemented and listening on `127.0.0.1:8080` by default.
- Client module: not implemented yet.

## Prerequisites

- Rust toolchain (rustup + cargo). Install via https://rustup.rs/
- Node.js/npm (optional) for `wscat` if you want to use it to test the server.

To install `wscat` (global):

```powershell
npm install -g wscat
```

## Build and run

From the repository root:

```powershell
# build (optional)
cargo build --release

# run in debug (fast compile & run)
cargo run

# or run the release binary
cargo run --release
```

## Testing with wscat

First start the broadcast server with the `start` command:

```powershell
cargo run start
```

Then open two or more terminals and connect to the running server:

```powershell
wscat -c ws://127.0.0.1:8080
```

Type a message in one terminal and press Enter — the text should appear in the other connected terminals (the message sender does not receive its own message back).

Example session:

1. Terminal A: `wscat -c ws://127.0.0.1:8080`
2. Terminal B: `wscat -c ws://127.0.0.1:8080`
3. In A: `hello everyone` -> B receives `hello everyone`

When you press Ctrl+C in the server terminal, the server will attempt a graceful shutdown and notify connected clients with a close frame.

## Troubleshooting

- If you see an error that the address is in use, either stop the process using that port or change the bind address/port in `src/server/mod.rs`.
- If `wscat` is not connecting, verify the server is running and that you used `127.0.0.1` rather than `localhost` (the server binds to the loopback address in the current implementation).

## Next steps (planned)

- Implement a client module in Rust (WebSocket client library).

## Notes for contributors

- The server currently uses `smol` and channels for brokered broadcasting. The registry limits client id to 0..=255 (u8). Consider increasing the id type if you expect many clients.
- See `src/server/mod.rs` for the primary server implementation and `src/main.rs` / `src/cli.rs` for how the server is started.
