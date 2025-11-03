use std::io::{self, Write};
use tokio::io::AsyncBufReadExt;
use tokio::net::UdpSocket;

use chat_shared::{Message, MessageError, MessageTypes};

struct ChatClient {
    socket: UdpSocket,
    name: String,
    server_addr: String,
    sent_messages: Vec<SentMessage>,
    message_id_counter: u8,
}

pub struct SentMessage {
    pub id: u8,
    pub length: usize,
}

#[derive(Debug)]
pub enum ChatClientError {
    IoError(io::Error),
    JoinError(String),
    MessageError(MessageError),
}

impl ChatClient {
    async fn new(server_addr: &str, name: String) -> io::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        Ok(ChatClient {
            socket,
            name,
            server_addr: server_addr.to_string(),
            sent_messages: vec![],
            message_id_counter: 0,
        })
    }

    async fn increment_message_id(&mut self) {
        self.message_id_counter = self.message_id_counter.checked_add(1).unwrap_or(0);
    }

    async fn send_message(
        &mut self,
        message_type: MessageTypes,
        content: &str,
    ) -> Result<(), ChatClientError> {
        self.increment_message_id().await;
        let message = Message::new(
            message_type,
            Some(content.to_string()),
            self.message_id_counter,
        );
        let message_bytes: Vec<u8> = message
            .clone()
            .try_into()
            .map_err(ChatClientError::MessageError)?;
        let sent_length = self
            .socket
            .send_to(&message_bytes, &self.server_addr)
            .await
            .map_err(ChatClientError::IoError)?;
        println!(
            "Sent message of {} bytes to {}: {}",
            sent_length,
            self.server_addr,
            message_bytes.len()
        );
        if sent_length != message_bytes.len() {
            Err(ChatClientError::JoinError(
                format!(
                    "Warning: Sent length {} does not match message length {}",
                    sent_length,
                    message_bytes.len(),
                )
                .to_string(),
            ))
        } else {
            self.sent_messages.push(SentMessage {
                id: message.id,
                length: message.length,
            });
            Ok(())
        }
    }

    async fn join_server(&mut self) -> Result<(), ChatClientError> {
        let message = self.name.to_string();
        self.send_message(MessageTypes::Join, &message).await
    }

    async fn get_user_input() -> Option<String> {
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);
        let mut input_line = String::new();

        match reader.read_line(&mut input_line).await {
            Ok(0) => {
                // EOF (e.g., Ctrl+D or stream closed)
                println!("**[Input]** EOF received. Exiting...");
            }
            Ok(_) => {
                let trimmed_input = input_line.trim();
                println!("**[Input]** You typed: {}", trimmed_input);

                if trimmed_input.eq_ignore_ascii_case("quit") {
                    println!("**[Input]** Quitting application.");
                    return None;
                }
                // IMPORTANT: Clear the buffer for the next read
                input_line.clear();
            }
            Err(e) => {
                eprintln!("Input error: {}", e);
            }
        }
        Some(input_line)
    }

    async fn listen_for_messages(&mut self) -> Result<(), ChatClientError> {
        let mut buf = [0; 1024];
        loop {
            let (len, addr) = self
                .socket
                .recv_from(&mut buf)
                .await
                .map_err(ChatClientError::IoError)?;

            let message = Message::try_from(&buf[..len]).map_err(ChatClientError::MessageError)?;

            println!(
                "**[UDP]** Received {} bytes from {}: {:?}",
                len, addr, message
            );
        }
    }

    async fn run(&mut self) -> io::Result<()> {
        loop {
            // 3. Use tokio::select! to concurrently wait for either operation
            tokio::select! {
                // Branch 1: UDP Socket Receive
                result = self.listen_for_messages() => {
                    if let Err(e) = result {
                        eprintln!("Error receiving UDP message: {:?}", e);
                    }
                }

                // Branch 2: User Input
                result = ChatClient::get_user_input() => {
                    if let Some(input_line) = result {
                        let trimmed_input = input_line.trim();
                        if !trimmed_input.is_empty() {
                            if let Err(e) = self.send_message(MessageTypes::ChatMessage, trimmed_input).await {
                                eprintln!("Error sending message: {:?}", e);
                            }
                        }
                    } else {
                        // User chose to quit
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (chat_server, chat_name) = prompt_server_info()?;
    let mut client = ChatClient::new(&chat_server, chat_name).await?;

    client
        .join_server()
        .await
        .unwrap_or_else(|_| panic!("Could not connect to: {}", chat_server));

    client.run().await
}

fn prompt_server_info() -> io::Result<(String, String)> {
    let server_default = "127.0.0.1:8080";
    let name_default = "Guest";
    let mut chat_server = String::new();
    let mut chat_name = String::new();
    println!("Press Enter Chat Server (default: {}):", server_default);
    io::stdout().flush()?;
    io::stdin().read_line(&mut chat_server)?;
    let chat_server = chat_server.trim();
    println!("Press Enter Chat Name (default: {}):", name_default);
    io::stdout().flush()?;
    io::stdin().read_line(&mut chat_name)?;
    let chat_name = chat_name.trim();
    let chat_server = if chat_server.is_empty() {
        server_default.to_string()
    } else {
        chat_server.to_string()
    };
    let chat_name = if chat_name.is_empty() {
        name_default.to_string()
    } else {
        chat_name.to_string()
    };
    Ok((chat_server, chat_name))
}
