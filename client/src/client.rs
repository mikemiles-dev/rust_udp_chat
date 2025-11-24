use crate::input::{self, ClientUserInput};
use crate::readline_helper;
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use shared::commands::client as commands;
use shared::logger;
use shared::message::{ChatMessage, ChatMessageError, MessageTypes};
use shared::network::{MAX_FILE_SIZE, TcpMessageHandler};
use shared::version::VERSION;
use std::collections::{HashMap, HashSet};
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
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use uuid::Uuid;

/// Pending file transfer request (for senders waiting for acceptance)
#[derive(Debug, Clone)]
pub struct PendingOutgoingTransfer {
    pub recipient: String,
    pub file_path: String,
    pub file_name: String,
    #[allow(dead_code)]
    pub file_size: usize,
}

/// Pending file transfer request (for receivers)
#[derive(Debug, Clone)]
pub struct PendingIncomingTransfer {
    #[allow(dead_code)]
    pub sender: String,
    pub file_name: String,
    #[allow(dead_code)]
    pub file_size: usize,
}

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
    /// Session token used to identify reconnecting clients and reclaim ghost sessions
    session_token: String,
    last_dm_sender: Option<String>,
    connected_users: Arc<RwLock<HashSet<String>>>,
    was_kicked: bool,
    current_status: Option<String>,
    /// Pending outgoing transfers (keyed by recipient name)
    pending_outgoing: HashMap<String, PendingOutgoingTransfer>,
    /// Pending incoming transfers (keyed by sender name)
    pending_incoming: HashMap<String, PendingIncomingTransfer>,
}

impl ChatClient {
    pub async fn new(server_addr: &str, name: String) -> Result<Self, ChatClientError> {
        // Parse address - could be host:port or just host
        let (host, port, use_tls) = Self::parse_server_addr(server_addr)?;

        logger::log_info(&format!("Connecting to {}:{}...", host, port));
        let stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
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
            let server_name = ServerName::try_from(host.clone()).map_err(|e| {
                logger::log_error(&format!("Invalid server name '{}': {:?}", host, e));
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name")
            })?;

            let tls_stream = connector.connect(server_name, stream).await.map_err(|e| {
                logger::log_error(&format!("TLS handshake failed: {}", e));
                ChatClientError::IoError
            })?;
            logger::log_success("TLS connection established");
            ClientStream::Tls(Box::new(tls_stream))
        } else {
            logger::log_info("Using plain TCP (no encryption)");
            ClientStream::Plain(stream)
        };

        // Generate a unique session token for this client session
        // This token is used to reclaim a ghost session on reconnection
        let session_token = Uuid::new_v4().to_string();

        Ok(ChatClient {
            connection,
            server_host: host,
            server_port: port,
            use_tls,
            chat_name: name,
            session_token,
            last_dm_sender: None,
            connected_users: Arc::new(RwLock::new(HashSet::new())),
            was_kicked: false,
            current_status: None,
            pending_outgoing: HashMap::new(),
            pending_incoming: HashMap::new(),
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
            let port = port
                .parse::<u16>()
                .map_err(|_| ChatClientError::InvalidAddress)?;
            Ok((host.to_string(), port, use_tls))
        } else {
            // No port specified, use default
            Ok((addr.to_string(), 8080, use_tls))
        }
    }

