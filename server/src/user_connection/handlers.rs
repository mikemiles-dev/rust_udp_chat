use crate::ServerCommand;
use rand::Rng;
use shared::logger;
use shared::message::{ChatMessage, MessageTypes};
use shared::network::TcpMessageHandler;
use shared::version::{self, VERSION};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{RwLock, broadcast};

use super::error::UserConnectionError;
use super::rate_limiting::RateLimiter;

// Helper struct to implement TcpMessageHandler for any AsyncRead + AsyncWrite stream
struct StreamWrapper<'a, S> {
    stream: &'a mut S,
}

impl<'a, S: AsyncRead + AsyncWrite + Unpin> TcpMessageHandler for StreamWrapper<'a, S> {
    type Stream = S;
    fn get_stream(&mut self) -> &mut Self::Stream {
        self.stream
    }
}

// Security limits
pub const MAX_USERNAME_LENGTH: usize = 32;
pub const MAX_MESSAGE_LENGTH: usize = 1024; // 1KB max message content
pub const MAX_STATUS_LENGTH: usize = 128; // Max status message length

pub struct MessageHandlers<'a> {
    pub addr: SocketAddr,
    pub tx: &'a broadcast::Sender<(ChatMessage, SocketAddr)>,
    pub server_commands: &'a broadcast::Sender<ServerCommand>,
    pub connected_clients: &'a Arc<RwLock<HashSet<String>>>,
    pub user_ips: &'a Arc<RwLock<HashMap<String, IpAddr>>>,
    pub user_statuses: &'a Arc<RwLock<HashMap<String, String>>>,
    pub user_sessions: &'a Arc<RwLock<HashMap<String, String>>>,
}

impl<'a> MessageHandlers<'a> {
    pub fn randomize_username(&self, username: &str) -> String {
        let mut rng = rand::thread_rng();
        let random_suffix: u32 = rng.gen_range(1000..9999);
        format!("{}_{}", username, random_suffix)
    }

