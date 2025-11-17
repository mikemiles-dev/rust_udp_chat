use crate::{input, logger};
use chat_shared::message::{ChatMessage, ChatMessageError, MessageTypes};
use chat_shared::network::TcpMessageHandler;
use colored::Colorize;
use std::io::{self, Write};
use std::net::{AddrParseError, SocketAddr};
use tokio::net::TcpStream;

#[derive(Debug)]
pub enum ChatClientError {
    InvalidAddress(AddrParseError),
    IoError(io::Error),
    JoinError(String),
    TokioError(tokio::io::Error),
    ChatMessageError(ChatMessageError),
}

pub struct ChatClient {
    connection: TcpStream,
    chat_name: String,
}

impl ChatClient {
    pub async fn new(server_addr: &str, name: String) -> Result<Self, ChatClientError> {
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

    pub async fn join_server(&mut self) -> Result<(), ChatClientError> {
        let chat_message =
            ChatMessage::try_new(MessageTypes::Join, Some(self.chat_name.as_bytes().to_vec()))
                .map_err(ChatClientError::ChatMessageError)?;
        self.send_message_chunked(chat_message)
            .await
            .map_err(ChatClientError::TokioError)?;
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
                        .map_or(true, |(username, _)| username != self.chat_name);

                    if should_display {
                        logger::log_chat(&content);
                    }
                }
            }
            _ => {
                logger::log_warning(&format!("Unknown message type: {:?}", message.msg_type));
            }
        }
    }

    async fn handle_user_input(&mut self, user_input: input::UserInput) -> Result<(), ChatClientError> {
        match user_input {
            input::UserInput::Message(msg) => {
                let message = ChatMessage::try_new(MessageTypes::ChatMessage, Some(msg.into_bytes()))
                    .map_err(ChatClientError::ChatMessageError)?;
                self.send_message_chunked(message)
                    .await
                    .map_err(ChatClientError::TokioError)
            }
            input::UserInput::Help => {
                logger::log_info("Available commands:");
                logger::log_info("  /help - Show this help message");
                logger::log_info("  /quit - Exit the chat");
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
