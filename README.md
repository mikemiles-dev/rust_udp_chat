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
- 🔐 **Native TLS Support** - Built-in TLS encryption with Let's Encrypt certificates
- 📊 **Rich Logging** - Categorized logs (INFO, ERROR, WARN, OK, SYSTEM, CHAT)
- 📝 **Command History** - Full readline support with persistent command history (up to 1000 commands)
- ⌨️ **Tab Completion** - Smart autocomplete for commands and usernames
- 💡 **Inline Hints** - Visual hints showing available completions as you type
- 🚀 **Production Ready** - Docker and native systemd deployment options
- 👮 **Admin Commands** - Server-side `/kick` and user management

## Architecture

The project is organized into three crates:

- **client** - Terminal client application
- **server** - Multi-threaded chat server
- **shared** - Shared code (message protocol, networking, logging)

## Requirements

- Rust 1.75+ (edition 2024)
- Cargo

## Quick Start

### Using Docker (Easiest)

Connect to the live server instantly with Docker:

```bash
# Interactive mode (will prompt for username)
docker run -it --rm ghcr.io/mikemiles-dev/rust_chat-client:latest

# Or specify everything upfront
docker run -it --rm \
  -e CHAT_SERVER=tls://milesrust.chat:8443 \
  -e CHAT_USERNAME=YourName \
  ghcr.io/mikemiles-dev/rust_chat-client:latest
```

No installation required! Docker will automatically pull the latest client image.

### Local Development

#### Starting the Server

```bash
cargo run --bin server
```

The server will start on `0.0.0.0:8080` by default.

You can use server commands:
```
/help       # Show available commands
/list       # List connected users
/kick USER  # Kick a user
/quit       # Shutdown server
```

#### Server Configuration

Configure the server using environment variables:

```bash
# Custom address and port
CHAT_SERVER_ADDR="127.0.0.1:9000" cargo run --bin server

# Custom max clients
CHAT_SERVER_MAX_CLIENTS="50" cargo run --bin server
```

#### Starting the Client

**Option 1: Connect to Live Demo Server**

Try out the chat by connecting to the live instance:

```bash
cargo run --bin client
# When prompted:
# Server: tls://milesrust.chat:8443
# Username: YourName
```

Or use environment variables:

```bash
CHAT_SERVER="tls://milesrust.chat:8443" CHAT_USERNAME="YourName" cargo run --bin client
```

**Option 2: Local Development**

```bash
cargo run --bin client
```

You'll be prompted to enter:
1. **Server address** (default: `127.0.0.1:8080`)
2. **Username** (default: `Guest`)

Alternatively, you can use environment variables to skip the prompts:

```bash
# Set both server and username
CHAT_SERVER="127.0.0.1:8080" CHAT_USERNAME="Alice" cargo run --bin client

# Set only server (will prompt for username)
CHAT_SERVER="chat.example.com:443" cargo run --bin client

# Set only username (will prompt for server)
CHAT_USERNAME="Bob" cargo run --bin client
```

### Production Deployment

For production deployment with TLS encryption:

- **[deploy/digital_ocean/](deploy/digital_ocean/)** - Deploy on Digital Ocean with tmux + native TLS

Complete setup scripts and documentation included.

## Usage

### Client Commands

Once connected to the server, clients can use the following commands:

- `/help` - Display available commands
- `/quit` - Exit the chat application
- `/list` - List all connected users
- `/dm <USERNAME> <MESSAGE>` - Send a direct message to a specific user
- `/r <MESSAGE>` - Reply to the last user who sent you a DM
- Any other text - Send a message to all connected users

### Server Commands

While the server is running, administrators can use these commands:

- `/help` or `/h` - Display available server commands
- `/list` - Show all currently connected users with count
- `/kick <username>` - Kick a user from the server
- `/quit` or `/q` - Gracefully shutdown the server

### Command History & Autocomplete

Both client and server support advanced input features powered by rustyline:

**Command History:**
- **↑/↓ arrows** - Navigate through previous commands
- **Ctrl+R** - Reverse search through history
- **Persistent** - Up to 1000 commands stored per session

**Tab Completion:**
- **Client:** Type `/` then press `TAB` to see all commands
- **Client:** Type `/dm ` then press `TAB` to autocomplete usernames
- **Server:** Type `/` then press `TAB` to see all server commands
- **Smart filtering** - Only shows matching completions

