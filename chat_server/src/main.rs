use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{env, io};
use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};

use chat_shared::message::ChatMessage;

mod user_connection;
use user_connection::UserConnection;

pub struct ChatServer {
    listener: TcpListener,
    broadcaster: broadcast::Sender<(ChatMessage, SocketAddr)>,
    connected_clients: Arc<RwLock<HashSet<String>>>,
}

impl ChatServer {
    async fn new(bind_addr: &str, max_clients: usize) -> io::Result<Self> {
        let (tx, _rx) = broadcast::channel(max_clients);
        let listener = TcpListener::bind(bind_addr).await?;

        Ok(ChatServer {
            listener,
            broadcaster: tx,
            connected_clients: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    async fn run(&mut self) -> io::Result<()> {
        loop {
            let (socket, addr) = self.listener.accept().await?;
            let tx_clone = self.broadcaster.clone();

            let mut client_connection =
                UserConnection::new(socket, addr, tx_clone, self.connected_clients.clone());

            tokio::spawn(async move {
                if let Err(e) = client_connection.handle().await {
                    eprintln!("Error handling client {}: {:?}", addr, e);
                }
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
    println!("Chat Server Started at {}", chat_server_addr);
    println!(
        "To change the address, set the {} environment variable to change.",
        CHAT_SERVER_ADDR_ENV_VAR
    );
    println!(
        "To change the max clients, set the {} environment variable to change.",
        CHAT_SERVER_MAX_CLIENTS_ENV_VAR
    );

    server.run().await
}
