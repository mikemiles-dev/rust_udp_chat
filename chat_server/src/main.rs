use std::io;
use tokio::net::UdpSocket;

use chat_shared::Message;

pub struct ChatServer {
    socket: UdpSocket,
}

impl ChatServer {
    async fn new(bind_addr: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        Ok(ChatServer { socket })
    }

    async fn run(&mut self) -> io::Result<()> {
        let mut buf = [0; 1024]; // Buffer to hold incoming data

        loop {
            tokio::select! {
                result = self.socket.recv_from(&mut buf) => {
                    let (len, addr) = result?;
                    println!(
                        "Received {} bytes from: {}",
                        len,
                        addr,
                    );

                    let message = match Message::try_from(&buf[..len]) {
                        Ok(msg) => msg,
                        Err(e) => {
                            eprintln!("Failed to parse message: {:?}", e);
                            continue;
                        }
                    };

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
                    }

                    // // Optional: Echo the data back to the sender
                    // let sent_len = self.socket.send_to(&buf[..len], addr).await?;
                    // println!("Sent {} bytes back to: {}", sent_len, addr);
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // 1. Bind the socket to a local address (e.g., all interfaces on port 8080)
    let mut server = ChatServer::new("0.0.0.0:8080").await?;
    println!("UDP Listener bound to 0.0.0.0:8080");

    server.run().await
}
