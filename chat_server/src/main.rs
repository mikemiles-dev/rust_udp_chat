use std::net::SocketAddr;
use std::sync::Arc;
use std::{env, io};

use chat_shared::message::{ChatMessage, MessageTypes};
use chat_shared::udp_wrapper::UdpWrapper;

pub struct ConnectedClient {
    pub addr: String,
    pub name: String,
}

pub struct ChatServer {
    pub reliable_socket: Arc<UdpWrapper>,
}

impl ChatServer {
    async fn new(bind_addr: &str) -> io::Result<Self> {
        let reliable_socket = UdpWrapper::new(bind_addr)?;

        tokio::spawn(reliable_socket.clone().run_receiver_loop());
        tokio::spawn(reliable_socket.clone().run_retransmitter_loop());

        Ok(ChatServer { reliable_socket })
    }

    async fn process_message(&mut self, message: ChatMessage, src_addr: SocketAddr) {
        match message.msg_type {
            MessageTypes::Join => {
                let content = message.get_content().unwrap_or_default();
                println!("**[Join]** {} has joined the chat.", content);
            }
            MessageTypes::Leave => {
                let content = message.get_content().unwrap_or_default();
                println!("**[Leave]** {} has left the chat.", content);
            }
            MessageTypes::ChatMessage => {
                let content = message.get_content().unwrap_or_default();
                println!("**[Message]** {} says: {}", src_addr, content);
            }
            MessageTypes::UserRename => {
                let content = message.get_content().unwrap_or_default();
                println!(
                    "**[Rename]** {} has changed their name to {}.",
                    src_addr, content
                );
            }
            _ => (),
        }
    }

    async fn run(&mut self) -> io::Result<()> {
        loop {
            if let Some(peer_addr) = self.reliable_socket.get_first_peer_addr().await {
                while let Some(msg) = self.reliable_socket.poll_ready_message(&peer_addr).await {
                    println!("[SERVER] DELIVERED: {}", String::from_utf8_lossy(&msg));
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    const CHAT_SERVER_ADDR_ENV_VAR: &str = "CHAT_SERVER_ADDR";
    let chat_server_addr = env::var(CHAT_SERVER_ADDR_ENV_VAR).unwrap_or("0.0.0.0:8080".to_string());
    let mut server = ChatServer::new(&chat_server_addr).await?;
    println!("Chat Server Started at {}", chat_server_addr);
    println!(
        "To change the address, set the {} environment variable to change.",
        CHAT_SERVER_ADDR_ENV_VAR
    );

    server.run().await
}
