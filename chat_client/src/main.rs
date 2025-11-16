use chat_shared::network::TcpMessageHandler;
use std::io::{self, Write};
use std::net::{AddrParseError, SocketAddr};

use chat_shared::message::{ChatMessage, ChatMessageError, MessageTypes};
use tokio::net::TcpStream;

// Centralized logging module
mod logger {
    use chrono::Local;
    use colored::Colorize;

    fn get_timestamp() -> String {
        Local::now().format("%H:%M:%S").to_string()
    }

    pub fn log_info(message: &str) {
        println!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[INFO]".cyan().bold(),
            message
        );
    }

    pub fn log_success(message: &str) {
        println!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[OK]".green().bold(),
            message
        );
    }

    pub fn log_error(message: &str) {
        eprintln!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[ERROR]".red().bold(),
            message
        );
    }

    pub fn log_warning(message: &str) {
        println!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[WARN]".yellow().bold(),
            message
        );
    }

    pub fn log_system(message: &str) {
        println!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[SYSTEM]".magenta().bold(),
            message
        );
    }

    pub fn log_chat(message: &str) {
        // Try to parse username from message format "username: message"
        if let Some((username, msg)) = message.split_once(": ") {
            let colored_username = colorize_username(username);
            println!(
                "{} {} {}: {}",
                format!("[{}]", get_timestamp()).dimmed(),
                "[CHAT]".white().bold(),
                colored_username,
                msg
            );
        } else {
            // Fallback if format doesn't match
            println!(
                "{} {} {}",
                format!("[{}]", get_timestamp()).dimmed(),
                "[CHAT]".white().bold(),
                message
            );
        }
    }

    fn colorize_username(username: &str) -> colored::ColoredString {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Generate a consistent hash for the username
        let mut hasher = DefaultHasher::new();
        username.hash(&mut hasher);
        let hash = hasher.finish();

        // Use the hash to select a color
        let colors = [
            colored::Color::Red,
            colored::Color::Green,
            colored::Color::Yellow,
            colored::Color::Blue,
            colored::Color::Magenta,
            colored::Color::Cyan,
            colored::Color::BrightRed,
            colored::Color::BrightGreen,
            colored::Color::BrightYellow,
            colored::Color::BrightBlue,
            colored::Color::BrightMagenta,
            colored::Color::BrightCyan,
        ];

        let color_index = (hash as usize) % colors.len();
        username.color(colors[color_index]).bold()
    }

    pub fn _log_debug(message: &str) {
        println!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[DEBUG]".blue().bold(),
            message
        );
    }
}

struct ChatClient {
    connection: TcpStream,
    chat_name: String,
}

#[derive(Debug)]
pub enum ChatClientError {
    InvalidAddress(AddrParseError),
    IoError(io::Error),
    JoinError(String),
    TokioError(tokio::io::Error),
    ChatMessageError(ChatMessageError),
}

pub enum UserInput {
    Message(String),
    Quit,
}

#[derive(Debug)]
pub enum UserInputError {
    InvalidCommand,
    IoError(io::Error),
}

impl TryFrom<&str> for UserInput {
    type Error = UserInputError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        let command = trimmed
            .split(" ")
            .next()
            .ok_or(UserInputError::InvalidCommand)?;
        match command {
            "/quit" => Ok(UserInput::Quit),
            _ => Ok(UserInput::Message(trimmed.to_string())),
        }
    }
}

impl ChatClient {
    async fn new(server_addr: &str, name: String) -> Result<Self, ChatClientError> {
        let server_addr: SocketAddr = server_addr
            .parse()
            .map_err(ChatClientError::InvalidAddress)?;

        let stream = TcpStream::connect(server_addr)
            .await
            .map_err(ChatClientError::IoError)?;

        Ok(ChatClient {
            connection: stream,
            chat_name: name,
        })
    }

    async fn join_server(&mut self) -> Result<(), ChatClientError> {
        let chat_message =
            ChatMessage::try_new(MessageTypes::Join, Some(self.chat_name.as_bytes().to_vec()))
                .map_err(ChatClientError::ChatMessageError)?;
        self.send_message_chunked(chat_message)
            .await
            .map_err(ChatClientError::TokioError)?;
        Ok(())
    }

    async fn get_user_input<R>(reader: &mut R) -> Result<UserInput, UserInputError>
    where
        R: tokio::io::AsyncBufReadExt + Unpin,
    {
        let mut input_line = String::new();

        match reader.read_line(&mut input_line).await {
            Ok(0) => Ok(UserInput::Quit),
            Ok(_) => UserInput::try_from(input_line.as_str()),
            Err(e) => Err(UserInputError::IoError(e)),
        }
    }

