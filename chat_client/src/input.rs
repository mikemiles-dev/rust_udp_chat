use chat_shared::input::{UserInput, UserInputError};

#[derive(Debug)]
pub enum ClientUserInput {
    Help,
    ListUsers,
    Message(String),
    DirectMessage { recipient: String, message: String },
    Reply(String),
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

        match parts.first().copied().unwrap_or("") {
            "/quit" => Ok(ClientUserInput::Quit),
            "/list" => Ok(ClientUserInput::ListUsers),
            "/help" => Ok(ClientUserInput::Help),
            "/dm" => {
                if parts.len() < 3 {
                    Err(UserInputError::InvalidCommand)
                } else {
                    let recipient = parts[1].to_string();
                    let message = parts[2..].join(" ");
                    Ok(ClientUserInput::DirectMessage { recipient, message })
                }
            }
            "/r" => {
                if parts.len() < 2 {
                    Err(UserInputError::InvalidCommand)
                } else {
                    let message = parts[1..].join(" ");
                    Ok(ClientUserInput::Reply(message))
                }
            }
            _ => {
                if trimmed.starts_with('/') {
                    Err(UserInputError::InvalidCommand)
                } else {
                    Ok(ClientUserInput::Message(trimmed.to_string()))
                }
            }
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
}
