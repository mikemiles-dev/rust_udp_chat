use std::collections::HashMap;
use std::net::SocketAddr;
use std::{env, io};
use tokio::net::UdpSocket;

use chat_shared::Message;

pub struct ConnectedClient {
    pub addr: String,
    pub name: String,
}

pub struct ChatServer {
    socket: UdpSocket,
    message_queue: HashMap<SocketAddr, Vec<Message>>,
}

impl ChatServer {
    async fn new(bind_addr: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        Ok(ChatServer {
            socket,
            message_queue: HashMap::new(),
        })
    }

    async fn process_message_queue(&mut self) -> io::Result<()> {
        for (addr, messages) in self.message_queue.iter_mut() {
            messages.sort_by(|a, b| a.id.cmp(&b.id));
            while let Some(message) = messages.pop() {
                Self::process_message(message, addr).await;
            }
        }
        Ok(())
    }

    async fn handle_message(&mut self, message: Message, src_addr: SocketAddr) {
        let entry = self.message_queue.entry(src_addr).or_insert_with(Vec::new);
        entry.push(message);
    }

    async fn process_message(message: Message, src_addr: &SocketAddr) {
        match message.msg_type {
            chat_shared::MessageTypes::Join => {
                let content = message.get_content().unwrap_or_default();
                println!("**[Join]** {} has joined the chat.", content);
            }
            chat_shared::MessageTypes::Leave => {
                let content = message.get_content().unwrap_or_default();
                println!("**[Leave]** {} has left the chat.", content);
            }
            chat_shared::MessageTypes::ChatMessage => {
                let content = message.get_content().unwrap_or_default();
                println!("**[Message]** {}", content);
            }
            _ => (),
        }
    }

    async fn aknowledge_message(
        socket: &UdpSocket,
        message_id: &Message,
        src_addr: &SocketAddr,
    ) -> io::Result<()> {
        let ack_message = Message::new(chat_shared::MessageTypes::Acknowledge, None, message_id.id);
        let ack_bytes: Vec<u8> = ack_message.try_into().unwrap();
        socket.send_to(&ack_bytes, src_addr).await?;
        Ok(())
    }

    async fn run(&mut self) -> io::Result<()> {
        let mut buf = [0; 1024]; // Buffer to hold incoming data
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(5));

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buf) => {
                    let (len, addr) = result?;

                    let message = match Message::try_from(&buf[..len]) {
                        Ok(msg) => msg,
                        Err(e) => {
                            eprintln!("Failed to parse message: {:?}", e);
                            continue;
                        }
                    };

                    self.handle_message(message, addr).await;

                    // // Optional: Echo the data back to the sender
                    // let sent_len = self.socket.send_to(&buf[..len], addr).await?;
                    // println!("Sent {} bytes back to: {}", sent_len, addr);
                }
                _ = interval.tick() => {
                    if let Err(e) = self.process_message_queue().await {
                        eprintln!("Error processing message queue: {}", e);
                    }
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