    async fn handle_message(&mut self, message: ChatMessage) {
        match message.msg_type {
            MessageTypes::Join => {
                if let Some(content) = message.content_as_string() {
                    logger::log_system(&format!("{} has joined the chat", content));
                } else {
                    logger::log_error("Received invalid UTF-8 join message");
                }
            }
            MessageTypes::Leave => {
                if let Some(content) = message.content_as_string() {
                    logger::log_system(&format!("{} has left the chat", content));
                } else {
                    logger::log_error("Received invalid UTF-8 leave message");
                }
            }
            MessageTypes::UserRename => {
                if let Some(content) = message.content_as_string() {
                    logger::log_success(&format!("You have been renamed to '{}'", content));
                    self.chat_name = content;
                } else {
                    logger::log_error("Received invalid UTF-8 rename message");
                }
            }
            MessageTypes::ChatMessage => {
                if let Some(content) = message.content_as_string() {
                    // Check if message is from current user
                    if let Some((username, _)) = content.split_once(": ") {
                        // Only display if it's not from ourselves
                        if username != self.chat_name {
                            logger::log_chat(&content);
                        }
                    } else {
                        // Fallback: display if format doesn't match
                        logger::log_chat(&content);
                    }
                } else {
                    logger::log_error("Received invalid UTF-8 chat message");
                }
            }
            _ => {
                logger::log_warning(&format!(
                    "Received unknown message type: {:?}",
                    message.msg_type
                ));
            }
        }
    }

    async fn handle_user_input(&mut self, user_input: UserInput) -> Result<(), ChatClientError> {
        match user_input {
            UserInput::Message(msg) => {
                let chat_message =
                    ChatMessage::try_new(MessageTypes::ChatMessage, Some(msg.into_bytes()))
                        .map_err(ChatClientError::ChatMessageError)?;
                self.send_message_chunked(chat_message)
                    .await
                    .map_err(ChatClientError::TokioError)?;
                Ok(())
            }
            UserInput::Quit => {
                logger::log_info("Quitting chat");
                Ok(())
            }
        }
    }

    fn display_prompt(&self) -> io::Result<()> {
        use colored::Colorize;
        print!("{} ", self.chat_name.bright_cyan().bold());
        io::stdout().flush()
    }

    async fn run(&mut self) -> io::Result<()> {
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);

        // Display initial prompt
        self.display_prompt()?;

        loop {
            // 3. Use tokio::select! to concurrently wait for either operation
            tokio::select! {
                // Branch 1: Receive
                result = self.read_message_chunked() => {
                    match result {
                        Ok(message) => {
                            self.handle_message(message).await;
                            // Redisplay prompt after receiving a message
                            self.display_prompt()?;
                        }
                        Err(chat_shared::network::TcpMessageHandlerError::IoError(e)) => {
                            logger::log_error(&format!("IO error reading from server: {:?}", e));
                            return Err(e);
                        }
                        Err(chat_shared::network::TcpMessageHandlerError::Disconnect) => {
                            logger::log_warning("Disconnected from server");
                            return Ok(());
                        }
                    }
                }
                // Branch 2: User Input
                result = ChatClient::get_user_input(&mut reader) => {
                    match result {
                        Ok(UserInput::Quit) => {
                            logger::log_info("Quitting chat");
                            return Ok(());
                        }
                        Ok(user_input) => {
                            if let Err(e) = self.handle_user_input(user_input).await {
                                logger::log_error(&format!("Error handling input: {e:?}"));
                                // Only display prompt after error
                                self.display_prompt()?;
                            }
                        }
                        Err(e) => {
                            logger::log_error(&format!("Input error: {e:?}"));
                            // Display prompt again after error
                            self.display_prompt()?;
                        }
                    }
                }
            }
        }
    }
}

impl TcpMessageHandler for ChatClient {
    fn get_stream(&mut self) -> &mut tokio::net::TcpStream {
        &mut self.connection
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (chat_server, chat_name) = prompt_server_info()?;
    let mut client = ChatClient::new(&chat_server, chat_name)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Failed to create ChatClient for server {}: {:?}",
                chat_server, e
            )
        });

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
    logger::log_info(&format!("Enter Chat Server (default: {}):", server_default));
    io::stdout().flush()?;
    io::stdin().read_line(&mut chat_server)?;
    let chat_server = chat_server.trim();
    logger::log_info(&format!("Enter Chat Name (default: {}):", name_default));
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
