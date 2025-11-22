use crate::input::{self, ClientUserInput};
use crate::readline_helper;
use shared::logger;
use shared::message::{ChatMessage, ChatMessageError, MessageTypes};
use shared::network::{TcpMessageHandler, MAX_FILE_SIZE};
use std::collections::HashSet;
use std::io;
use std::net::AddrParseError;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use rustls::ClientConfig;
use rustls::pki_types::ServerName;

#[derive(Debug)]
pub enum ChatClientError {
    InvalidAddress,
    IoError,
    ChatMessageError,
}

impl From<AddrParseError> for ChatClientError {
    fn from(_: AddrParseError) -> Self {
        ChatClientError::InvalidAddress
    }
}

impl From<io::Error> for ChatClientError {
    fn from(_: io::Error) -> Self {
        ChatClientError::IoError
    }
}

impl From<ChatMessageError> for ChatClientError {
    fn from(_: ChatMessageError) -> Self {
        ChatClientError::ChatMessageError
    }
}

pub enum ClientStream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for ClientStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            ClientStream::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            ClientStream::Tls(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ClientStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            ClientStream::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            ClientStream::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            ClientStream::Plain(stream) => Pin::new(stream).poll_flush(cx),
            ClientStream::Tls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            ClientStream::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            ClientStream::Tls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

pub struct ChatClient {
    connection: ClientStream,
    server_host: String,
    server_port: u16,
    use_tls: bool,
    chat_name: String,
    last_dm_sender: Option<String>,
    connected_users: Arc<RwLock<HashSet<String>>>,
    was_kicked: bool,
}

impl ChatClient {
    pub async fn new(server_addr: &str, name: String) -> Result<Self, ChatClientError> {
        // Parse address - could be host:port or just host
        let (host, port, use_tls) = Self::parse_server_addr(server_addr)?;

        logger::log_info(&format!("Connecting to {}:{}...", host, port));
        let stream = TcpStream::connect(format!("{}:{}", host, port)).await
            .map_err(|e| {
                logger::log_error(&format!("Failed to connect to {}:{} - {}", host, port, e));
                ChatClientError::IoError
            })?;

        logger::log_success(&format!("TCP connection established to {}:{}", host, port));

        let connection = if use_tls {
            logger::log_info("Establishing TLS connection...");
            let mut root_cert_store = rustls::RootCertStore::empty();
            root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

            let config = ClientConfig::builder()
                .with_root_certificates(root_cert_store)
                .with_no_client_auth();

            let connector = TlsConnector::from(Arc::new(config));
            let server_name = ServerName::try_from(host.clone())
                .map_err(|e| {
                    logger::log_error(&format!("Invalid server name '{}': {:?}", host, e));
                    io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name")
                })?;

            let tls_stream = connector.connect(server_name, stream).await
                .map_err(|e| {
                    logger::log_error(&format!("TLS handshake failed: {}", e));
                    ChatClientError::IoError
                })?;
            logger::log_success("TLS connection established");
            ClientStream::Tls(Box::new(tls_stream))
        } else {
            logger::log_info("Using plain TCP (no encryption)");
            ClientStream::Plain(stream)
        };

        Ok(ChatClient {
            connection,
            server_host: host,
            server_port: port,
            use_tls,
            chat_name: name,
            last_dm_sender: None,
            connected_users: Arc::new(RwLock::new(HashSet::new())),
            was_kicked: false,
        })
    }

    fn parse_server_addr(addr: &str) -> Result<(String, u16, bool), ChatClientError> {
        // Check if address starts with tls://
        let (use_tls, addr) = if let Some(stripped) = addr.strip_prefix("tls://") {
            (true, stripped)
        } else {
            (false, addr)
        };

        // Parse host:port
        if let Some((host, port)) = addr.rsplit_once(':') {
            let port = port.parse::<u16>()
                .map_err(|_| ChatClientError::InvalidAddress)?;
            Ok((host.to_string(), port, use_tls))
        } else {
            // No port specified, use default
            Ok((addr.to_string(), 8080, use_tls))
        }
    }

    pub async fn join_server(&mut self) -> Result<(), ChatClientError> {
        let chat_message =
            ChatMessage::try_new(MessageTypes::Join, Some(self.chat_name.as_bytes().to_vec()))?;
        self.send_message_chunked(chat_message).await?;
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<(), ChatClientError> {
        const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(60);
        const BACKOFF_MULTIPLIER: u32 = 2;

        // Explicitly shutdown the old connection before reconnecting
        let _ = self.connection.shutdown().await;

        // Give the server time to detect the closure and clean up
        sleep(Duration::from_millis(100)).await;

        let mut backoff = INITIAL_BACKOFF;
        let mut attempt = 1;

        loop {
            logger::log_info(&format!(
                "Attempting to reconnect to {}:{} (attempt {})...",
                self.server_host, self.server_port, attempt
            ));

            match TcpStream::connect(format!("{}:{}", self.server_host, self.server_port)).await {
                Ok(stream) => {
                    // Re-establish TLS if needed
                    let connection = if self.use_tls {
                        logger::log_info("Re-establishing TLS connection...");
                        let mut root_cert_store = rustls::RootCertStore::empty();
                        root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

                        let config = ClientConfig::builder()
                            .with_root_certificates(root_cert_store)
                            .with_no_client_auth();

                        let connector = TlsConnector::from(Arc::new(config));
                        let server_name = ServerName::try_from(self.server_host.clone())
                            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name"))?;

                        let tls_stream = connector.connect(server_name, stream).await?;
                        logger::log_success("TLS connection re-established");
                        ClientStream::Tls(Box::new(tls_stream))
                    } else {
                        ClientStream::Plain(stream)
                    };

                    self.connection = connection;
                    logger::log_success("Reconnected to server!");

                    // Rejoin the server with the same username
                    if let Err(e) = self.join_server().await {
                        logger::log_error(&format!("Failed to rejoin server: {:?}", e));
                        return Err(e);
                    }

                    return Ok(());
                }
                Err(e) => {
                    logger::log_warning(&format!(
                        "Reconnection attempt {} failed: {}. Retrying in {:?}...",
                        attempt, e, backoff
                    ));
                    sleep(backoff).await;

                    // Exponential backoff with cap
                    backoff =
                        std::cmp::min(backoff.saturating_mul(BACKOFF_MULTIPLIER), MAX_BACKOFF);
                    attempt += 1;
                }
            }
        }
    }

    fn get_message_content(&self, message: &ChatMessage, msg_type_name: &str) -> Option<String> {
        message.content_as_string().or_else(|| {
            logger::log_error(&format!("Received invalid UTF-8 {} message", msg_type_name));
            None
        })
    }

    async fn handle_message(&mut self, message: ChatMessage) {
        match message.msg_type {
            MessageTypes::Join => {
                if let Some(content) = self.get_message_content(&message, "join") {
                    logger::log_system(&format!("{} has joined the chat", content));
                }
            }
            MessageTypes::Leave => {
                if let Some(content) = self.get_message_content(&message, "leave") {
                    logger::log_system(&format!("{} has left the chat", content));
                }
            }
            MessageTypes::UserRename => {
                if let Some(content) = self.get_message_content(&message, "rename") {
                    logger::log_success(&format!("You have been renamed to '{}'", content));
                    self.chat_name = content;
                }
            }
            MessageTypes::ChatMessage => {
                if let Some(content) = self.get_message_content(&message, "chat") {
                    let should_display = content
                        .split_once(": ")
                        .is_none_or(|(username, _)| username != self.chat_name);

                    if should_display {
                        logger::log_chat(&content);
                    }
                }
            }
            MessageTypes::ListUsers => {
                if let Some(content) = self.get_message_content(&message, "list users") {
                    // Update the connected users list for autocomplete
                    let mut users = self.connected_users.write().unwrap();
                    users.clear();
                    for user in content.lines() {
                        users.insert(user.to_string());
                    }
                    drop(users);

                    logger::log_info("Current users online:");
                    for user in content.lines() {
                        logger::log_info(&format!(" - {}", user));
                    }
                }
            }
            MessageTypes::DirectMessage => {
                if let Some(content) = self.get_message_content(&message, "dm")
                    && let Some((sender, rest)) = content.split_once('|')
                    && let Some((recipient, msg)) = rest.split_once('|')
                {
                    // Only display if we are the recipient (not the sender - we already showed it locally)
                    if recipient == self.chat_name {
                        logger::log_warning(&format!("[DM from {}]: {}", sender, msg));
                        // Track the sender so we can reply with /r
                        self.last_dm_sender = Some(sender.to_string());
                    }
                }
            }
            MessageTypes::Error => {
                if let Some(content) = self.get_message_content(&message, "error") {
                    logger::log_error(&content);
                    // Check if this is a kick message
                    if content.contains("kicked") {
                        self.was_kicked = true;
                    }
                }
            }
            MessageTypes::FileTransfer => {
                self.handle_file_transfer(&message);
            }
            MessageTypes::FileTransferAck => {
                if let Some(content) = self.get_message_content(&message, "file ack") {
                    logger::log_success(&content);
                }
            }
            _ => {
                logger::log_warning(&format!("Unknown message type: {:?}", message.msg_type));
            }
        }
    }

    fn handle_file_transfer(&self, message: &ChatMessage) {
        let content = match message.get_content() {
            Some(c) => c,
            None => {
                logger::log_error("Received empty file transfer");
                return;
            }
        };

        // Parse binary format: recipient_len(1)|recipient|sender_len(1)|sender|filename_len(1)|filename|filedata
        if content.len() < 2 {
            logger::log_error("Invalid file transfer format");
            return;
        }

        // First extract recipient to check if this file is for us
        let recipient_len = content[0] as usize;
        if content.len() < 1 + recipient_len + 1 {
            logger::log_error("Invalid file transfer format");
            return;
        }

        let recipient = match std::str::from_utf8(&content[1..1 + recipient_len]) {
            Ok(s) => s,
            Err(_) => {
                logger::log_error("Invalid recipient name in file transfer");
                return;
            }
        };

        // Check if this file is for us
        if recipient != self.chat_name {
            return; // Not for us, ignore
        }

        // Now extract sender
        let sender_start = 1 + recipient_len;
        let sender_len = content[sender_start] as usize;
        if content.len() < sender_start + 1 + sender_len + 1 {
            logger::log_error("Invalid file transfer format");
            return;
        }

        let sender = match std::str::from_utf8(&content[sender_start + 1..sender_start + 1 + sender_len]) {
            Ok(s) => s,
            Err(_) => {
                logger::log_error("Invalid sender name in file transfer");
                return;
            }
        };

        // Extract filename
        let filename_len_pos = sender_start + 1 + sender_len;
        let filename_len = content[filename_len_pos] as usize;
        let filename_start = filename_len_pos + 1;
        if content.len() < filename_start + filename_len {
            logger::log_error("Invalid file transfer format");
            return;
        }

        let filename = match std::str::from_utf8(&content[filename_start..filename_start + filename_len]) {
            Ok(s) => s,
            Err(_) => {
                logger::log_error("Invalid filename in file transfer");
                return;
            }
        };

        let file_data = &content[filename_start + filename_len..];

        logger::log_warning(&format!(
            "[FILE from {}]: '{}' ({} bytes)",
            sender,
            filename,
            file_data.len()
        ));

        // Save file to downloads directory or current directory
        let save_path = format!("downloads/{}", filename);

        // Create downloads directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all("downloads") {
            logger::log_error(&format!("Failed to create downloads directory: {}", e));
            return;
        }

        match std::fs::write(&save_path, file_data) {
            Ok(_) => {
                logger::log_success(&format!("File saved to: {}", save_path));
            }
            Err(e) => {
                logger::log_error(&format!("Failed to save file: {}", e));
            }
        }
    }

    async fn handle_user_input(
        &mut self,
        user_input: input::ClientUserInput,
    ) -> Result<(), ChatClientError> {
        match user_input {
            input::ClientUserInput::Message(msg) => {
                if msg.trim().is_empty() {
                    return Ok(());
                }
                // Display locally immediately
                let display_msg = format!("{}: {}", self.chat_name, msg);
                logger::log_chat(&display_msg);

                let message =
                    ChatMessage::try_new(MessageTypes::ChatMessage, Some(msg.into_bytes()))?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::DirectMessage {
                recipient,
                message: msg,
            } => {
                if msg.trim().is_empty() {
                    return Ok(());
                }
                // Display DM locally immediately
                logger::log_info(&format!("[DM to {}]: {}", recipient, msg));

                let dm_content = format!("{}|{}", recipient, msg);
                let message = ChatMessage::try_new(
                    MessageTypes::DirectMessage,
                    Some(dm_content.into_bytes()),
                )?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::Reply(msg) => {
                if msg.trim().is_empty() {
                    return Ok(());
                }
                if let Some(recipient) = &self.last_dm_sender {
                    // Display reply locally immediately
                    logger::log_info(&format!("[DM to {}]: {}", recipient, msg));

                    let dm_content = format!("{}|{}", recipient, msg);
                    let message = ChatMessage::try_new(
                        MessageTypes::DirectMessage,
                        Some(dm_content.into_bytes()),
                    )?;
                    self.send_message_chunked(message).await?;
                    Ok(())
                } else {
                    logger::log_error("No one to reply to. Use /dm <username> <message> first.");
                    Ok(())
                }
            }
            input::ClientUserInput::Help => {
                logger::log_info("Available commands:");
                logger::log_info("  /help - Show this help message");
                logger::log_info("  /list - List all users");
                logger::log_info("  /dm <username> <message> - Send direct message");
                logger::log_info("  /r <message> - Reply to last direct message");
                logger::log_info("  /send <username> <filepath> - Send a file (max 10MB)");
                logger::log_info("  /rename <new_name> - Change your username");
                logger::log_info("  /quit - Exit the chat");
                Ok(())
            }
            input::ClientUserInput::ListUsers => {
                let message = ChatMessage::try_new(MessageTypes::ListUsers, None)?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::Rename(new_name) => {
                let message = ChatMessage::try_new(
                    MessageTypes::RenameRequest,
                    Some(new_name.into_bytes()),
                )?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::SendFile { recipient, file_path } => {
                self.send_file(&recipient, &file_path).await
            }
            input::ClientUserInput::Quit => Ok(()),
        }
    }

    async fn send_file(&mut self, recipient: &str, file_path: &str) -> Result<(), ChatClientError> {
        let path = Path::new(file_path);

        // Check if file exists
        if !path.exists() {
            logger::log_error(&format!("File not found: {}", file_path));
            return Ok(());
        }

        // Get file name
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Read file
        let file_data = match std::fs::read(path) {
            Ok(data) => data,
            Err(e) => {
                logger::log_error(&format!("Failed to read file: {}", e));
                return Ok(());
            }
        };

        // Check file size (10MB limit, minus some overhead for metadata)
        let max_content_size = MAX_FILE_SIZE - 1024; // Leave room for headers
        if file_data.len() > max_content_size {
            logger::log_error(&format!(
                "File too large: {} bytes (max {} bytes / ~10MB)",
                file_data.len(),
                max_content_size
            ));
            return Ok(());
        }

        logger::log_info(&format!(
            "Sending file '{}' ({} bytes) to {}...",
            file_name,
            file_data.len(),
            recipient
        ));

        // Build file transfer message: recipient|filename|filedata
        // We use a binary format: recipient_len(1)|recipient|filename_len(1)|filename|filedata
        let mut content = Vec::new();
        content.push(recipient.len() as u8);
        content.extend_from_slice(recipient.as_bytes());
        content.push(file_name.len() as u8);
        content.extend_from_slice(file_name.as_bytes());
        content.extend_from_slice(&file_data);

        let message = ChatMessage::try_new(MessageTypes::FileTransfer, Some(content))?;
        self.send_message_chunked(message).await?;

        logger::log_success(&format!("File '{}' sent to {}", file_name, recipient));
        Ok(())
    }

    pub async fn run(&mut self) -> io::Result<()> {
        // Spawn readline handler in a blocking thread with username as prompt
        let mut readline_rx = readline_helper::spawn_readline_handler(
            self.connected_users.clone(),
            self.chat_name.clone(),
        );

        loop {
            tokio::select! {
                result = self.read_message_chunked() => {
                    match result {
                        Ok(message) => {
                            self.handle_message(message).await;
                        }
                        Err(shared::network::TcpMessageHandlerError::IoError(_)) |
                        Err(shared::network::TcpMessageHandlerError::Disconnect) => {
                            logger::log_warning("Disconnected from server");

                            // Don't reconnect if we were kicked
                            if self.was_kicked {
                                logger::log_info("Not reconnecting - you were kicked from the server");
                                return Ok(());
                            }

                            // Attempt to reconnect with exponential backoff
                            match self.reconnect().await {
                                Ok(()) => {
                                    // Connection restored
                                }
                                Err(e) => {
                                    logger::log_error(&format!("Failed to reconnect: {:?}", e));
                                    return Err(io::Error::other("Reconnection failed"));
                                }
                            }
                        }
                    }
                }
                Some(line) = readline_rx.recv() => {
                    match line {
                        Some(input_line) => {
                            match ClientUserInput::try_from(input_line.as_str()) {
                                Ok(input::ClientUserInput::Quit) => return Ok(()),
                                Ok(input::ClientUserInput::ListUsers) => {
                                    let message = ChatMessage::try_new(MessageTypes::ListUsers, None)
                                        .map_err(|e| io::Error::other(format!("Failed to create ListUsers message: {e:?}")))?;
                                    self.send_message_chunked(message).await
                                        .map_err(|e| io::Error::other(format!("Failed to send ListUsers message: {e:?}")))?;
                                }
                                Ok(user_input) => {
                                    if let Err(e) = self.handle_user_input(user_input).await {
                                        logger::log_error(&format!("Error: {e:?}"));
                                    }
                                }
                                Err(e) => {
                                    logger::log_error(&format!("Input error: {e:?}"));
                                }
                            }
                        }
                        None => {
                            // EOF or error from readline
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

impl TcpMessageHandler for ChatClient {
    type Stream = ClientStream;
    fn get_stream(&mut self) -> &mut Self::Stream {
        &mut self.connection
    }
}
