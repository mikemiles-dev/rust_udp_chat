use crate::input::{self, ClientUserInput};
use chat_shared::input::UserInput;
use chat_shared::logger;
use chat_shared::message::{ChatMessage, ChatMessageError, MessageTypes};
use chat_shared::network::TcpMessageHandler;
use colored::Colorize;
use std::io::{self, Write};
use std::net::{AddrParseError, SocketAddr};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::sleep;

#[derive(Debug)]
pub enum ChatClientError {
    InvalidAddress,
    IoError,
    ChatMessageError,
}

impl From<AddrParseError> for ChatClientError {
    fn from(_: AddrParseError) -> Self {
        ChatClientError::InvalidAddress
    }
}

impl From<io::Error> for ChatClientError {
    fn from(_: io::Error) -> Self {
        ChatClientError::IoError
    }
}

impl From<ChatMessageError> for ChatClientError {
    fn from(_: ChatMessageError) -> Self {
        ChatClientError::ChatMessageError
    }
}

pub struct ChatClient {
    connection: TcpStream,
    server_addr: SocketAddr,
    chat_name: String,
    last_dm_sender: Option<String>,
}

impl ChatClient {
    pub async fn new(server_addr: &str, name: String) -> Result<Self, ChatClientError> {
        let server_addr: SocketAddr = server_addr.parse()?;
        let stream = TcpStream::connect(server_addr).await?;

        Ok(ChatClient {
            connection: stream,
            server_addr,
            chat_name: name,
            last_dm_sender: None,
        })
    }

    pub async fn join_server(&mut self) -> Result<(), ChatClientError> {
        let chat_message =
            ChatMessage::try_new(MessageTypes::Join, Some(self.chat_name.as_bytes().to_vec()))?;
        self.send_message_chunked(chat_message).await?;
        Ok(())
    }

    async fn reconnect(&mut self) -> Result<(), ChatClientError> {
        const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
        const MAX_BACKOFF: Duration = Duration::from_secs(60);
        const BACKOFF_MULTIPLIER: u32 = 2;

        // Explicitly shutdown the old connection before reconnecting
        let _ = self.connection.shutdown().await;

        // Give the server time to detect the closure and clean up
        sleep(Duration::from_millis(100)).await;

        let mut backoff = INITIAL_BACKOFF;
        let mut attempt = 1;

        loop {
            logger::log_info(&format!(
                "Attempting to reconnect to {} (attempt {})...",
                self.server_addr, attempt
            ));

            match TcpStream::connect(self.server_addr).await {
                Ok(stream) => {
                    self.connection = stream;
                    logger::log_success("Reconnected to server!");

                    // Rejoin the server with the same username
                    if let Err(e) = self.join_server().await {
                        logger::log_error(&format!("Failed to rejoin server: {:?}", e));
                        return Err(e);
                    }

                    return Ok(());
                }
                Err(e) => {
                    logger::log_warning(&format!(
                        "Reconnection attempt {} failed: {}. Retrying in {:?}...",
                        attempt, e, backoff
                    ));
                    sleep(backoff).await;

                    // Exponential backoff with cap
                    backoff =
                        std::cmp::min(backoff.saturating_mul(BACKOFF_MULTIPLIER), MAX_BACKOFF);
                    attempt += 1;
                }
            }
        }
    }

    fn get_message_content(&self, message: &ChatMessage, msg_type_name: &str) -> Option<String> {
        message.content_as_string().or_else(|| {
            logger::log_error(&format!("Received invalid UTF-8 {} message", msg_type_name));
            None
        })
    }

    async fn handle_message(&mut self, message: ChatMessage) {
        match message.msg_type {
            MessageTypes::Join => {
                if let Some(content) = self.get_message_content(&message, "join") {
                    logger::log_system(&format!("{} has joined the chat", content));
                }
            }
            MessageTypes::Leave => {
                if let Some(content) = self.get_message_content(&message, "leave") {
                    logger::log_system(&format!("{} has left the chat", content));
                }
            }
            MessageTypes::UserRename => {
                if let Some(content) = self.get_message_content(&message, "rename") {
                    logger::log_success(&format!("You have been renamed to '{}'", content));
                    self.chat_name = content;
                }
            }
            MessageTypes::ChatMessage => {
                if let Some(content) = self.get_message_content(&message, "chat") {
                    let should_display = content
                        .split_once(": ")
                        .is_none_or(|(username, _)| username != self.chat_name);

                    if should_display {
                        logger::log_chat(&content);
                    }
                }
            }
            MessageTypes::ListUsers => {
                if let Some(content) = self.get_message_content(&message, "list users") {
                    logger::log_info("Current users online:");
                    for user in content.lines() {
                        logger::log_info(&format!(" - {}", user));
                    }
                }
            }
            MessageTypes::DirectMessage => {
                if let Some(content) = self.get_message_content(&message, "dm")
                    && let Some((sender, rest)) = content.split_once('|')
                    && let Some((recipient, msg)) = rest.split_once('|')
                {
                    // Display if we are the recipient
                    if recipient == self.chat_name {
                        logger::log_warning(&format!("[DM from {}]: {}", sender, msg));
                        // Track the sender so we can reply with /r
                        self.last_dm_sender = Some(sender.to_string());
                    }
                    // Display if we are the sender (confirmation)
                    else if sender == self.chat_name {
                        logger::log_info(&format!("[DM to {}]: {}", recipient, msg));
                    }
                }
            }
            MessageTypes::Error => {
                if let Some(content) = self.get_message_content(&message, "error") {
                    logger::log_error(&content);
                }
            }
            _ => {
                logger::log_warning(&format!("Unknown message type: {:?}", message.msg_type));
            }
        }
    }

