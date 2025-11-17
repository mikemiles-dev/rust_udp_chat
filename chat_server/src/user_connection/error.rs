use std::io;
use std::net::SocketAddr;
use chat_shared::message::ChatMessage;
use tokio::sync::broadcast;

#[derive(Debug)]
pub enum UserConnectionError {
    IoError(io::Error),
    BroadcastError(broadcast::error::SendError<(ChatMessage, SocketAddr)>),
    JoinError,
    InvalidMessage,
}

impl std::fmt::Display for UserConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserConnectionError::IoError(e) => write!(f, "IO Error: {}", e),
            UserConnectionError::BroadcastError(e) => write!(f, "Broadcast Error: {}", e),
            UserConnectionError::JoinError => write!(f, "Join Error: Username already taken"),
            UserConnectionError::InvalidMessage => write!(f, "Invalid Message Error"),
        }
    }
}