    pub async fn join_server(&mut self) -> Result<(), ChatClientError> {
        // First send version check
        logger::log_info(&format!("Sending version check (v{})...", VERSION));
        let version_message = ChatMessage::try_new(
            MessageTypes::VersionCheck,
            Some(VERSION.as_bytes().to_vec()),
        )?;
        self.send_message_chunked(version_message).await?;

        // Send join message with username and session token
        // Format: username|session_token
        let join_content = format!("{}|{}", self.chat_name, self.session_token);
        let chat_message =
            ChatMessage::try_new(MessageTypes::Join, Some(join_content.into_bytes()))?;
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
                        let server_name =
                            ServerName::try_from(self.server_host.clone()).map_err(|_| {
                                io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name")
                            })?;

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

                    // Restore user's status if they had one set
                    if let Some(status) = &self.current_status {
                        let content = Some(status.clone().into_bytes());
                        if let Ok(status_msg) =
                            ChatMessage::try_new(MessageTypes::SetStatus, content)
                            && let Err(e) = self.send_message_chunked(status_msg).await
                        {
                            logger::log_warning(&format!("Failed to restore status: {:?}", e));
                        }
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

    async fn handle_message(&mut self, message: ChatMessage) -> bool {
        match message.msg_type {
            MessageTypes::Ping => {
                // Respond to server ping with pong
                if let Ok(pong_msg) = ChatMessage::try_new(MessageTypes::Pong, None)
                    && let Err(e) = self.send_message_chunked(pong_msg).await
                {
                    logger::log_warning(&format!("Failed to send pong: {:?}", e));
                    return false; // Signal connection issue
                }
                return true;
            }
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
            MessageTypes::FileTransferRequest => {
                self.handle_file_transfer_request(&message);
            }
            MessageTypes::FileTransferResponse => {
                return self.handle_file_transfer_response(&message).await;
            }
            MessageTypes::SetStatus => {
                if let Some(content) = self.get_message_content(&message, "status") {
                    logger::log_success(&content);
                }
            }
            MessageTypes::Pong => {
                // Ignore pong messages (we don't send pings from client)
            }
            MessageTypes::VersionMismatch => {
                if let Some(content) = self.get_message_content(&message, "version mismatch") {
                    let parts: Vec<&str> = content.split('|').collect();
                    if parts.len() >= 3 {
                        logger::log_error(&format!(
                            "Version mismatch: client v{} != server v{}",
                            parts[0], parts[1]
                        ));
                        logger::log_error(&format!(
                            "Please upgrade your binary or Docker image. See: {}",
                            parts[2]
                        ));
                    } else {
                        logger::log_error(
                            "Version mismatch with server. Please upgrade your client.",
                        );
                    }
                    // Mark as kicked so we don't try to reconnect
                    self.was_kicked = true;
                    return false;
                }
            }
            MessageTypes::VersionCheck => {
                // Server shouldn't send this to client, ignore
            }
            _ => {
                logger::log_warning(&format!("Unknown message type: {:?}", message.msg_type));
            }
        }
        true
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

        let sender =
            match std::str::from_utf8(&content[sender_start + 1..sender_start + 1 + sender_len]) {
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

        let filename =
            match std::str::from_utf8(&content[filename_start..filename_start + filename_len]) {
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

    fn handle_file_transfer_request(&mut self, message: &ChatMessage) {
        let content = match message.get_content() {
            Some(c) => c,
            None => {
                logger::log_error("Received empty file transfer request");
                return;
            }
        };

        // Parse binary format: recipient_len(1)|recipient|sender_len(1)|sender|filename_len(1)|filename|filesize(8 bytes)
        if content.len() < 2 {
            logger::log_error("Invalid file transfer request format");
            return;
        }

        // Extract recipient
        let recipient_len = content[0] as usize;
        if content.len() < 1 + recipient_len + 1 {
            logger::log_error("Invalid file transfer request format");
            return;
        }

        let recipient = match std::str::from_utf8(&content[1..1 + recipient_len]) {
            Ok(s) => s,
            Err(_) => {
                logger::log_error("Invalid recipient name in file transfer request");
                return;
            }
        };

        // Check if this request is for us
        if recipient != self.chat_name {
            return; // Not for us, ignore
        }

        // Extract sender
        let sender_start = 1 + recipient_len;
        let sender_len = content[sender_start] as usize;
        if content.len() < sender_start + 1 + sender_len + 1 {
            logger::log_error("Invalid file transfer request format");
            return;
        }

        let sender =
            match std::str::from_utf8(&content[sender_start + 1..sender_start + 1 + sender_len]) {
                Ok(s) => s,
                Err(_) => {
                    logger::log_error("Invalid sender name in file transfer request");
                    return;
                }
            };

        // Extract filename
        let filename_len_pos = sender_start + 1 + sender_len;
        let filename_len = content[filename_len_pos] as usize;
        let filename_start = filename_len_pos + 1;
        if content.len() < filename_start + filename_len + 8 {
            logger::log_error("Invalid file transfer request format");
            return;
        }

        let filename =
            match std::str::from_utf8(&content[filename_start..filename_start + filename_len]) {
                Ok(s) => s,
                Err(_) => {
                    logger::log_error("Invalid filename in file transfer request");
                    return;
                }
            };

        // Extract file size (8 bytes, big-endian u64)
        let size_start = filename_start + filename_len;
        let file_size = u64::from_be_bytes([
            content[size_start],
            content[size_start + 1],
            content[size_start + 2],
            content[size_start + 3],
            content[size_start + 4],
            content[size_start + 5],
            content[size_start + 6],
            content[size_start + 7],
        ]) as usize;

        // Store the pending transfer
        self.pending_incoming.insert(
            sender.to_string(),
            PendingIncomingTransfer {
                sender: sender.to_string(),
                file_name: filename.to_string(),
                file_size,
            },
        );

        // Format file size for display
        let size_display = if file_size >= 1024 * 1024 {
            format!("{:.1} MB", file_size as f64 / (1024.0 * 1024.0))
        } else if file_size >= 1024 {
            format!("{:.1} KB", file_size as f64 / 1024.0)
        } else {
            format!("{} bytes", file_size)
        };

        logger::log_warning(&format!(
            "[FILE REQUEST from {}]: '{}' ({})",
            sender, filename, size_display
        ));
        logger::log_info(&format!(
            "Use /accept {} to accept or /reject {} to decline",
            sender, sender
        ));
    }

    async fn handle_file_transfer_response(&mut self, message: &ChatMessage) -> bool {
        let content = match message.get_content() {
            Some(c) => c,
            None => {
                logger::log_error("Received empty file transfer response");
                return true;
            }
        };

        // Parse format: recipient_len(1)|recipient|sender_len(1)|sender|accepted(1)
        if content.len() < 2 {
            logger::log_error("Invalid file transfer response format");
            return true;
        }

        // Extract recipient (original sender of the file request)
        let recipient_len = content[0] as usize;
        if content.len() < 1 + recipient_len + 1 {
            logger::log_error("Invalid file transfer response format");
            return true;
        }

        let recipient = match std::str::from_utf8(&content[1..1 + recipient_len]) {
            Ok(s) => s,
            Err(_) => {
                logger::log_error("Invalid recipient name in file transfer response");
                return true;
            }
        };

        // Check if this response is for us (we're the original sender)
        if recipient != self.chat_name {
            return true; // Not for us, ignore
        }

        // Extract sender (the one who accepted/rejected)
        let sender_start = 1 + recipient_len;
        let sender_len = content[sender_start] as usize;
        if content.len() < sender_start + 1 + sender_len + 1 {
            logger::log_error("Invalid file transfer response format");
            return true;
        }

        let responder =
            match std::str::from_utf8(&content[sender_start + 1..sender_start + 1 + sender_len]) {
                Ok(s) => s,
                Err(_) => {
                    logger::log_error("Invalid sender name in file transfer response");
                    return true;
                }
            };

        // Extract accepted flag
        let accepted_pos = sender_start + 1 + sender_len;
        let accepted = content[accepted_pos] == 1;

        if accepted {
            // Look up the pending transfer and send the file
            if let Some(transfer) = self.pending_outgoing.remove(responder) {
                logger::log_success(&format!(
                    "{} accepted file transfer for '{}'",
                    responder, transfer.file_name
                ));
                // Actually send the file now
                if let Err(e) = self
                    .send_file_data(&transfer.recipient, &transfer.file_path)
                    .await
                {
                    logger::log_error(&format!("Failed to send file: {:?}", e));
                }
            } else {
                logger::log_warning(&format!(
                    "Received acceptance from {} but no pending transfer found",
                    responder
                ));
            }
        } else {
            // Remove the pending transfer
            if let Some(transfer) = self.pending_outgoing.remove(responder) {
                logger::log_warning(&format!(
                    "{} rejected file transfer for '{}'",
                    responder, transfer.file_name
                ));
            } else {
                logger::log_warning(&format!(
                    "Received rejection from {} but no pending transfer found",
                    responder
                ));
            }
        }

        true
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
                for line in commands::help_text() {
                    logger::log_info(&line);
                }
                Ok(())
            }
            input::ClientUserInput::ListUsers => {
                let message = ChatMessage::try_new(MessageTypes::ListUsers, None)?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::Rename(new_name) => {
                let message =
                    ChatMessage::try_new(MessageTypes::RenameRequest, Some(new_name.into_bytes()))?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::SendFile {
                recipient,
                file_path,
            } => self.send_file_request(&recipient, &file_path).await,
            input::ClientUserInput::AcceptFile { sender } => {
                self.accept_file_transfer(&sender).await
            }
            input::ClientUserInput::RejectFile { sender } => {
                self.reject_file_transfer(&sender).await
            }
            input::ClientUserInput::Status(status) => {
                // Store status locally so we can restore it after reconnection
                self.current_status = status.clone();
                let content = status.map(|s| s.into_bytes());
                let message = ChatMessage::try_new(MessageTypes::SetStatus, content)?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::Quit => {
                // Send Leave message to server so it knows this is an explicit quit
                // (as opposed to a connection drop that might be a reconnection)
                let message = ChatMessage::try_new(MessageTypes::Leave, None)?;
                let _ = self.send_message_chunked(message).await;
                Ok(())
            }
        }
    }

    /// Send a file transfer request (not the actual file data)
    async fn send_file_request(
        &mut self,
        recipient: &str,
        file_path: &str,
    ) -> Result<(), ChatClientError> {
        let path = Path::new(file_path);

        // Check if file exists
        if !path.exists() {
            logger::log_error(&format!("File not found: {}", file_path));
            return Ok(());
        }

        // Get file metadata
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                logger::log_error(&format!("Failed to read file metadata: {}", e));
                return Ok(());
            }
        };

        let file_size = metadata.len() as usize;

        // Check file size (100MB limit, minus some overhead for metadata)
        let max_content_size = MAX_FILE_SIZE - 1024; // Leave room for headers
        if file_size > max_content_size {
            logger::log_error(&format!(
                "File too large: {} bytes (max {} bytes / ~100MB)",
                file_size, max_content_size
            ));
            return Ok(());
        }

        // Get file name
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Format file size for display
        let size_display = if file_size >= 1024 * 1024 {
            format!("{:.1} MB", file_size as f64 / (1024.0 * 1024.0))
        } else if file_size >= 1024 {
            format!("{:.1} KB", file_size as f64 / 1024.0)
        } else {
            format!("{} bytes", file_size)
        };

        logger::log_info(&format!(
            "Requesting to send '{}' ({}) to {}...",
            file_name, size_display, recipient
        ));

        // Store the pending transfer
        self.pending_outgoing.insert(
            recipient.to_string(),
            PendingOutgoingTransfer {
                recipient: recipient.to_string(),
                file_path: file_path.to_string(),
                file_name: file_name.to_string(),
                file_size,
            },
        );

        // Build file transfer request message
        // Format: recipient_len(1)|recipient|filename_len(1)|filename|filesize(8 bytes)
        let mut content = Vec::new();
        content.push(recipient.len() as u8);
        content.extend_from_slice(recipient.as_bytes());
        content.push(file_name.len() as u8);
        content.extend_from_slice(file_name.as_bytes());
        content.extend_from_slice(&(file_size as u64).to_be_bytes());

        let message = ChatMessage::try_new(MessageTypes::FileTransferRequest, Some(content))?;
        self.send_message_chunked(message).await?;

        logger::log_info(&format!(
            "File transfer request sent. Waiting for {} to accept...",
            recipient
        ));
        Ok(())
    }

    /// Actually send the file data (called after recipient accepts)
    async fn send_file_data(
        &mut self,
        recipient: &str,
        file_path: &str,
    ) -> Result<(), ChatClientError> {
        let path = Path::new(file_path);

        // Check if file still exists
        if !path.exists() {
            logger::log_error(&format!("File no longer exists: {}", file_path));
            return Ok(());
        }

        // Get file name
        let file_name = path
            .file_name()
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

    /// Accept a pending file transfer
    async fn accept_file_transfer(&mut self, sender: &str) -> Result<(), ChatClientError> {
        // Check if there's a pending transfer from this sender
        if let Some(transfer) = self.pending_incoming.remove(sender) {
            logger::log_info(&format!(
                "Accepting file '{}' from {}...",
                transfer.file_name, sender
            ));

            // Build response message
            // Format: sender_len(1)|sender|accepted(1)
            let mut content = Vec::new();
            content.push(sender.len() as u8);
            content.extend_from_slice(sender.as_bytes());
            content.push(1u8); // accepted = true

            let message = ChatMessage::try_new(MessageTypes::FileTransferResponse, Some(content))?;
            self.send_message_chunked(message).await?;
            Ok(())
        } else {
            logger::log_error(&format!("No pending file transfer from '{}'", sender));
            Ok(())
        }
    }

    /// Reject a pending file transfer
    async fn reject_file_transfer(&mut self, sender: &str) -> Result<(), ChatClientError> {
        // Check if there's a pending transfer from this sender
        if let Some(transfer) = self.pending_incoming.remove(sender) {
            logger::log_info(&format!(
                "Rejecting file '{}' from {}",
                transfer.file_name, sender
            ));

            // Build response message
            // Format: sender_len(1)|sender|accepted(1)
            let mut content = Vec::new();
            content.push(sender.len() as u8);
            content.extend_from_slice(sender.as_bytes());
            content.push(0u8); // accepted = false

            let message = ChatMessage::try_new(MessageTypes::FileTransferResponse, Some(content))?;
            self.send_message_chunked(message).await?;
            Ok(())
        } else {
            logger::log_error(&format!("No pending file transfer from '{}'", sender));
            Ok(())
        }
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
                            if !self.handle_message(message).await {
                                // handle_message returned false, indicating a connection issue
                                logger::log_warning("Connection issue detected while handling message");
                            }
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
                                    if let Err(e) = self.send_message_chunked(message).await {
                                        logger::log_warning("Connection lost while sending message");

                                        if !self.was_kicked {
                                            match self.reconnect().await {
                                                Ok(()) => {
                                                    // Connection restored
                                                }
                                                Err(reconnect_err) => {
                                                    logger::log_error(&format!("Failed to reconnect: {:?}", reconnect_err));
                                                    return Err(io::Error::other(format!("Failed to send ListUsers message: {e:?}")));
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(user_input) => {
                                    if let Err(e) = self.handle_user_input(user_input).await {
                                        // Check if this is a connection error that needs reconnection
                                        if matches!(e, ChatClientError::IoError) {
                                            logger::log_warning("Connection lost while sending message");

                                            if !self.was_kicked {
                                                match self.reconnect().await {
                                                    Ok(()) => {
                                                        // Connection restored
                                                    }
                                                    Err(reconnect_err) => {
                                                        logger::log_error(&format!("Failed to reconnect: {:?}", reconnect_err));
                                                        return Err(io::Error::other("Reconnection failed"));
                                                    }
                                                }
                                            }
                                        } else {
                                            logger::log_error(&format!("Error: {e:?}"));
                                        }
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
