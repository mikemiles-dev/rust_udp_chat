use shared::logger;
use shared::message::ChatMessage;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::{env, io};
use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use tokio_rustls::TlsAcceptor;

mod completer;
mod input;
mod readline_helper;
mod user_connection;
use input::ServerUserInput;
use user_connection::{UserConnection, UserConnectionError};

#[derive(Debug, Clone)]
pub enum ServerCommand {
    Kick(String),
    Rename { old_name: String, new_name: String },
    Ban(IpAddr),
}

pub struct ChatServer {
    listener: TcpListener,
    broadcaster: broadcast::Sender<(ChatMessage, SocketAddr)>,
    server_commands: broadcast::Sender<ServerCommand>,
    connected_clients: Arc<RwLock<HashSet<String>>>,
    /// Maps username to their IP address
    user_ips: Arc<RwLock<HashMap<String, IpAddr>>>,
    /// Maps username to their status message
    user_statuses: Arc<RwLock<HashMap<String, String>>>,
    /// Set of banned IP addresses
    banned_ips: Arc<RwLock<HashSet<IpAddr>>>,
    max_clients: usize,
    active_connections: Arc<AtomicUsize>,
    tls_acceptor: Option<TlsAcceptor>,
}

impl ChatServer {
    async fn new(bind_addr: &str, max_clients: usize, tls_acceptor: Option<TlsAcceptor>) -> io::Result<Self> {
        let (tx, _rx) = broadcast::channel(max_clients * 16); // Allow message buffering
        let (cmd_tx, _cmd_rx) = broadcast::channel(100); // Server commands channel
        let listener = TcpListener::bind(bind_addr).await?;

        Ok(ChatServer {
            listener,
            broadcaster: tx,
            server_commands: cmd_tx,
            connected_clients: Arc::new(RwLock::new(HashSet::new())),
            user_ips: Arc::new(RwLock::new(HashMap::new())),
            user_statuses: Arc::new(RwLock::new(HashMap::new())),
            banned_ips: Arc::new(RwLock::new(HashSet::new())),
            max_clients,
            active_connections: Arc::new(AtomicUsize::new(0)),
            tls_acceptor,
        })
    }

