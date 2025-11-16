use chat_shared::network::TcpMessageHandler;
use std::collections::HashSet;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, broadcast};

use chat_shared::message::{ChatMessage, MessageTypes};
use chat_shared::network::TcpMessageHandlerError;
use rand::Rng;

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
        println!("New client connected: {}", self.addr);

        let mut rx = self.tx.subscribe();

        loop {
            tokio::select! {
                // Branch 1: Receive from client
                result = self.read_message_chunked() => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.process_message(msg).await {
                                eprintln!("Error handling message from {}: {:?}", self.addr, e);
                            }
                        }
                        Err(TcpMessageHandlerError::IoError(e)) => {
                            eprintln!("IO error reading from {}: {:?}", self.addr, e);
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
                                println!(">>> User '{}' has left the chat.", chat_name);
                            }
                            break;
                        }
                    };
                }
                // Branch 2: Broadcast to other clients
                result = rx.recv() => {
                    match result {
                        Ok((msg, src_addr)) => {
                            // Avoid sending the message back to the sender
                            if src_addr != self.addr {
                                self.send_message_chunked(msg).await.map_err(UserConnectionError::IoError)?;
                            }
                        }
                        Err(e) => {
                            eprintln!("Broadcast receive error for {}: {:?}", self.addr, e);
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
            _ => (),
        }
        Ok(())
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
                eprintln!(">>> User '{}' already exists. Renaming...", content);
                let new_name = self.randomize_username(&content);
                if !clients.insert(new_name.clone()) {
                    eprintln!(">>> Failed to assign random username to '{}'.", content);
                    return Err(UserConnectionError::JoinError);
                }
                println!(">>> User '{}' renamed to '{}'.", content, new_name);
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
            println!(">>> User '{}' has joined the chat.", chat_name);
        }
        Ok(())
    }
}