    async fn handle_user_input(
        &mut self,
        user_input: input::ClientUserInput,
    ) -> Result<(), ChatClientError> {
        match user_input {
            input::ClientUserInput::Message(msg) => {
                if msg.trim().is_empty() {
                    logger::log_error("Cannot send empty message");
                    return Ok(());
                }
                let message =
                    ChatMessage::try_new(MessageTypes::ChatMessage, Some(msg.into_bytes()))?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::DirectMessage {
                recipient,
                message: msg,
            } => {
                if msg.trim().is_empty() {
                    logger::log_error("Cannot send empty direct message");
                    return Ok(());
                }
                let dm_content = format!("{}|{}", recipient, msg);
                let message = ChatMessage::try_new(
                    MessageTypes::DirectMessage,
                    Some(dm_content.into_bytes()),
                )?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::Reply(msg) => {
                if msg.trim().is_empty() {
                    logger::log_error("Cannot send empty reply");
                    return Ok(());
                }
                if let Some(recipient) = &self.last_dm_sender {
                    let dm_content = format!("{}|{}", recipient, msg);
                    let message = ChatMessage::try_new(
                        MessageTypes::DirectMessage,
                        Some(dm_content.into_bytes()),
                    )?;
                    self.send_message_chunked(message).await?;
                    Ok(())
                } else {
                    logger::log_error("No one to reply to. Use /dm <username> <message> first.");
                    Ok(())
                }
            }
            input::ClientUserInput::Help => {
                logger::log_info("Available commands:");
                logger::log_info("  /help - Show this help message");
                logger::log_info("  /list - List all users");
                logger::log_info("  /dm <username> <message> - Send direct message");
                logger::log_info("  /r <message> - Reply to last direct message");
                logger::log_info("  /quit - Exit the chat");
                Ok(())
            }
            input::ClientUserInput::ListUsers => {
                let message = ChatMessage::try_new(MessageTypes::ListUsers, None)?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::ClientUserInput::Quit => Ok(()),
        }
    }

    fn display_prompt(&self) -> io::Result<()> {
        print!("{} ", self.chat_name.bright_cyan().bold());
        io::stdout().flush()
    }

    pub async fn run(&mut self) -> io::Result<()> {
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);
        self.display_prompt()?;

        loop {
            tokio::select! {
                result = self.read_message_chunked() => {
                    match result {
                        Ok(message) => {
                            self.handle_message(message).await;
                            self.display_prompt()?;
                        }
                        Err(chat_shared::network::TcpMessageHandlerError::IoError(_)) |
                        Err(chat_shared::network::TcpMessageHandlerError::Disconnect) => {
                            logger::log_warning("Disconnected from server");

                            // Attempt to reconnect with exponential backoff
                            match self.reconnect().await {
                                Ok(()) => {
                                    self.display_prompt()?;
                                }
                                Err(e) => {
                                    logger::log_error(&format!("Failed to reconnect: {:?}", e));
                                    return Err(io::Error::other("Reconnection failed"));
                                }
                            }
                        }
                    }
                }
                result = ClientUserInput::get_user_input::<_, ClientUserInput>(&mut reader) => {
                    match result {
                        Ok(input::ClientUserInput::Quit) => return Ok(()),
                        Ok(input::ClientUserInput::ListUsers) => {
                            let message = ChatMessage::try_new(MessageTypes::ListUsers, None)
                                .map_err(|e| io::Error::other(format!("Failed to create ListUsers message: {e:?}")))?;
                            self.send_message_chunked(message).await
                                .map_err(|e| io::Error::other(format!("Failed to send ListUsers message: {e:?}")))?;
                            self.display_prompt()?;
                        }
                        Ok(user_input) => {
                            if let Err(e) = self.handle_user_input(user_input).await {
                                logger::log_error(&format!("Error: {e:?}"));
                                self.display_prompt()?;
                            }
                        }
                        Err(e) => {
                            logger::log_error(&format!("Input error: {e:?}"));
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
