# rust_chat

A modern, colorful terminal-based chat application written in Rust with async/await support.

![Usage](usage.png)

## Features

- 🎨 **Colorized Output** - Beautiful, color-coded terminal interface with timestamps
- 👥 **Multi-user Support** - Multiple clients can connect simultaneously
- 🔄 **Real-time Messaging** - Instant message broadcasting to all connected users
- 💬 **Direct Messaging** - Send private messages to specific users with `/dm` and `/r` commands
- 🏷️ **Username Colorization** - Each user gets a unique, consistent color
- ⚡ **Async I/O** - Built on Tokio for high-performance async networking
- 🔧 **Modular Architecture** - Clean separation between client, server, and shared code
- 🛡️ **Smart Username Handling** - Automatic renaming for duplicate usernames
- 🔁 **Auto-Reconnect** - Exponential backoff reconnection when server goes down
- 🔒 **Security Hardened** - Rate limiting, input validation, connection limits, and memory safety
- 📊 **Rich Logging** - Categorized logs (INFO, ERROR, WARN, OK, SYSTEM, CHAT)

## Architecture

The project is organized into three crates:

- **chat_client** - Terminal client application
- **chat_server** - Multi-threaded chat server
- **chat_shared** - Shared code (message protocol, networking, logging)

## Requirements

- Rust 1.75+ (edition 2024)
- Cargo

## Quick Start

### Starting the Server

```bash
cargo run --bin chat_server
```

The server will start on `0.0.0.0:8080` by default.

#### Server Configuration

Configure the server using environment variables:

```bash
# Custom address and port
CHAT_SERVER_ADDR="127.0.0.1:9000" cargo run --bin chat_server

# Custom max clients
CHAT_SERVER_MAX_CLIENTS="50" cargo run --bin chat_server
```

### Starting the Client

```bash
cargo run --bin chat_client
```

You'll be prompted to enter:
1. **Server address** (default: `127.0.0.1:8080`)
2. **Username** (default: `Guest`)

## Usage

### Chat Commands

Once connected, you can use the following commands:

- `/help` -             Display available commands
- `/quit` -             Exit the chat application
- `/list` -             List users
- `/dm <USERNAME>` -    Direct Message a User
- `/r` -                Reply to a DM
- Any other text - Send a message to all connected users

### Example Session

```
[12:34:56] [INFO] Enter Chat Server (default: 127.0.0.1:8080):
[12:34:58] [INFO] Enter Chat Name (default: Guest):
Alice
[12:34:59] [SYSTEM] Alice has joined the chat
Alice █

[12:35:02] [SYSTEM] Bob has joined the chat
Alice hello everyone!
[12:35:05] [CHAT] Bob: hi Alice!
```

## Project Structure

```
rust_chat/
├── chat_client/
│   └── src/
│       ├── main.rs          # Entry point and setup
│       ├── client.rs        # Client logic and message handling
│       └── input.rs         # User input processing
├── chat_server/
│   └── src/
│       ├── main.rs          # Server entry point
│       └── user_connection.rs # Connection handling
└── chat_shared/
    └── src/
        ├── lib.rs           # Module exports
        ├── logger.rs        # Colorized logging utilities
        ├── message.rs       # Message protocol
        └── network.rs       # TCP message handling
```

## Features in Detail

### Colorized Logging

All output is color-coded by category:
- **INFO** (Cyan) - General information
- **OK** (Green) - Success messages
- **ERROR** (Red) - Error messages
- **WARN** (Yellow) - Warnings
- **SYSTEM** (Magenta) - User join/leave notifications
- **CHAT** (White) - Chat messages with colored usernames

### Username Colorization

Each username is assigned a consistent color using hash-based selection from 12 vibrant colors. The same username always appears in the same color, making it easy to follow conversations.

### Smart Username Handling

If you try to join with a username that's already taken, the server automatically appends a random 4-digit suffix (e.g., `Alice_1234`).

### Auto-Reconnect with Exponential Backoff

If the connection to the server is lost, the client automatically attempts to reconnect with exponential backoff:
- **Initial delay**: 1 second
- **Maximum delay**: 60 seconds
- **Strategy**: Doubles the wait time after each failed attempt (1s → 2s → 4s → 8s → 16s → 32s → 60s)
- **Preservation**: Your username and last DM sender are preserved across reconnections
- **Auto-rejoin**: Automatically rejoins the server with the same username when reconnected

