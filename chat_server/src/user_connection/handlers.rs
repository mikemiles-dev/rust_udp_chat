use chat_shared::logger;
use chat_shared::message::{ChatMessage, MessageTypes};
use chat_shared::network::TcpMessageHandler;
use rand::Rng;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, broadcast};

use super::error::UserConnectionError;
use super::rate_limiting::RateLimiter;

// Helper struct to implement TcpMessageHandler for TcpStream
struct StreamWrapper<'a> {
    stream: &'a mut TcpStream,
}

impl<'a> TcpMessageHandler for StreamWrapper<'a> {
    fn get_stream(&mut self) -> &mut TcpStream {
        self.stream
    }
}

// Security limits
pub const MAX_USERNAME_LENGTH: usize = 32;
pub const MAX_MESSAGE_LENGTH: usize = 1024; // 1KB max message content

pub struct MessageHandlers<'a> {
    pub addr: SocketAddr,
    pub tx: &'a broadcast::Sender<(ChatMessage, SocketAddr)>,
    pub connected_clients: &'a Arc<RwLock<HashSet<String>>>,
}

impl<'a> MessageHandlers<'a> {
    pub fn randomize_username(&self, username: &str) -> String {
        let mut rng = rand::thread_rng();
        let random_suffix: u32 = rng.gen_range(1000..9999);
        format!("{}_{}", username, random_suffix)
    }

    pub async fn process_message(
        &self,
        message: ChatMessage,
        rate_limiter: &mut RateLimiter,
        stream: &mut TcpStream,
        chat_name: &mut Option<String>,
    ) -> Result<(), UserConnectionError> {
        let mut tcp_handler = StreamWrapper { stream };
        // Rate limiting check (except for Join messages)
        if !matches!(message.msg_type, MessageTypes::Join)
            && !rate_limiter.check_and_consume()
        {
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
                self.process_direct_message(message.content_as_string(), &mut tcp_handler, chat_name)
                    .await?;
            }
            _ => (),
        }
        Ok(())
    }

    async fn process_list_users(
        &self,
        tcp_handler: &mut StreamWrapper<'_>,
    ) -> Result<(), UserConnectionError> {
        let clients = self.connected_clients.clone();
        let clients = clients.read().await;
        let user_list = clients.iter().cloned().collect::<Vec<String>>().join("\n");
        let list_message =
            ChatMessage::try_new(MessageTypes::ListUsers, Some(user_list.into_bytes()))
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

    async fn process_direct_message(
        &self,
        content: Option<String>,
        tcp_handler: &mut StreamWrapper<'_>,
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

    async fn process_join(
        &self,
        username: Option<String>,
        tcp_handler: &mut StreamWrapper<'_>,
        chat_name: &mut Option<String>,
    ) -> Result<(), UserConnectionError> {
        let content = username.ok_or(UserConnectionError::InvalidMessage)?;

        // Validate username length
        if content.is_empty() || content.len() > MAX_USERNAME_LENGTH {
            logger::log_warning(&format!(
                "Invalid username length from {}: {} chars",
                self.addr,
                content.len()
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        // Validate username characters (alphanumeric, underscore, hyphen only)
        if !content
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            logger::log_warning(&format!(
                "Invalid username characters from {}: {}",
                self.addr, content
            ));
            return Err(UserConnectionError::InvalidMessage);
        }

        let connected_clients = self.connected_clients.clone();
        {
            let mut clients = connected_clients.write().await;
            *chat_name = if !clients.insert(content.clone()) {
                logger::log_warning(&format!("User '{}' already exists, renaming...", content));
                let new_name = self.randomize_username(&content);
                if !clients.insert(new_name.clone()) {
                    logger::log_error(&format!(
                        "Failed to assign random username to '{}'",
                        content
                    ));
                    return Err(UserConnectionError::JoinError);
                }
                logger::log_success(&format!("User '{}' renamed to '{}'", content, new_name));
                let rename_message = ChatMessage::try_new(
                    MessageTypes::UserRename,
                    Some(new_name.clone().into_bytes()),
                )
                .map_err(|_| UserConnectionError::InvalidMessage)?;
                tcp_handler
                    .send_message_chunked(rename_message)
                    .await
                    .map_err(UserConnectionError::IoError)?;
                Some(new_name)
            } else {
                Some(content)
            };
        }

        if let Some(chat_name) = &chat_name {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_username_validation_valid() {
        // Valid usernames
        assert_eq!("alice".len(), 5);
        assert!("alice"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));

        assert_eq!("Bob123".len(), 6);
        assert!("Bob123"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));

        assert_eq!("user_name".len(), 9);
        assert!("user_name"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));

        assert_eq!("user-name".len(), 9);
        assert!("user-name"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
    }

    #[test]
    fn test_username_validation_invalid_chars() {
        // Invalid characters
        assert!(!"user@name"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
        assert!(!"user name"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
        assert!(!"user!name"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
        assert!(!"user.name"
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
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
