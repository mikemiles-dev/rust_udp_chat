use chat_shared::input::UserInput;
use chat_shared::logger;
use chat_shared::message::ChatMessage;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::{env, io};
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};

mod input;
mod user_connection;
use input::ServerUserInput;
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
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);

        loop {
            tokio::select! {
                // Handle incoming client connections
                result = self.listener.accept() => {
                    match result {
                        Ok((socket, addr)) => {
                            // Check connection limit
                            let current_connections = self.active_connections.load(Ordering::Relaxed);
                            if current_connections >= self.max_clients {
                                logger::log_warning(&format!(
                                    "Connection limit reached ({}/{}), rejecting connection from {}",
                                    current_connections, self.max_clients, addr
                                ));
                                continue;
                            }

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
                        Err(e) => {
                            logger::log_error(&format!("Failed to accept connection: {:?}", e));
                        }
                    }
                }
                // Handle server commands from stdin
                result = ServerUserInput::get_user_input::<_, ServerUserInput>(&mut reader) => {
                    match result {
                        Ok(ServerUserInput::Quit) => {
                            logger::log_info("Server shutting down...");
                            return Ok(());
                        }
                        Ok(ServerUserInput::ListUsers) => {
                            self.handle_list_users().await;
                        }
                        Ok(ServerUserInput::Help) => {
                            self.handle_help();
                        }
                        Err(_) => {
                            logger::log_error("Invalid command. Type /help for available commands.");
                        }
                    }
                }
            }
        }
    }

    async fn handle_list_users(&self) {
        let clients = self.connected_clients.read().await;
        let count = clients.len();
        if count == 0 {
            logger::log_info("No users currently connected.");
        } else {
            logger::log_info(&format!("Connected users ({}):", count));
            for user in clients.iter() {
                logger::log_info(&format!("  - {}", user));
            }
        }
    }

    fn handle_help(&self) {
        logger::log_info("Available server commands:");
        logger::log_info("  /list    - List all connected users");
        logger::log_info("  /help    - Show this help message");
        logger::log_info("  /quit    - Shutdown the server");
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
    logger::log_info("Server commands: /help, /list, /quit");

    server.run().await
}