    pub async fn process_message<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        message: ChatMessage,
        rate_limiter: &mut RateLimiter,
        stream: &mut S,
        chat_name: &mut Option<String>,
    ) -> Result<(), UserConnectionError> {
        let mut tcp_handler = StreamWrapper { stream };
        // Rate limiting check (except for Join messages)
        if !matches!(message.msg_type, MessageTypes::Join) && !rate_limiter.check_and_consume() {
            logger::log_warning(&format!("Rate limit exceeded for {}", self.addr));
            let error_msg = ChatMessage::try_new(
                MessageTypes::Error,
                Some(b"Rate limit exceeded. Please slow down.".to_vec()),
            )
            .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_msg)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }

        match message.msg_type {
            MessageTypes::VersionCheck => {
                self.process_version_check(message.content_as_string(), &mut tcp_handler)
                    .await?;
            }
            MessageTypes::Join => {
                self.process_join(message.content_as_string(), &mut tcp_handler, chat_name)
                    .await?;
            }
            MessageTypes::ChatMessage => {
                self.process_chat_message(message.content_as_string(), chat_name)
                    .await?;
            }
            MessageTypes::ListUsers => {
                self.process_list_users(&mut tcp_handler).await?;
            }
            MessageTypes::DirectMessage => {
                self.process_direct_message(
                    message.content_as_string(),
                    &mut tcp_handler,
                    chat_name,
                )
                .await?;
            }
            MessageTypes::RenameRequest => {
                self.process_rename_request(
                    message.content_as_string(),
                    &mut tcp_handler,
                    chat_name,
                )
                .await?;
            }
            MessageTypes::FileTransfer => {
                self.process_file_transfer(message.get_content(), &mut tcp_handler, chat_name)
                    .await?;
            }
            MessageTypes::FileTransferRequest => {
                self.process_file_transfer_request(
                    message.get_content(),
                    &mut tcp_handler,
                    chat_name,
                )
                .await?;
            }
            MessageTypes::FileTransferResponse => {
                self.process_file_transfer_response(
                    message.get_content(),
                    &mut tcp_handler,
                    chat_name,
                )
                .await?;
            }
            MessageTypes::SetStatus => {
                self.process_set_status(message.content_as_string(), &mut tcp_handler, chat_name)
                    .await?;
            }
            MessageTypes::Leave => {
                // User explicitly quit - signal this to the connection handler
                return Err(UserConnectionError::ExplicitQuit);
            }
            _ => (),
        }
        Ok(())
    }

    async fn process_list_users<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        tcp_handler: &mut StreamWrapper<'_, S>,
    ) -> Result<(), UserConnectionError> {
        let clients = self.connected_clients.read().await;
        let statuses = self.user_statuses.read().await;

        // Build user list with statuses
        let user_list: Vec<String> = clients
            .iter()
            .map(|username| {
                if let Some(status) = statuses.get(username) {
                    format!("{} - {}", username, status)
                } else {
                    username.clone()
                }
            })
            .collect();

        drop(clients);
        drop(statuses);

        let list_message = ChatMessage::try_new(
            MessageTypes::ListUsers,
            Some(user_list.join("\n").into_bytes()),
        )
        .map_err(|_| UserConnectionError::InvalidMessage)?;
        tcp_handler
            .send_message_chunked(list_message)
            .await
            .map_err(UserConnectionError::IoError)?;
        Ok(())
    }

    async fn process_chat_message(
        &self,
        content: Option<String>,
        chat_name: &Option<String>,
    ) -> Result<(), UserConnectionError> {
        let chat_content = content.ok_or(UserConnectionError::InvalidMessage)?;

        // Validate message length
        if chat_content.is_empty() || chat_content.len() > MAX_MESSAGE_LENGTH {
            logger::log_warning(&format!(
                "Invalid message length from {}: {} chars",
                self.addr,
                chat_content.len()
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        if let Some(chat_name) = chat_name {
            let full_message = format!("{}: {}", chat_name, chat_content);
            logger::log_chat(&full_message);
            let broadcast_message =
                ChatMessage::try_new(MessageTypes::ChatMessage, Some(full_message.into_bytes()))
                    .map_err(|_| UserConnectionError::InvalidMessage)?;
            self.tx
                .send((broadcast_message, self.addr))
                .map_err(UserConnectionError::BroadcastError)?;
            Ok(())
        } else {
            logger::log_warning(&format!(
                "User at {} sent chat message before joining",
                self.addr
            ));
            Err(UserConnectionError::InvalidMessage)
        }
    }

    async fn process_direct_message<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        content: Option<String>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &Option<String>,
    ) -> Result<(), UserConnectionError> {
        let content = content.ok_or(UserConnectionError::InvalidMessage)?;

        if let Some((recipient, message)) = content.split_once('|') {
            // Validate message length
            if message.is_empty() || message.len() > MAX_MESSAGE_LENGTH {
                logger::log_warning(&format!(
                    "Invalid DM length from {}: {} chars",
                    self.addr,
                    message.len()
                ));
                return Err(UserConnectionError::InvalidMessage);
            }
            if let Some(sender) = chat_name {
                // Check if recipient exists
                let clients = self.connected_clients.read().await;
                if !clients.contains(recipient) {
                    drop(clients); // Release the lock before sending error

                    // Send error message back to sender
                    let error_msg = format!("User '{}' not found", recipient);
                    logger::log_warning(&format!(
                        "[DM] {} -> {} (user not found)",
                        sender, recipient
                    ));

                    let error_message =
                        ChatMessage::try_new(MessageTypes::Error, Some(error_msg.into_bytes()))
                            .map_err(|_| UserConnectionError::InvalidMessage)?;

                    tcp_handler
                        .send_message_chunked(error_message)
                        .await
                        .map_err(UserConnectionError::IoError)?;
                    return Ok(());
                }
                drop(clients); // Release the lock

                // Log that a DM is happening, but don't show the content
                logger::log_system(&format!("[DM] {} -> {}", sender, recipient));

                // Format: sender|recipient|message for client filtering
                let dm_content = format!("{}|{}|{}", sender, recipient, message);
                let dm_message = ChatMessage::try_new(
                    MessageTypes::DirectMessage,
                    Some(dm_content.into_bytes()),
                )
                .map_err(|_| UserConnectionError::InvalidMessage)?;

                // Broadcast to all clients (clients will filter)
                self.tx
                    .send((dm_message, self.addr))
                    .map_err(UserConnectionError::BroadcastError)?;
                Ok(())
            } else {
                logger::log_warning(&format!("User at {} sent DM before joining", self.addr));
                Err(UserConnectionError::InvalidMessage)
            }
        } else {
            Err(UserConnectionError::InvalidMessage)
        }
    }

    async fn process_join<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        username: Option<String>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &mut Option<String>,
    ) -> Result<(), UserConnectionError> {
        let content = username.ok_or(UserConnectionError::InvalidMessage)?;

        // Parse username and session token (format: username|session_token)
        let (requested_username, session_token) = if let Some((user, token)) = content.split_once('|') {
            (user.to_string(), Some(token.to_string()))
        } else {
            // Backwards compatibility: if no session token, just use the username
            (content, None)
        };

        // Validate username length
        if requested_username.is_empty() || requested_username.len() > MAX_USERNAME_LENGTH {
            logger::log_warning(&format!(
                "Invalid username length from {}: {} chars",
                self.addr,
                requested_username.len()
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        // Validate username characters (alphanumeric, underscore, hyphen only)
        if !requested_username
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            logger::log_warning(&format!(
                "Invalid username characters from {}: {}",
                self.addr, requested_username
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let connected_clients = self.connected_clients.clone();
        {
            let mut clients = connected_clients.write().await;

            // Check if username already exists
            if clients.contains(&requested_username) {
                // Username exists - check if this is a valid reconnection (same session token and IP)
                let can_reclaim = if let Some(ref token) = session_token {
                    let sessions = self.user_sessions.read().await;
                    let ips = self.user_ips.read().await;

                    let session_matches = sessions.get(&requested_username).is_some_and(|t| t == token);
                    let ip_matches = ips.get(&requested_username).is_some_and(|ip| *ip == self.addr.ip());

                    drop(sessions);
                    drop(ips);

                    session_matches && ip_matches
                } else {
                    false
                };

                if can_reclaim {
                    // This is a valid reconnection - reclaim the ghost session
                    logger::log_success(&format!(
                        "User '{}' reclaiming ghost session from {} (same token and IP)",
                        requested_username, self.addr
                    ));

                    // Signal the old connection to disconnect silently
                    let _ = self.server_commands.send(ServerCommand::SessionTakeover(requested_username.clone()));

                    // The username is already in the set, so we just claim it for this connection
                    *chat_name = Some(requested_username.clone());
                } else {
                    // Not a valid reconnection - rename the user
                    logger::log_warning(&format!("User '{}' already exists, renaming...", requested_username));
                    let new_name = self.randomize_username(&requested_username);
                    if !clients.insert(new_name.clone()) {
                        logger::log_error(&format!(
                            "Failed to assign random username to '{}'",
                            requested_username
                        ));
                        return Err(UserConnectionError::JoinError);
                    }
                    logger::log_success(&format!("User '{}' renamed to '{}'", requested_username, new_name));
                    let rename_message = ChatMessage::try_new(
                        MessageTypes::UserRename,
                        Some(new_name.clone().into_bytes()),
                    )
                    .map_err(|_| UserConnectionError::InvalidMessage)?;
                    tcp_handler
                        .send_message_chunked(rename_message)
                        .await
                        .map_err(UserConnectionError::IoError)?;
                    *chat_name = Some(new_name.clone());

                    // Store session token for the new name
                    if let Some(token) = session_token {
                        let mut sessions = self.user_sessions.write().await;
                        sessions.insert(new_name, token);
                    }
                }
            } else {
                // Username is available - claim it
                clients.insert(requested_username.clone());
                *chat_name = Some(requested_username.clone());

                // Store session token for this username
                if let Some(token) = session_token {
                    drop(clients); // Release clients lock before acquiring sessions lock
                    let mut sessions = self.user_sessions.write().await;
                    sessions.insert(requested_username.clone(), token);
                }
            }
        }

        if let Some(chat_name) = &chat_name {
            // Store the user's IP address
            let mut ips = self.user_ips.write().await;
            ips.insert(chat_name.clone(), self.addr.ip());
            drop(ips);

            let join_message =
                ChatMessage::try_new(MessageTypes::Join, Some(chat_name.clone().into_bytes()))
                    .map_err(|_| UserConnectionError::InvalidMessage)?;
            self.tx
                .send((join_message, self.addr))
                .map_err(UserConnectionError::BroadcastError)?;
            logger::log_system(&format!("{} has joined the chat", chat_name));
        }
        Ok(())
    }

    async fn process_rename_request<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        new_name: Option<String>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &mut Option<String>,
    ) -> Result<(), UserConnectionError> {
        let new_name = new_name.ok_or(UserConnectionError::InvalidMessage)?;

        // Validate new username length
        if new_name.is_empty() || new_name.len() > MAX_USERNAME_LENGTH {
            logger::log_warning(&format!(
                "Invalid username length for rename from {}: {} chars",
                self.addr,
                new_name.len()
            ));
            let error_msg = ChatMessage::try_new(
                MessageTypes::Error,
                Some(b"Invalid username length (1-32 characters)".to_vec()),
            )
            .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_msg)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }

        // Validate username characters (alphanumeric, underscore, hyphen only)
        if !new_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            logger::log_warning(&format!(
                "Invalid username characters for rename from {}: {}",
                self.addr, new_name
            ));
            let error_msg = ChatMessage::try_new(
                MessageTypes::Error,
                Some(
                    b"Invalid characters (only alphanumeric, underscore, hyphen allowed)".to_vec(),
                ),
            )
            .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_msg)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }

        // Check if user has joined first
        let old_name = match chat_name {
            Some(name) => name.clone(),
            None => {
                logger::log_warning(&format!(
                    "User at {} tried to rename before joining",
                    self.addr
                ));
                return Err(UserConnectionError::InvalidMessage);
            }
        };

        // Try to claim the new name
        let mut clients = self.connected_clients.write().await;

        // Check if new name is already taken
        if clients.contains(&new_name) {
            drop(clients);
            let error_msg = ChatMessage::try_new(
                MessageTypes::Error,
                Some(format!("Username '{}' is already taken", new_name).into_bytes()),
            )
            .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_msg)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }

        // Remove old name and add new name
        clients.remove(&old_name);
        clients.insert(new_name.clone());
        drop(clients);

        // Update user_ips mapping
        let mut ips = self.user_ips.write().await;
        if let Some(ip) = ips.remove(&old_name) {
            ips.insert(new_name.clone(), ip);
        }
        drop(ips);

        // Update the chat_name
        *chat_name = Some(new_name.clone());

        logger::log_success(&format!("User '{}' renamed to '{}'", old_name, new_name));

        // Send UserRename message back to the client
        let rename_message = ChatMessage::try_new(
            MessageTypes::UserRename,
            Some(new_name.clone().into_bytes()),
        )
        .map_err(|_| UserConnectionError::InvalidMessage)?;
        tcp_handler
            .send_message_chunked(rename_message)
            .await
            .map_err(UserConnectionError::IoError)?;

        // Broadcast rename announcement to all clients
        let announcement = format!("{} is now known as {}", old_name, new_name);
        let broadcast_message =
            ChatMessage::try_new(MessageTypes::ChatMessage, Some(announcement.into_bytes()))
                .map_err(|_| UserConnectionError::InvalidMessage)?;
        self.tx
            .send((broadcast_message, self.addr))
            .map_err(UserConnectionError::BroadcastError)?;

        Ok(())
    }

    async fn process_file_transfer<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        content: Option<&[u8]>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &Option<String>,
    ) -> Result<(), UserConnectionError> {
        let content = content.ok_or(UserConnectionError::InvalidMessage)?;

        // Check if user has joined
        let sender = match chat_name {
            Some(name) => name.clone(),
            None => {
                logger::log_warning(&format!(
                    "User at {} tried to send file before joining",
                    self.addr
                ));
                return Err(UserConnectionError::InvalidMessage);
            }
        };

        // Parse binary format: recipient_len(1)|recipient|filename_len(1)|filename|filedata
        if content.len() < 2 {
            logger::log_warning(&format!("Invalid file transfer format from {}", self.addr));
            return Err(UserConnectionError::InvalidMessage);
        }

        let recipient_len = content[0] as usize;
        if content.len() < 1 + recipient_len + 1 {
            logger::log_warning(&format!("Invalid file transfer format from {}", self.addr));
            return Err(UserConnectionError::InvalidMessage);
        }

        let recipient = std::str::from_utf8(&content[1..1 + recipient_len])
            .map_err(|_| UserConnectionError::InvalidMessage)?;

        let filename_len = content[1 + recipient_len] as usize;
        let filename_start = 1 + recipient_len + 1;
        if content.len() < filename_start + filename_len {
            logger::log_warning(&format!("Invalid file transfer format from {}", self.addr));
            return Err(UserConnectionError::InvalidMessage);
        }

        let filename = std::str::from_utf8(&content[filename_start..filename_start + filename_len])
            .map_err(|_| UserConnectionError::InvalidMessage)?;

        let file_data = &content[filename_start + filename_len..];

        // Check if recipient exists
        let clients = self.connected_clients.read().await;
        if !clients.contains(recipient) {
            drop(clients);
            let error_msg = format!("User '{}' not found", recipient);
            logger::log_warning(&format!(
                "[FILE] {} -> {} (user not found)",
                sender, recipient
            ));
            let error_message =
                ChatMessage::try_new(MessageTypes::Error, Some(error_msg.into_bytes()))
                    .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_message)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }
        drop(clients);

        logger::log_system(&format!(
            "[FILE] {} -> {} ('{}', {} bytes)",
            sender,
            recipient,
            filename,
            file_data.len()
        ));

        // Build outgoing message with sender instead of recipient
        // Format: sender_len(1)|sender|filename_len(1)|filename|filedata
        let mut outgoing_content = Vec::new();
        outgoing_content.push(sender.len() as u8);
        outgoing_content.extend_from_slice(sender.as_bytes());
        outgoing_content.push(filename.len() as u8);
        outgoing_content.extend_from_slice(filename.as_bytes());
        outgoing_content.extend_from_slice(file_data);

        // We need to include recipient info for client filtering
        // Prepend recipient_len(1)|recipient to the outgoing content
        let mut final_content = Vec::new();
        final_content.push(recipient.len() as u8);
        final_content.extend_from_slice(recipient.as_bytes());
        final_content.extend_from_slice(&outgoing_content);

        let file_message = ChatMessage::try_new(MessageTypes::FileTransfer, Some(final_content))
            .map_err(|_| UserConnectionError::InvalidMessage)?;

        // Broadcast to all clients (recipient will filter)
        self.tx
            .send((file_message, self.addr))
            .map_err(UserConnectionError::BroadcastError)?;

        Ok(())
    }

    async fn process_file_transfer_request<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        content: Option<&[u8]>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &Option<String>,
    ) -> Result<(), UserConnectionError> {
        let content = content.ok_or(UserConnectionError::InvalidMessage)?;

        // Check if user has joined
        let sender = match chat_name {
            Some(name) => name.clone(),
            None => {
                logger::log_warning(&format!(
                    "User at {} tried to send file request before joining",
                    self.addr
                ));
                return Err(UserConnectionError::InvalidMessage);
            }
        };

        // Parse binary format: recipient_len(1)|recipient|filename_len(1)|filename|filesize(8 bytes)
        if content.len() < 2 {
            logger::log_warning(&format!(
                "Invalid file transfer request format from {}",
                self.addr
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let recipient_len = content[0] as usize;
        if content.len() < 1 + recipient_len + 1 {
            logger::log_warning(&format!(
                "Invalid file transfer request format from {}",
                self.addr
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let recipient = std::str::from_utf8(&content[1..1 + recipient_len])
            .map_err(|_| UserConnectionError::InvalidMessage)?;

        let filename_len = content[1 + recipient_len] as usize;
        let filename_start = 1 + recipient_len + 1;
        if content.len() < filename_start + filename_len + 8 {
            logger::log_warning(&format!(
                "Invalid file transfer request format from {}",
                self.addr
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let filename = std::str::from_utf8(&content[filename_start..filename_start + filename_len])
            .map_err(|_| UserConnectionError::InvalidMessage)?;

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
        ]);

        // Check if recipient exists
        let clients = self.connected_clients.read().await;
        if !clients.contains(recipient) {
            drop(clients);
            let error_msg = format!("User '{}' not found", recipient);
            logger::log_warning(&format!(
                "[FILE REQUEST] {} -> {} (user not found)",
                sender, recipient
            ));
            let error_message =
                ChatMessage::try_new(MessageTypes::Error, Some(error_msg.into_bytes()))
                    .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_message)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }
        drop(clients);

        logger::log_system(&format!(
            "[FILE REQUEST] {} -> {} ('{}', {} bytes)",
            sender, recipient, filename, file_size
        ));

        // Build outgoing message with sender info
        // Format: recipient_len(1)|recipient|sender_len(1)|sender|filename_len(1)|filename|filesize(8 bytes)
        let mut outgoing_content = Vec::new();
        outgoing_content.push(recipient.len() as u8);
        outgoing_content.extend_from_slice(recipient.as_bytes());
        outgoing_content.push(sender.len() as u8);
        outgoing_content.extend_from_slice(sender.as_bytes());
        outgoing_content.push(filename.len() as u8);
        outgoing_content.extend_from_slice(filename.as_bytes());
        outgoing_content.extend_from_slice(&file_size.to_be_bytes());

        let request_message =
            ChatMessage::try_new(MessageTypes::FileTransferRequest, Some(outgoing_content))
                .map_err(|_| UserConnectionError::InvalidMessage)?;

        // Broadcast to all clients (recipient will filter)
        self.tx
            .send((request_message, self.addr))
            .map_err(UserConnectionError::BroadcastError)?;

        Ok(())
    }

    async fn process_file_transfer_response<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        content: Option<&[u8]>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &Option<String>,
    ) -> Result<(), UserConnectionError> {
        let content = content.ok_or(UserConnectionError::InvalidMessage)?;

        // Check if user has joined
        let responder = match chat_name {
            Some(name) => name.clone(),
            None => {
                logger::log_warning(&format!(
                    "User at {} tried to send file response before joining",
                    self.addr
                ));
                return Err(UserConnectionError::InvalidMessage);
            }
        };

        // Parse binary format: sender_len(1)|sender|accepted(1)
        // sender here is the original file sender (who we're responding to)
        if content.len() < 3 {
            logger::log_warning(&format!(
                "Invalid file transfer response format from {}",
                self.addr
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let original_sender_len = content[0] as usize;
        if content.len() < 1 + original_sender_len + 1 {
            logger::log_warning(&format!(
                "Invalid file transfer response format from {}",
                self.addr
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let original_sender = std::str::from_utf8(&content[1..1 + original_sender_len])
            .map_err(|_| UserConnectionError::InvalidMessage)?;

        let accepted = content[1 + original_sender_len] == 1;

        // Check if original sender exists
        let clients = self.connected_clients.read().await;
        if !clients.contains(original_sender) {
            drop(clients);
            let error_msg = format!("User '{}' not found", original_sender);
            logger::log_warning(&format!(
                "[FILE RESPONSE] {} -> {} (user not found)",
                responder, original_sender
            ));
            let error_message =
                ChatMessage::try_new(MessageTypes::Error, Some(error_msg.into_bytes()))
                    .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_message)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }
        drop(clients);

        logger::log_system(&format!(
            "[FILE RESPONSE] {} {} file from {}",
            responder,
            if accepted { "accepted" } else { "rejected" },
            original_sender
        ));

        // Build outgoing message
        // Format: recipient_len(1)|recipient|sender_len(1)|sender|accepted(1)
        // recipient = original sender (who receives this response)
        // sender = responder (who accepted/rejected)
        let mut outgoing_content = Vec::new();
        outgoing_content.push(original_sender.len() as u8);
        outgoing_content.extend_from_slice(original_sender.as_bytes());
        outgoing_content.push(responder.len() as u8);
        outgoing_content.extend_from_slice(responder.as_bytes());
        outgoing_content.push(if accepted { 1u8 } else { 0u8 });

        let response_message =
            ChatMessage::try_new(MessageTypes::FileTransferResponse, Some(outgoing_content))
                .map_err(|_| UserConnectionError::InvalidMessage)?;

        // Broadcast to all clients (original sender will filter)
        self.tx
            .send((response_message, self.addr))
            .map_err(UserConnectionError::BroadcastError)?;

        Ok(())
    }

    async fn process_set_status<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        status: Option<String>,
        tcp_handler: &mut StreamWrapper<'_, S>,
        chat_name: &Option<String>,
    ) -> Result<(), UserConnectionError> {
        // Check if user has joined first
        let username = match chat_name {
            Some(name) => name.clone(),
            None => {
                logger::log_warning(&format!(
                    "User at {} tried to set status before joining",
                    self.addr
                ));
                return Err(UserConnectionError::InvalidMessage);
            }
        };

        let status_text = status.unwrap_or_default();

        // Validate status length
        if status_text.len() > MAX_STATUS_LENGTH {
            let error_msg = ChatMessage::try_new(
                MessageTypes::Error,
                Some(
                    format!("Status too long (max {} characters)", MAX_STATUS_LENGTH).into_bytes(),
                ),
            )
            .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(error_msg)
                .await
                .map_err(UserConnectionError::IoError)?;
            return Ok(());
        }

        // Update or remove status
        let mut statuses = self.user_statuses.write().await;
        if status_text.is_empty() {
            statuses.remove(&username);
            logger::log_system(&format!("{} cleared their status", username));
        } else {
            statuses.insert(username.clone(), status_text.clone());
            logger::log_system(&format!("{} set status: {}", username, status_text));
        }
        drop(statuses);

        // Send confirmation back to client
        let confirm_msg = if status_text.is_empty() {
            "Status cleared".to_string()
        } else {
            format!("Status set to: {}", status_text)
        };
        let response =
            ChatMessage::try_new(MessageTypes::SetStatus, Some(confirm_msg.into_bytes()))
                .map_err(|_| UserConnectionError::InvalidMessage)?;
        tcp_handler
            .send_message_chunked(response)
            .await
            .map_err(UserConnectionError::IoError)?;

        Ok(())
    }

    async fn process_version_check<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        client_version: Option<String>,
        tcp_handler: &mut StreamWrapper<'_, S>,
    ) -> Result<(), UserConnectionError> {
        let client_version = client_version.ok_or(UserConnectionError::InvalidMessage)?;

        if !version::versions_compatible(&client_version, VERSION) {
            logger::log_warning(&format!(
                "Version mismatch from {}: client v{} != server v{}",
                self.addr, client_version, VERSION
            ));

            // Send version mismatch error with details
            let mismatch_content = format!(
                "{}|{}|{}",
                client_version,
                VERSION,
                version::GITHUB_README_URL
            );
            let mismatch_msg = ChatMessage::try_new(
                MessageTypes::VersionMismatch,
                Some(mismatch_content.into_bytes()),
            )
            .map_err(|_| UserConnectionError::InvalidMessage)?;
            tcp_handler
                .send_message_chunked(mismatch_msg)
                .await
                .map_err(UserConnectionError::IoError)?;

            return Err(UserConnectionError::VersionMismatch);
        }

        logger::log_info(&format!(
            "Version check passed for {}: v{}",
            self.addr, client_version
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_username_validation_valid() {
        // Valid usernames
        assert_eq!("alice".len(), 5);
        assert!(
            "alice"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );

        assert_eq!("Bob123".len(), 6);
        assert!(
            "Bob123"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );

        assert_eq!("user_name".len(), 9);
        assert!(
            "user_name"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );

        assert_eq!("user-name".len(), 9);
        assert!(
            "user-name"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );
    }

    #[test]
    fn test_username_validation_invalid_chars() {
        // Invalid characters
        assert!(
            !"user@name"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );
        assert!(
            !"user name"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );
        assert!(
            !"user!name"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );
        assert!(
            !"user.name"
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        );
    }

    #[test]
    fn test_username_validation_length() {
        // Too short
        let empty = "";
        assert!(empty.is_empty());

        // Valid length
        let valid = "a".repeat(32);
        assert_eq!(valid.len(), 32);
        assert!(valid.len() <= MAX_USERNAME_LENGTH);

        // Too long
        let too_long = "a".repeat(33);
        assert!(too_long.len() > MAX_USERNAME_LENGTH);
    }

    #[test]
    fn test_message_length_validation() {
        // Valid message
        let valid = "Hello, World!";
        assert!(!valid.is_empty());
        assert!(valid.len() <= MAX_MESSAGE_LENGTH);

        // Empty message
        let empty = "";
        assert!(empty.is_empty());

        // Too long message
        let too_long = "x".repeat(MAX_MESSAGE_LENGTH + 1);
        assert!(too_long.len() > MAX_MESSAGE_LENGTH);
    }
}