**Visual Hints:**
- Inline gray text shows possible completions as you type
- Multiple matches display all options

**Example:**
```bash
# Type "/h" and see hint showing "elp"
/h[elp]

# Press TAB to complete
/help

# Type "/dm A" and press TAB to see users starting with A
/dm Alice

# Press ↑ to repeat last command
```

### Example Client Session

```
[12:34:56] [INFO] Enter Chat Server (default: 127.0.0.1:8080):
[12:34:58] [INFO] Enter Chat Name (default: Guest):
Alice
[12:34:59] [SYSTEM] Alice has joined the chat
Alice █

[12:35:02] [SYSTEM] Bob has joined the chat
Alice hello everyone!
[12:35:05] [CHAT] Bob: hi Alice!
Alice /dm Bob Hey, want to chat privately?
[12:35:10] [DM] from Bob: Sure thing!
Alice /r Perfect! Let's discuss the project.
```

### Example Server Session

```
[12:34:50] [OK] Chat Server started at 0.0.0.0:8080
[12:34:50] [INFO] To change address, set CHAT_SERVER_ADDR environment variable
[12:34:50] [INFO] To change max clients, set CHAT_SERVER_MAX_CLIENTS environment variable
[12:34:50] [INFO] Server commands: /help, /list, /quit
[12:34:59] [SYSTEM] Alice has joined the chat
[12:35:02] [SYSTEM] Bob has joined the chat
/list
[12:35:15] [INFO] Connected users (2):
[12:35:15] [INFO]   - Alice
[12:35:15] [INFO]   - Bob
[12:35:30] [SYSTEM] Charlie has joined the chat
/list
[12:35:35] [INFO] Connected users (3):
[12:35:35] [INFO]   - Alice
[12:35:35] [INFO]   - Bob
[12:35:35] [INFO]   - Charlie
/kick Bob
[12:35:45] [WARN] Kicking user: Bob
[12:35:45] [SYSTEM] Bob has left the chat
/quit
[12:35:50] [INFO] Server shutting down...
```

## Project Structure

```
rust_chat/
├── client/
│   └── src/
│       ├── main.rs          # Entry point and setup
│       ├── client.rs        # Client logic and message handling
│       ├── input.rs         # Client command processing
│       ├── completer.rs     # Tab completion for commands & usernames
│       └── readline_helper.rs # Rustyline integration with async
├── server/
│   └── src/
│       ├── main.rs          # Server entry point and command handling
│       ├── input.rs         # Server command processing
│       ├── completer.rs     # Tab completion for server commands
│       ├── readline_helper.rs # Rustyline integration with async
│       └── user_connection/
│           ├── mod.rs       # UserConnection struct and event loop
│           ├── error.rs     # Error types and Display impl
│           ├── handlers.rs  # Message processing logic
│           └── rate_limiting.rs # Token bucket rate limiter
├── shared/
│   └── src/
│       ├── lib.rs           # Module exports
│       ├── input.rs         # Shared UserInput trait
│       ├── logger.rs        # Colorized logging utilities
│       ├── message.rs       # Message protocol
│       └── network.rs       # TCP message handling
└── deploy/
    └── digital_ocean/
        ├── setup-certificates.sh # Get Let's Encrypt TLS certificates
        ├── start-server.sh      # Start server in tmux with TLS
        ├── README.md            # Complete Digital Ocean guide
        └── QUICK_START.md       # Quick reference
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

### Command History & Tab Completion

Powered by `rustyline`, both client and server feature a rich command-line experience:

**Command History Features:**
- **Navigation**: Use ↑/↓ arrow keys to browse through command history
- **Reverse Search**: Press Ctrl+R to search backwards through history
- **Persistent Storage**: Up to 1,000 commands remembered per session
- **Auto-add**: Commands are automatically added to history after execution

**Tab Completion Features:**
- **Command Completion**: Press TAB after typing `/` to see all available commands
- **Username Completion** (Client only): Type `/dm ` and press TAB to autocomplete usernames from connected users
- **Smart Filtering**: Completions filter based on what you've already typed
- **Multiple Matches**: Shows all matching options when ambiguous

**Visual Hints:**
- Inline gray text appears as you type, showing the most likely completion
- Helps you discover commands without referring to documentation

**Implementation Details:**
- Runs in a separate blocking thread to maintain async performance
- Communicates with async runtime via `tokio::sync::mpsc` channels
- Client tracks connected users list for username autocomplete
- Updates dynamically when `/list` command is executed

Example interaction:
```bash
# Type partial command
$ /h
  ↳ [gray hint: "elp" shown]

