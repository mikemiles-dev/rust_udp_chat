mod error;
mod handlers;
mod rate_limiting;

pub use error::UserConnectionError;
use handlers::MessageHandlers;
use rate_limiting::{RATE_LIMIT_MESSAGES, RATE_LIMIT_WINDOW, RateLimiter};

use crate::ServerCommand;
use shared::logger;
use shared::message::{ChatMessage, MessageTypes};
use shared::network::{TcpMessageHandler, TcpMessageHandlerError};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{RwLock, broadcast};
use tokio_rustls::server::TlsStream;

/// How often to send ping messages to clients
const PING_INTERVAL: Duration = Duration::from_secs(30);
/// How long to wait for a pong response before considering the client dead
const PONG_TIMEOUT: Duration = Duration::from_secs(60);

pub enum ConnectionStream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for ConnectionStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ConnectionStream::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
            ConnectionStream::Tls(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ConnectionStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            ConnectionStream::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            ConnectionStream::Tls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ConnectionStream::Plain(stream) => Pin::new(stream).poll_flush(cx),
            ConnectionStream::Tls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ConnectionStream::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            ConnectionStream::Tls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

pub struct UserConnection {
    socket: ConnectionStream,
    addr: SocketAddr,
    tx: broadcast::Sender<(ChatMessage, SocketAddr)>,
    server_commands: broadcast::Sender<ServerCommand>,
    connected_clients: Arc<RwLock<HashSet<String>>>,
    user_ips: Arc<RwLock<HashMap<String, IpAddr>>>,
    user_statuses: Arc<RwLock<HashMap<String, String>>>,
    user_sessions: Arc<RwLock<HashMap<String, String>>>,
    chat_name: Option<String>,
    rate_limiter: RateLimiter,
    /// True if user explicitly quit (vs connection drop which may be a reconnect)
    clear_status_on_disconnect: bool,
    /// True if session was taken over by a reconnecting client - don't clean up username
    session_taken_over: bool,
}

impl TcpMessageHandler for UserConnection {
    type Stream = ConnectionStream;
    fn get_stream(&mut self) -> &mut Self::Stream {
        &mut self.socket
    }
}

impl UserConnection {
    pub fn new(
        socket: TcpStream,
        addr: SocketAddr,
        tx: broadcast::Sender<(ChatMessage, SocketAddr)>,
        server_commands: broadcast::Sender<ServerCommand>,
        connected_clients: Arc<RwLock<HashSet<String>>>,
        user_ips: Arc<RwLock<HashMap<String, IpAddr>>>,
        user_statuses: Arc<RwLock<HashMap<String, String>>>,
        user_sessions: Arc<RwLock<HashMap<String, String>>>,
    ) -> Self {
        UserConnection {
            socket: ConnectionStream::Plain(socket),
            addr,
            tx,
            server_commands,
            connected_clients,
            user_ips,
            user_statuses,
            user_sessions,
            chat_name: None,
            rate_limiter: RateLimiter::new(RATE_LIMIT_MESSAGES, RATE_LIMIT_WINDOW),
            clear_status_on_disconnect: false,
            session_taken_over: false,
        }
    }

    pub fn new_tls(
        socket: TlsStream<TcpStream>,
        addr: SocketAddr,
        tx: broadcast::Sender<(ChatMessage, SocketAddr)>,
        server_commands: broadcast::Sender<ServerCommand>,
        connected_clients: Arc<RwLock<HashSet<String>>>,
        user_ips: Arc<RwLock<HashMap<String, IpAddr>>>,
        user_statuses: Arc<RwLock<HashMap<String, String>>>,
        user_sessions: Arc<RwLock<HashMap<String, String>>>,
    ) -> Self {
        UserConnection {
            socket: ConnectionStream::Tls(Box::new(socket)),
            addr,
            tx,
            server_commands,
            connected_clients,
            user_ips,
            user_statuses,
            user_sessions,
            chat_name: None,
            rate_limiter: RateLimiter::new(RATE_LIMIT_MESSAGES, RATE_LIMIT_WINDOW),
            clear_status_on_disconnect: false,
            session_taken_over: false,
        }
    }

    pub async fn handle(&mut self) -> Result<(), UserConnectionError> {
        logger::log_info(&format!("New client connected: {}", self.addr));

        let mut rx = self.tx.subscribe();
        let mut cmd_rx = self.server_commands.subscribe();

        // Heartbeat tracking
        let mut last_activity = Instant::now();
        let mut ping_interval = tokio::time::interval(PING_INTERVAL);
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Skip the first immediate tick - we don't want to ping right away
        ping_interval.tick().await;

        loop {
            tokio::select! {
                // Branch 1: Receive from client
                result = self.read_message_chunked() => {
                    match result {
                        Ok(msg) => {
                            // Update last activity on any message received
                            last_activity = Instant::now();

                            // Handle Pong silently (just updates last_activity above)
                            if msg.msg_type == MessageTypes::Pong {
                                continue;
                            }

                            match self.process_message(msg).await {
                                Ok(()) => {}
                                Err(UserConnectionError::ExplicitQuit) => {
                                    // User explicitly quit - clear status on disconnect
                                    self.clear_status_on_disconnect = true;
                                    break;
                                }
                                Err(UserConnectionError::VersionMismatch) => {
                                    // Version mismatch - disconnect client (error already sent)
                                    logger::log_warning(&format!("Client {} disconnected due to version mismatch", self.addr));
                                    break;
                                }
                                Err(e) => {
                                    logger::log_error(&format!("Error handling message from {}: {:?}", self.addr, e));
                                }
                            }
                        }
                        Err(TcpMessageHandlerError::IoError(e)) => {
                            logger::log_error(&format!("IO error reading from {}: {:?}", self.addr, e));
                            break;
                        }
                        Err(TcpMessageHandlerError::Disconnect) => {
                            logger::log_warning(&format!("Client {} disconnected", self.addr));
                            break;
                        }
                    };
                }
                // Branch 2: Broadcast to other clients
                result = rx.recv() => {
                    match result {
                        Ok((msg, _src_addr)) => {
                            if let Err(e) = self.send_message_chunked(msg).await {
                                logger::log_warning(&format!("Failed to send message to {}: {:?}", self.addr, e));
                                // Client likely disconnected, break to clean up
                                break;
                            }
                        }
                        Err(e) => {
                            logger::log_error(&format!("Broadcast receive error for {}: {:?}", self.addr, e));
                            break;
                        }
                    }
                }
                // Branch 3: Server commands (kick, rename, etc.)
                result = cmd_rx.recv() => {
                    match result {
                        Ok(ServerCommand::Kick(username)) => {
                            if let Some(chat_name) = &self.chat_name
                                && chat_name == &username {
                                logger::log_info(&format!("User {} kicked by server", chat_name));
                                // Send error message to client before disconnecting
                                if let Ok(kick_msg) = ChatMessage::try_new(
                                    MessageTypes::Error,
                                    Some("You have been kicked by the server".as_bytes().to_vec())
                                ) {
                                    let _ = self.send_message_chunked(kick_msg).await;
                                }
                                // Clear status when kicked
                                self.clear_status_on_disconnect = true;
                                break;
                            }
                        }
                        Ok(ServerCommand::Rename { old_name, new_name }) => {
                            if let Some(chat_name) = &self.chat_name
                                && chat_name == &old_name {
                                // Update user_ips mapping
                                let mut ips = self.user_ips.write().await;
                                if let Some(ip) = ips.remove(&old_name) {
                                    ips.insert(new_name.clone(), ip);
                                }
                                drop(ips);

                                // Update the local chat_name
                                self.chat_name = Some(new_name.clone());

                                // Send UserRename message to client
                                if let Ok(rename_msg) = ChatMessage::try_new(
                                    MessageTypes::UserRename,
                                    Some(new_name.clone().into_bytes())
                                ) {
                                    let _ = self.send_message_chunked(rename_msg).await;
                                }

                                logger::log_info(&format!("User {} renamed to {} by server", old_name, new_name));

                                // Broadcast announcement to all clients
                                let announcement = format!("{} is now known as {} (renamed by server)", old_name, new_name);
                                if let Ok(broadcast_msg) = ChatMessage::try_new(
                                    MessageTypes::ChatMessage,
                                    Some(announcement.into_bytes())
                                ) {
                                    let _ = self.tx.send((broadcast_msg, self.addr));
                                }
                            }
                        }
                        Ok(ServerCommand::Ban(ip)) => {
                            // Disconnect if our IP matches
                            if self.addr.ip() == ip {
                                logger::log_info(&format!("User {:?} banned (IP {})", self.chat_name, ip));
                                // Send error message to client before disconnecting
                                if let Ok(ban_msg) = ChatMessage::try_new(
                                    MessageTypes::Error,
                                    Some("You have been banned from the server".as_bytes().to_vec())
                                ) {
                                    let _ = self.send_message_chunked(ban_msg).await;
                                }
                                // Clear status when banned
                                self.clear_status_on_disconnect = true;
                                break;
                            }
                        }
                        Ok(ServerCommand::SessionTakeover(username)) => {
                            // Another connection is reclaiming this session
                            if let Some(chat_name) = &self.chat_name
                                && chat_name == &username {
                                logger::log_info(&format!(
                                    "Session for {} taken over by reconnecting client, closing old connection",
                                    chat_name
                                ));
                                // Mark session as taken over - don't clean up username/session on disconnect
                                self.session_taken_over = true;
                                break;
                            }
                        }
                        Err(_) => {
                            // Channel closed, ignore
                        }
                    }
                }
                // Branch 4: Periodic ping and timeout check
                _ = ping_interval.tick() => {
                    // Check if client has timed out (no activity for PONG_TIMEOUT)
                    if last_activity.elapsed() > PONG_TIMEOUT {
                        logger::log_warning(&format!(
                            "Client {} ({:?}) timed out - no response for {:?}",
                            self.addr,
                            self.chat_name,
                            last_activity.elapsed()
                        ));
                        break;
                    }

                    // Send ping to client
                    if let Ok(ping_msg) = ChatMessage::try_new(MessageTypes::Ping, None)
                        && let Err(e) = self.send_message_chunked(ping_msg).await
                    {
                        logger::log_warning(&format!("Failed to send ping to {}: {:?}", self.addr, e));
                        break;
                    }
                }
            }
        }

        // Cleanup on disconnect
        if let Some(chat_name) = &self.chat_name {
            // If session was taken over by a reconnecting client, don't clean up
            // The new connection now owns the username and session
            if self.session_taken_over {
                logger::log_info(&format!(
                    "Old connection for {} closed (session taken over)",
                    chat_name
                ));
                return Ok(());
            }

            let mut clients = self.connected_clients.write().await;
            clients.remove(chat_name);
            drop(clients);

            // Remove from user_ips mapping
            let mut ips = self.user_ips.write().await;
            ips.remove(chat_name);
            drop(ips);

            // Only remove status and session on explicit quit/kick/ban, not on connection drops
            // (which may be reconnection attempts)
            if self.clear_status_on_disconnect {
                let mut statuses = self.user_statuses.write().await;
                statuses.remove(chat_name);
                drop(statuses);

                let mut sessions = self.user_sessions.write().await;
                sessions.remove(chat_name);
                drop(sessions);
            }

            if let Ok(leave_message) =
                ChatMessage::try_new(MessageTypes::Leave, Some(chat_name.clone().into_bytes()))
            {
                let _ = self.tx.send((leave_message, self.addr));
            }
            logger::log_system(&format!("{} has left the chat", chat_name));
        }

        Ok(())
    }

    async fn process_message(&mut self, message: ChatMessage) -> Result<(), UserConnectionError> {
        let handlers = MessageHandlers {
            addr: self.addr,
            tx: &self.tx,
            server_commands: &self.server_commands,
            connected_clients: &self.connected_clients,
            user_ips: &self.user_ips,
            user_statuses: &self.user_statuses,
            user_sessions: &self.user_sessions,
        };

        handlers
            .process_message(
                message,
                &mut self.rate_limiter,
                &mut self.socket,
                &mut self.chat_name,
            )
            .await
    }
}