Example reconnection sequence:
```
Disconnected from server
Attempting to reconnect to 127.0.0.1:8080 (attempt 1)...
Reconnection attempt 1 failed: Connection refused. Retrying in 1s...
Attempting to reconnect to 127.0.0.1:8080 (attempt 2)...
Reconnection attempt 2 failed: Connection refused. Retrying in 2s...
...
Attempting to reconnect to 127.0.0.1:8080 (attempt 5)...
Reconnected to server!
Alice has joined the chat
```

### Direct Messaging

Send private messages to specific users:
- **Send a DM**: `/dm <username> <message>` - Send a direct message to a specific user
- **Reply to DM**: `/r <message>` - Quick reply to the last person who sent you a DM
- **Privacy**: The server logs that DMs are happening but doesn't display the message content
- **Validation**: Server validates that the recipient exists before sending

### Security Features

The application implements comprehensive security measures to protect against common network attacks:

#### Input Validation
- **Username Validation**:
  - Maximum length: 32 characters
  - Allowed characters: alphanumeric, underscore, and hyphen only
  - Empty usernames rejected
- **Message Validation**:
  - Maximum message size: 8KB (prevents memory exhaustion)
  - Maximum content length: 1KB per message
  - Empty messages blocked (client and server-side)
  - Integer overflow protection with safe type conversion

#### Rate Limiting
- **Token Bucket Algorithm**: 10 messages per second per connection
- **Auto-refill**: Resets every second
- **Smart Filtering**: Join messages excluded from rate limits
- **User Feedback**: Clients receive "Rate limit exceeded" errors
- **Protection Against**: Spam floods, DoS attacks, message bombing

#### Connection Management
- **Connection Limits**: Configurable max clients (default: 100)
- **Enforcement**: Server rejects new connections when at capacity
- **Atomic Tracking**: Thread-safe connection counting
- **Auto-cleanup**: Connections automatically decremented on disconnect
- **Graceful Handling**: Proper cleanup on all disconnect scenarios

#### Memory Safety
- **Zero `unsafe` Code**: Entire codebase is memory-safe Rust
- **No `.unwrap()` Panics**: All error paths use safe `Result` propagation
- **Bounded Allocations**: All memory allocations are size-limited
- **Overflow Protection**: Integer conversions use `try_from()` for safety

#### Network Security
- **Message Size Limits**: 8KB maximum per message
- **Chunked Protocol**: Supports large messages without blocking
- **Acknowledgment System**: "OK" handshake prevents desynchronization
- **Clean Disconnects**: Explicit connection shutdown before reconnect
- **Backpressure Handling**: Broadcast channel sized for burst traffic

#### Error Handling
- **Validated Inputs**: All user inputs are validated before processing
- **Error Messages**: Clear feedback sent to clients for invalid operations
- **Logging**: Security events logged with warnings
- **Graceful Degradation**: Invalid requests don't crash the server

#### Security Metrics

| Security Feature | Implementation |
|-----------------|----------------|
| Max Message Size | 8KB |
| Max Username Length | 32 characters |
| Max Message Content | 1KB |
| Rate Limit | 10 messages/second |
| Connection Limit | Configurable (default: 100) |
| Memory Safety | 100% safe Rust |
| Input Validation | Comprehensive |

**Note**: For production deployment, consider adding TLS encryption, authentication, and E2E encryption for enhanced security.

### Message Protocol

Messages are sent over TCP with a custom chunked protocol that supports:
- Join notifications
- Leave notifications
- Chat messages
- Direct messages
- Username renames
- User list requests
- Error messages

## Building

### Development Build

```bash
cargo build
```

### Release Build

```bash
cargo build --release
```

### Running Tests

```bash
cargo test
```

### Code Quality

```bash
# Run clippy
cargo clippy --all-targets --all-features

# Run with strict warnings
cargo clippy --all-targets --all-features -- -D warnings
```

## Dependencies

### Core
- **tokio** - Async runtime
- **colored** - Terminal colors
- **chrono** - Timestamp formatting

### Server-specific
- **rand** - Random username generation

## Contributing

Contributions are welcome! Please ensure:
1. Code passes `cargo clippy` with no warnings
2. Code is properly formatted with `cargo fmt`
3. All tests pass with `cargo test`

## License

This project is available for educational and personal use.

## Future Enhancements

- [ ] TLS/SSL encryption for network traffic
- [ ] End-to-end encryption for direct messages
- [ ] User authentication system
- [ ] Chat rooms/channels
- [ ] Message history and persistence
- [ ] File sharing capabilities
- [ ] Read timeouts for slowloris protection
- [ ] GUI client
