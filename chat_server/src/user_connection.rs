use chat_shared::logger;
use chat_shared::message::{ChatMessage, MessageTypes};
use chat_shared::network::{TcpMessageHandler, TcpMessageHandlerError};
use rand::Rng;
use std::collections::HashSet;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, broadcast};

pub struct UserConnection {
    socket: TcpStream,
    addr: SocketAddr,
    tx: broadcast::Sender<(ChatMessage, SocketAddr)>,
    connected_clients: Arc<RwLock<HashSet<String>>>,
    chat_name: Option<String>,
}

impl TcpMessageHandler for UserConnection {
    fn get_stream(&mut self) -> &mut tokio::net::TcpStream {
        &mut self.socket
    }
}

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

impl UserConnection {
    pub fn new(
        socket: TcpStream,
        addr: SocketAddr,
        tx: broadcast::Sender<(ChatMessage, SocketAddr)>,
        connected_clients: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        UserConnection {
            socket,
            addr,
            tx,
            connected_clients,
            chat_name: None,
        }
    }

    pub async fn handle(&mut self) -> Result<(), UserConnectionError> {
        logger::log_info(&format!("New client connected: {}", self.addr));

        let mut rx = self.tx.subscribe();

        loop {
            tokio::select! {
                // Branch 1: Receive from client
                result = self.read_message_chunked() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.process_message(msg).await {
                                logger::log_error(&format!("Error handling message from {}: {:?}", self.addr, e));
                            }
                        }
                        Err(TcpMessageHandlerError::IoError(e)) => {
                            logger::log_error(&format!("IO error reading from {}: {:?}", self.addr, e));
                            break;
                        }
                        Err(TcpMessageHandlerError::Disconnect) => {
                            if let Some(chat_name) = &self.chat_name {
                                let mut clients = self.connected_clients.write().await;
                                clients.remove(chat_name);
                                let leave_message = ChatMessage::try_new(
                                    MessageTypes::Leave,
                                    Some(chat_name.clone().into_bytes()),
                                ).map_err(|_| UserConnectionError::InvalidMessage)?;
                                self.tx.send((leave_message, self.addr))
                                    .map_err(UserConnectionError::BroadcastError)?;
                                logger::log_system(&format!("{} has left the chat", chat_name));
                            }
                            break;
                        }
                    };
                }
                // Branch 2: Broadcast to other clients
                result = rx.recv() => {
                    match result {
                        Ok((msg, _src_addr)) => {
                            self.send_message_chunked(msg).await.map_err(UserConnectionError::IoError)?;
                        }
                        Err(e) => {
                            logger::log_error(&format!("Broadcast receive error for {}: {:?}", self.addr, e));
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn randomize_username(&self, username: &str) -> String {
        let mut rng = rand::thread_rng();
        let random_suffix: u32 = rng.gen_range(1000..9999);
        format!("{}_{}", username, random_suffix)
    }

    pub async fn process_message(
        &mut self,
        message: ChatMessage,
    ) -> Result<(), UserConnectionError> {
        match message.msg_type {
            MessageTypes::Join => {
                self.process_join(message.content_as_string()).await?;
            }
            MessageTypes::ChatMessage => {
                self.process_chat_message(message.content_as_string())
                    .await?;
            }
            MessageTypes::ListUsers => {
                self.process_list_users().await?;
            }
            _ => (),
        }
        Ok(())
    }

    pub async fn process_list_users(&mut self) -> Result<(), UserConnectionError> {
        let clients = self.connected_clients.clone();
        let clients = clients.read().await;
        let user_list = clients.iter().cloned().collect::<Vec<String>>().join("\n");
        let list_message =
            ChatMessage::try_new(MessageTypes::ListUsers, Some(user_list.into_bytes()))
                .map_err(|_| UserConnectionError::InvalidMessage)?;
        self.send_message_chunked(list_message)
            .await
            .map_err(UserConnectionError::IoError)?;
        Ok(())
    }

    pub async fn process_chat_message(
        &mut self,
        content: Option<String>,
    ) -> Result<(), UserConnectionError> {
        if content.is_none() {
            return Err(UserConnectionError::InvalidMessage);
        }
        let chat_content = content.unwrap();
        if let Some(chat_name) = &self.chat_name {
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

    pub async fn process_join(
        &mut self,
        username: Option<String>,
    ) -> Result<(), UserConnectionError> {
        if username.is_none() {
            return Err(UserConnectionError::InvalidMessage);
        }
        let connected_clients = self.connected_clients.clone();
        if let Some(content) = username {
            let mut clients = connected_clients.write().await;
            self.chat_name = if !clients.insert(content.clone()) {
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
                self.send_message_chunked(rename_message)
                    .await
                    .map_err(UserConnectionError::IoError)?;
                Some(new_name)
            } else {
                Some(content)
            }
        } else {
            return Err(UserConnectionError::InvalidMessage);
        }
        if let Some(chat_name) = &self.chat_name {
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
