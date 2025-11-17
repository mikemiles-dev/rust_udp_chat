use crate::input;
use chat_shared::logger;
use chat_shared::message::{ChatMessage, ChatMessageError, MessageTypes};
use chat_shared::network::TcpMessageHandler;
use colored::Colorize;
use std::io::{self, Write};
use std::net::{AddrParseError, SocketAddr};
use tokio::net::TcpStream;

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
    chat_name: String,
    last_dm_sender: Option<String>,
}

impl ChatClient {
    pub async fn new(server_addr: &str, name: String) -> Result<Self, ChatClientError> {
        let server_addr: SocketAddr = server_addr.parse()?;
        let stream = TcpStream::connect(server_addr).await?;

        Ok(ChatClient {
            connection: stream,
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
                if let Some(content) = self.get_message_content(&message, "dm") {
                    if let Some((sender, rest)) = content.split_once('|') {
                        if let Some((recipient, msg)) = rest.split_once('|') {
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
        user_input: input::UserInput,
    ) -> Result<(), ChatClientError> {
        match user_input {
            input::UserInput::Message(msg) => {
                let message =
                    ChatMessage::try_new(MessageTypes::ChatMessage, Some(msg.into_bytes()))?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::UserInput::DirectMessage { recipient, message: msg } => {
                let dm_content = format!("{}|{}", recipient, msg);
                let message =
                    ChatMessage::try_new(MessageTypes::DirectMessage, Some(dm_content.into_bytes()))?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::UserInput::Reply(msg) => {
                if let Some(recipient) = &self.last_dm_sender {
                    let dm_content = format!("{}|{}", recipient, msg);
                    let message =
                        ChatMessage::try_new(MessageTypes::DirectMessage, Some(dm_content.into_bytes()))?;
                    self.send_message_chunked(message).await?;
                    Ok(())
                } else {
                    logger::log_error("No one to reply to. Use /dm <username> <message> first.");
                    Ok(())
                }
            }
            input::UserInput::Help => {
                logger::log_info("Available commands:");
                logger::log_info("  /help - Show this help message");
                logger::log_info("  /list - List all users");
                logger::log_info("  /dm <username> <message> - Send direct message");
                logger::log_info("  /r <message> - Reply to last direct message");
                logger::log_info("  /quit - Exit the chat");
                Ok(())
            }
            input::UserInput::ListUsers => {
                let message = ChatMessage::try_new(MessageTypes::ListUsers, None)?;
                self.send_message_chunked(message).await?;
                Ok(())
            }
            input::UserInput::Quit => Ok(()),
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
                        Err(chat_shared::network::TcpMessageHandlerError::IoError(e)) => {
                            logger::log_error(&format!("IO error: {:?}", e));
                            return Err(e);
                        }
                        Err(chat_shared::network::TcpMessageHandlerError::Disconnect) => {
                            logger::log_warning("Disconnected from server");
                            return Ok(());
                        }
                    }
                }
                result = input::get_user_input(&mut reader) => {
                    match result {
                        Ok(input::UserInput::Quit) => return Ok(()),
                        Ok(input::UserInput::ListUsers) => {
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
