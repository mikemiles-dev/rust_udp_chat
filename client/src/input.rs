use shared::commands::client as commands;
use shared::input::{UserInput, UserInputError};

#[derive(Debug)]
pub enum ClientUserInput {
    Help,
    ListUsers,
    Message(String),
    DirectMessage {
        recipient: String,
        message: String,
    },
    Reply(String),
    Rename(String),
    SendFile {
        recipient: String,
        file_path: String,
    },
    AcceptFile {
        sender: String,
    },
    RejectFile {
        sender: String,
    },
    Status(Option<String>),
    Quit,
}

impl UserInput for ClientUserInput {
    fn get_quit_command() -> Self {
        ClientUserInput::Quit
    }
}

impl TryFrom<&str> for ClientUserInput {
    type Error = UserInputError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = parts.first().copied().unwrap_or("");

        if commands::QUIT.matches(cmd) {
            Ok(ClientUserInput::Quit)
        } else if commands::LIST.matches(cmd) {
            Ok(ClientUserInput::ListUsers)
        } else if commands::HELP.matches(cmd) {
            Ok(ClientUserInput::Help)
        } else if commands::DM.matches(cmd) {
            if parts.len() < 3 {
                Err(UserInputError::InvalidCommand)
            } else {
                let recipient = parts[1].to_string();
                let message = parts[2..].join(" ");
                Ok(ClientUserInput::DirectMessage { recipient, message })
            }
        } else if commands::REPLY.matches(cmd) {
            if parts.len() < 2 {
                Err(UserInputError::InvalidCommand)
            } else {
                let message = parts[1..].join(" ");
                Ok(ClientUserInput::Reply(message))
            }
        } else if commands::RENAME.matches(cmd) {
            if parts.len() < 2 {
                Err(UserInputError::InvalidCommand)
            } else {
                let new_name = parts[1].to_string();
                Ok(ClientUserInput::Rename(new_name))
            }
        } else if commands::SEND.matches(cmd) {
            if parts.len() < 3 {
                Err(UserInputError::InvalidCommand)
            } else {
                let recipient = parts[1].to_string();
                let file_path = parts[2..].join(" ");
                Ok(ClientUserInput::SendFile {
                    recipient,
                    file_path,
                })
            }
        } else if commands::ACCEPT.matches(cmd) {
            if parts.len() < 2 {
                Err(UserInputError::InvalidCommand)
            } else {
                let sender = parts[1].to_string();
                Ok(ClientUserInput::AcceptFile { sender })
            }
        } else if commands::REJECT.matches(cmd) {
            if parts.len() < 2 {
                Err(UserInputError::InvalidCommand)
            } else {
                let sender = parts[1].to_string();
                Ok(ClientUserInput::RejectFile { sender })
            }
        } else if commands::STATUS.matches(cmd) {
            if parts.len() < 2 {
                // No status provided - clear status
                Ok(ClientUserInput::Status(None))
            } else {
                let status = parts[1..].join(" ");
                Ok(ClientUserInput::Status(Some(status)))
            }
        } else if trimmed.starts_with('/') {
            Err(UserInputError::InvalidCommand)
        } else {
            Ok(ClientUserInput::Message(trimmed.to_string()))
        }
    }
}