# Press TAB
$ /help

# Type /dm and TAB to see all users
$ /dm [TAB]
  Alice  Bob  Charlie

# Type first letter and TAB
$ /dm A[TAB]
$ /dm Alice
```

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

**Note**: For production deployment, TLS encryption is built-in. Consider adding authentication and E2E encryption for enhanced security.

### Message Protocol

Messages are sent over TCP with a custom chunked protocol that supports:
- Join notifications
- Leave notifications
- Chat messages
- Direct messages
- Username renames
- User list requests
- Error messages

## Docker Image

The client is available as a Docker image for easy deployment and testing:

### Image Information

- **Registry**: GitHub Container Registry (ghcr.io)
- **Image**: `ghcr.io/michaelmileusnich/rust_chat-client`
- **Tags**: `latest`, version tags (e.g., `v1.0.0`), branch tags
- **Platforms**: linux/amd64, linux/arm64

### Usage Examples

```bash
# Connect to live server
docker run -it --rm \
  -e CHAT_SERVER=tls://milesrust.chat:8443 \
  -e CHAT_USERNAME=Alice \
  ghcr.io/michaelmileusnich/rust_chat-client:latest

# Connect to local server
docker run -it --rm \
  -e CHAT_SERVER=127.0.0.1:8080 \
  -e CHAT_USERNAME=Bob \
  ghcr.io/michaelmileusnich/rust_chat-client:latest

# Interactive mode (prompts for server and username)
docker run -it --rm ghcr.io/michaelmileusnich/rust_chat-client:latest

# Use specific version
docker run -it --rm ghcr.io/michaelmileusnich/rust_chat-client:v1.0.0
```

### Building Your Own Image

```bash
# Build the client image
docker build -f client/Dockerfile -t rust-chat-client .

# Run your local build
docker run -it --rm rust-chat-client
```

The image is automatically built and published via GitHub Actions on every push to main.

## Building from Source

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

## Production Deployment

Deploy to Digital Ocean with native TLS encryption:

**[deploy/digital_ocean/](deploy/digital_ocean/)** - Complete deployment guide

### Features

- ✅ Interactive setup script for Let's Encrypt certificates
- ✅ Native TLS encryption (no reverse proxy needed)
- ✅ Server management via tmux
- ✅ Interactive server commands (`/kick`, `/list`, etc.)
- ✅ Auto-renewal for certificates
- ✅ Runs on port 8443

### Quick Start

See **[deploy/digital_ocean/QUICK_START.md](deploy/digital_ocean/QUICK_START.md)** for rapid deployment or **[deploy/digital_ocean/README.md](deploy/digital_ocean/README.md)** for complete documentation.

## Dependencies

### Core
- **tokio** - Async runtime with full features
- **colored** - Terminal colors for output
- **chrono** - Timestamp formatting
- **rustyline** - Readline-like library for command history and tab completion

### Server-specific
- **rand** - Random username generation for collision handling
- **tokio-rustls** - Native TLS implementation
- **rustls** - Modern TLS library
- **rustls-pemfile** - PEM certificate parsing

### Client-specific
- **webpki-roots** - Mozilla's root certificates for TLS validation

### Deployment
- **Certbot** - Let's Encrypt certificate management
- **tmux** - Terminal multiplexer for server management
- **Docker** - Optional containerized client deployment

## Contributing

Contributions are welcome! Please ensure:
1. Code passes `cargo clippy` with no warnings
2. Code is properly formatted with `cargo fmt`
3. All tests pass with `cargo test`

## License

This project is available for educational and personal use.

## Future Enhancements

- [x] **TLS/SSL encryption** - ✅ Implemented with native TLS (tokio-rustls)
- [ ] End-to-end encryption for direct messages
- [ ] User authentication system
- [ ] Chat rooms/channels
- [ ] Message history and persistence
- [ ] File sharing capabilities
- [ ] Read timeouts for slowloris protection
- [ ] GUI client