    async fn run(&mut self) -> io::Result<()> {
        // Spawn readline handler in a blocking thread (if TTY available)
        let mut readline_rx = readline_helper::spawn_readline_handler();

        if readline_rx.is_none() {
            logger::log_info("Running in non-interactive mode (no TTY)");
            logger::log_info("Server commands disabled - use docker exec for admin tasks");
        }

        loop {
            tokio::select! {
                // Handle incoming client connections
                result = self.listener.accept() => {
                    match result {
                        Ok((socket, addr)) => {
                            // Check if IP is banned
                            let banned = self.banned_ips.read().await;
                            if banned.contains(&addr.ip()) {
                                logger::log_warning(&format!(
                                    "Rejected connection from banned IP: {}",
                                    addr.ip()
                                ));
                                drop(socket);
                                continue;
                            }
                            drop(banned);

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
                            let cmd_tx_clone = self.server_commands.clone();
                            let active_connections_clone = self.active_connections.clone();
                            let tls_acceptor = self.tls_acceptor.clone();
                            let connected_clients = self.connected_clients.clone();
                            let user_ips = self.user_ips.clone();
                            let user_statuses = self.user_statuses.clone();

                            tokio::spawn(async move {
                                // Wrap socket in TLS if configured
                                let result = if let Some(acceptor) = tls_acceptor {
                                    // Add timeout to TLS handshake to prevent hanging connections
                                    match tokio::time::timeout(
                                        std::time::Duration::from_secs(30),
                                        acceptor.accept(socket)
                                    ).await {
                                        Ok(Ok(tls_stream)) => {
                                            let mut client_connection =
                                                UserConnection::new_tls(tls_stream, addr, tx_clone, cmd_tx_clone, connected_clients, user_ips, user_statuses);
                                            client_connection.handle().await
                                        }
                                        Ok(Err(e)) => {
                                            logger::log_error(&format!("TLS handshake failed for {}: {:?}", addr, e));
                                            Err(UserConnectionError::IoError(io::Error::other("TLS handshake failed")))
                                        }
                                        Err(_) => {
                                            logger::log_error(&format!("TLS handshake timed out for {}", addr));
                                            Err(UserConnectionError::IoError(io::Error::other("TLS handshake timed out")))
                                        }
                                    }
                                } else {
                                    let mut client_connection =
                                        UserConnection::new(socket, addr, tx_clone, cmd_tx_clone, connected_clients, user_ips, user_statuses);
                                    client_connection.handle().await
                                };

                                if let Err(e) = result {
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
                // Handle server commands from readline (only if TTY available)
                Some(line) = async {
                    match &mut readline_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await, // Never resolves if no TTY
                    }
                } => {
                    match line {
                        Some(input_line) => {
                            match ServerUserInput::try_from(input_line.as_str()) {
                                Ok(ServerUserInput::Quit) => {
                                    logger::log_info("Server shutting down...");
                                    return Ok(());
                                }
                                Ok(ServerUserInput::ListUsers) => {
                                    self.handle_list_users().await;
                                }
                                Ok(ServerUserInput::Kick(username)) => {
                                    self.handle_kick(username).await;
                                }
                                Ok(ServerUserInput::Rename { old_name, new_name }) => {
                                    self.handle_rename(old_name, new_name).await;
                                }
                                Ok(ServerUserInput::Ban(username)) => {
                                    self.handle_ban_user(username).await;
                                }
                                Ok(ServerUserInput::BanIp(ip)) => {
                                    self.handle_ban_ip(ip).await;
                                }
                                Ok(ServerUserInput::Unban(ip)) => {
                                    self.handle_unban(ip).await;
                                }
                                Ok(ServerUserInput::BanList) => {
                                    self.handle_banlist().await;
                                }
                                Ok(ServerUserInput::Help) => {
                                    self.handle_help();
                                }
                                Err(_) => {
                                    logger::log_error("Invalid command. Type /help for available commands.");
                                }
                            }
                        }
                        None => {
                            // EOF from readline
                            logger::log_info("Server shutting down...");
                            return Ok(());
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

    async fn handle_kick(&self, username: String) {
        let clients = self.connected_clients.read().await;
        if clients.contains(&username) {
            drop(clients);
            // Send kick command to all connections - the matching one will disconnect
            if self.server_commands.send(ServerCommand::Kick(username.clone())).is_ok() {
                logger::log_warning(&format!("Kicking user: {}", username));
            }
        } else {
            logger::log_error(&format!("User '{}' not found", username));
        }
    }

    async fn handle_rename(&self, old_name: String, new_name: String) {
        let mut clients = self.connected_clients.write().await;

        // Check if the user to rename exists
        if !clients.contains(&old_name) {
            logger::log_error(&format!("User '{}' not found", old_name));
            return;
        }

        // Check if the new name is already taken
        if clients.contains(&new_name) {
            logger::log_error(&format!("Username '{}' is already taken", new_name));
            return;
        }

        // Validate new username
        if new_name.is_empty() || new_name.len() > 32 {
            logger::log_error("Invalid username length (1-32 characters)");
            return;
        }
        if !new_name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            logger::log_error("Invalid characters (only alphanumeric, underscore, hyphen allowed)");
            return;
        }

        // Update the connected_clients set
        clients.remove(&old_name);
        clients.insert(new_name.clone());
        drop(clients);

        // Send rename command to all connections - the matching one will handle it
        if self.server_commands.send(ServerCommand::Rename {
            old_name: old_name.clone(),
            new_name: new_name.clone(),
        }).is_ok() {
            logger::log_success(&format!("Renaming user '{}' to '{}'", old_name, new_name));
        }
    }

    async fn handle_ban_user(&self, username: String) {
        // Look up the user's IP
        let user_ips = self.user_ips.read().await;
        let ip = match user_ips.get(&username) {
            Some(ip) => *ip,
            None => {
                logger::log_error(&format!("User '{}' not found or not connected", username));
                return;
            }
        };
        drop(user_ips);

        // Add to banned IPs
        let mut banned = self.banned_ips.write().await;
        if banned.insert(ip) {
            drop(banned);
            logger::log_warning(&format!("Banned IP {} (user '{}')", ip, username));

            // Kick the user and disconnect them
            if self.server_commands.send(ServerCommand::Ban(ip)).is_ok() {
                logger::log_info(&format!("Disconnecting user '{}' from banned IP", username));
            }
        } else {
            logger::log_info(&format!("IP {} is already banned", ip));
        }
    }

    async fn handle_ban_ip(&self, ip: IpAddr) {
        let mut banned = self.banned_ips.write().await;
        if banned.insert(ip) {
            drop(banned);
            logger::log_warning(&format!("Banned IP {}", ip));

            // Disconnect any users from this IP
            if self.server_commands.send(ServerCommand::Ban(ip)).is_ok() {
                logger::log_info(&format!("Disconnecting users from banned IP {}", ip));
            }
        } else {
            logger::log_info(&format!("IP {} is already banned", ip));
        }
    }

    async fn handle_unban(&self, ip: IpAddr) {
        let mut banned = self.banned_ips.write().await;
        if banned.remove(&ip) {
            logger::log_success(&format!("Unbanned IP {}", ip));
        } else {
            logger::log_error(&format!("IP {} is not banned", ip));
        }
    }

    async fn handle_banlist(&self) {
        let banned = self.banned_ips.read().await;
        if banned.is_empty() {
            logger::log_info("No IPs are currently banned.");
        } else {
            logger::log_info(&format!("Banned IPs ({}):", banned.len()));
            for ip in banned.iter() {
                logger::log_info(&format!("  - {}", ip));
            }
        }
    }

    fn handle_help(&self) {
        logger::log_info("Available server commands:");
        logger::log_info("  /list                    - List all connected users");
        logger::log_info("  /kick <user>             - Kick a user from the server");
        logger::log_info("  /rename <user> <newname> - Rename a user");
        logger::log_info("  /ban <user|ip>           - Ban a user by name or IP address");
        logger::log_info("  /unban <ip>              - Unban an IP address");
        logger::log_info("  /banlist                 - List all banned IPs");
        logger::log_info("  /help                    - Show this help message");
        logger::log_info("  /quit                    - Shutdown the server");
    }
}

fn load_tls_config(cert_path: &str, key_path: &str) -> io::Result<ServerConfig> {
    let cert_file = File::open(cert_path)
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Certificate file not found: {}", e)))?;
    let key_file = File::open(key_path)
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("Key file not found: {}", e)))?;

    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);

    let certs = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid certificate: {}", e)))?;

    let key = private_key(&mut key_reader)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Invalid private key: {}", e)))?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No private key found"))?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("TLS config error: {}", e)))?;

    Ok(config)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    const CHAT_SERVER_ADDR_ENV_VAR: &str = "CHAT_SERVER_ADDR";
    const CHAT_SERVER_MAX_CLIENTS_ENV_VAR: &str = "CHAT_SERVER_MAX_CLIENTS";
    const TLS_CERT_PATH_ENV_VAR: &str = "TLS_CERT_PATH";
    const TLS_KEY_PATH_ENV_VAR: &str = "TLS_KEY_PATH";

    let chat_server_addr = env::var(CHAT_SERVER_ADDR_ENV_VAR).unwrap_or("0.0.0.0:8080".to_string());
    let max_clients = env::var(CHAT_SERVER_MAX_CLIENTS_ENV_VAR)
        .unwrap_or("100".to_string())
        .parse::<usize>()
        .unwrap_or(100);

    // Check if TLS is configured
    let tls_acceptor = match (env::var(TLS_CERT_PATH_ENV_VAR), env::var(TLS_KEY_PATH_ENV_VAR)) {
        (Ok(cert_path), Ok(key_path)) if Path::new(&cert_path).exists() && Path::new(&key_path).exists() => {
            logger::log_info("TLS enabled - loading certificates...");
            match load_tls_config(&cert_path, &key_path) {
                Ok(config) => {
                    logger::log_success("TLS certificates loaded successfully");
                    Some(TlsAcceptor::from(Arc::new(config)))
                }
                Err(e) => {
                    logger::log_error(&format!("Failed to load TLS config: {}", e));
                    logger::log_warning("Starting server WITHOUT TLS encryption");
                    None
                }
            }
        }
        _ => {
            logger::log_info("TLS not configured - running without encryption");
            logger::log_info(&format!("To enable TLS, set {} and {} environment variables", TLS_CERT_PATH_ENV_VAR, TLS_KEY_PATH_ENV_VAR));
            None
        }
    };

    let mut server = ChatServer::new(&chat_server_addr, max_clients, tls_acceptor).await?;

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