impl TryFrom<String> for ClientUserInput {
    type Error = UserInputError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quit_command() {
        let input = ClientUserInput::try_from("/quit");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ClientUserInput::Quit));
    }

    #[test]
    fn test_help_command() {
        let input = ClientUserInput::try_from("/help");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ClientUserInput::Help));
    }

    #[test]
    fn test_list_command() {
        let input = ClientUserInput::try_from("/list");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ClientUserInput::ListUsers));
    }

    #[test]
    fn test_dm_command_valid() {
        let input = ClientUserInput::try_from("/dm Alice Hello there!");
        assert!(input.is_ok());
        if let ClientUserInput::DirectMessage { recipient, message } = input.unwrap() {
            assert_eq!(recipient, "Alice");
            assert_eq!(message, "Hello there!");
        } else {
            panic!("Expected DirectMessage variant");
        }
    }

    #[test]
    fn test_dm_command_multiword_message() {
        let input = ClientUserInput::try_from("/dm Bob This is a longer message");
        assert!(input.is_ok());
        if let ClientUserInput::DirectMessage { recipient, message } = input.unwrap() {
            assert_eq!(recipient, "Bob");
            assert_eq!(message, "This is a longer message");
        } else {
            panic!("Expected DirectMessage variant");
        }
    }

    #[test]
    fn test_dm_command_missing_message() {
        let input = ClientUserInput::try_from("/dm Alice");
        assert!(input.is_err());
        assert!(matches!(input.unwrap_err(), UserInputError::InvalidCommand));
    }

    #[test]
    fn test_dm_command_missing_recipient() {
        let input = ClientUserInput::try_from("/dm");
        assert!(input.is_err());
        assert!(matches!(input.unwrap_err(), UserInputError::InvalidCommand));
    }

    #[test]
    fn test_reply_command_valid() {
        let input = ClientUserInput::try_from("/r Thanks!");
        assert!(input.is_ok());
        if let ClientUserInput::Reply(message) = input.unwrap() {
            assert_eq!(message, "Thanks!");
        } else {
            panic!("Expected Reply variant");
        }
    }

    #[test]
    fn test_reply_command_multiword() {
        let input = ClientUserInput::try_from("/r Got it, will do");
        assert!(input.is_ok());
        if let ClientUserInput::Reply(message) = input.unwrap() {
            assert_eq!(message, "Got it, will do");
        } else {
            panic!("Expected Reply variant");
        }
    }

    #[test]
    fn test_reply_command_missing_message() {
        let input = ClientUserInput::try_from("/r");
        assert!(input.is_err());
        assert!(matches!(input.unwrap_err(), UserInputError::InvalidCommand));
    }

    #[test]
    fn test_regular_message() {
        let input = ClientUserInput::try_from("Hello everyone!");
        assert!(input.is_ok());
        if let ClientUserInput::Message(msg) = input.unwrap() {
            assert_eq!(msg, "Hello everyone!");
        } else {
            panic!("Expected Message variant");
        }
    }

    #[test]
    fn test_invalid_command() {
        let input = ClientUserInput::try_from("/unknown");
        assert!(input.is_err());
        assert!(matches!(input.unwrap_err(), UserInputError::InvalidCommand));
    }

    #[test]
    fn test_whitespace_trimming() {
        let input = ClientUserInput::try_from("  /help  ");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ClientUserInput::Help));
    }

    #[test]
    fn test_message_with_leading_whitespace() {
        let input = ClientUserInput::try_from("  Hello  ");
        assert!(input.is_ok());
        if let ClientUserInput::Message(msg) = input.unwrap() {
            assert_eq!(msg, "Hello");
        } else {
            panic!("Expected Message variant");
        }
    }

    #[test]
    fn test_dm_with_extra_whitespace() {
        let input = ClientUserInput::try_from("/dm   Alice   Hello   World");
        assert!(input.is_ok());
        if let ClientUserInput::DirectMessage { recipient, message } = input.unwrap() {
            assert_eq!(recipient, "Alice");
            assert_eq!(message, "Hello World"); // Extra whitespace is normalized
        } else {
            panic!("Expected DirectMessage variant");
        }
    }

    #[test]
    fn test_status_command_with_message() {
        let input = ClientUserInput::try_from("/status AFK for lunch");
        assert!(input.is_ok());
        if let ClientUserInput::Status(Some(status)) = input.unwrap() {
            assert_eq!(status, "AFK for lunch");
        } else {
            panic!("Expected Status variant with message");
        }
    }

    #[test]
    fn test_status_command_clear() {
        let input = ClientUserInput::try_from("/status");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ClientUserInput::Status(None)));
    }
}
