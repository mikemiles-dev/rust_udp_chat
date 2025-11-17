use chat_shared::logger;
use chat_shared::message::ChatMessage;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::{env, io};
use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};

mod user_connection;
use user_connection::UserConnection;

pub struct ChatServer {
    listener: TcpListener,
    broadcaster: broadcast::Sender<(ChatMessage, SocketAddr)>,
    connected_clients: Arc<RwLock<HashSet<String>>>,
    max_clients: usize,
    active_connections: Arc<AtomicUsize>,
}

impl ChatServer {
    async fn new(bind_addr: &str, max_clients: usize) -> io::Result<Self> {
        let (tx, _rx) = broadcast::channel(max_clients * 16); // Allow message buffering
        let listener = TcpListener::bind(bind_addr).await?;

        Ok(ChatServer {
            listener,
            broadcaster: tx,
            connected_clients: Arc::new(RwLock::new(HashSet::new())),
            max_clients,
            active_connections: Arc::new(AtomicUsize::new(0)),
        })
    }

    async fn run(&mut self) -> io::Result<()> {
        loop {
            // Check connection limit before accepting
            let current_connections = self.active_connections.load(Ordering::Relaxed);
            if current_connections >= self.max_clients {
                logger::log_warning(&format!(
                    "Connection limit reached ({}/{}), waiting...",
                    current_connections, self.max_clients
                ));
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                continue;
            }

            let (socket, addr) = self.listener.accept().await?;

            // Increment connection count
            self.active_connections.fetch_add(1, Ordering::Relaxed);

            let tx_clone = self.broadcaster.clone();
            let active_connections_clone = self.active_connections.clone();

            let mut client_connection =
                UserConnection::new(socket, addr, tx_clone, self.connected_clients.clone());

            tokio::spawn(async move {
                if let Err(e) = client_connection.handle().await {
                    logger::log_error(&format!("Error handling client {}: {:?}", addr, e));
                }

                // Decrement connection count when done
                active_connections_clone.fetch_sub(1, Ordering::Relaxed);
                logger::log_info(&format!("Connection from {} closed", addr));
            });
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    const CHAT_SERVER_ADDR_ENV_VAR: &str = "CHAT_SERVER_ADDR";
    const CHAT_SERVER_MAX_CLIENTS_ENV_VAR: &str = "CHAT_SERVER_MAX_CLIENTS";
    let chat_server_addr = env::var(CHAT_SERVER_ADDR_ENV_VAR).unwrap_or("0.0.0.0:8080".to_string());
    let max_clients = env::var(CHAT_SERVER_MAX_CLIENTS_ENV_VAR)
        .unwrap_or("100".to_string())
        .parse::<usize>()
        .unwrap_or(100);
    let mut server = ChatServer::new(&chat_server_addr, max_clients).await?;
    logger::log_success(&format!("Chat Server started at {}", chat_server_addr));
    logger::log_info(&format!(
        "To change address, set {} environment variable",
        CHAT_SERVER_ADDR_ENV_VAR
    ));
    logger::log_info(&format!(
        "To change max clients, set {} environment variable",
        CHAT_SERVER_MAX_CLIENTS_ENV_VAR
    ));

    server.run().await
}
