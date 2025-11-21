use shared::input::{UserInput, UserInputError};

use std::net::IpAddr;

#[derive(Debug)]
pub enum ServerUserInput {
    Help,
    ListUsers,
    Kick(String),
    Rename { old_name: String, new_name: String },
    Ban(String),      // Ban by username (will resolve to IP)
    BanIp(IpAddr),    // Ban by IP directly
    Unban(IpAddr),    // Unban by IP
    BanList,          // List all banned IPs
    Quit,
}

impl UserInput for ServerUserInput {
    fn get_quit_command() -> Self {
        ServerUserInput::Quit
    }
}

impl TryFrom<&str> for ServerUserInput {
    type Error = UserInputError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();

        match trimmed {
            "/quit" | "/q" => Ok(ServerUserInput::Quit),
            "/list" => Ok(ServerUserInput::ListUsers),
            "/help" | "/h" => Ok(ServerUserInput::Help),
            _ => {
                if let Some(username) = trimmed.strip_prefix("/kick ") {
                    let username = username.trim();
                    if username.is_empty() {
                        Err(UserInputError::InvalidCommand)
                    } else {
                        Ok(ServerUserInput::Kick(username.to_string()))
                    }
                } else if let Some(args) = trimmed.strip_prefix("/rename ") {
                    let parts: Vec<&str> = args.split_whitespace().collect();
                    if parts.len() != 2 {
                        Err(UserInputError::InvalidCommand)
                    } else {
                        Ok(ServerUserInput::Rename {
                            old_name: parts[0].to_string(),
                            new_name: parts[1].to_string(),
                        })
                    }
                } else if let Some(target) = trimmed.strip_prefix("/ban ") {
                    let target = target.trim();
                    if target.is_empty() {
                        Err(UserInputError::InvalidCommand)
                    } else if let Ok(ip) = target.parse::<IpAddr>() {
                        // It's an IP address
                        Ok(ServerUserInput::BanIp(ip))
                    } else {
                        // It's a username
                        Ok(ServerUserInput::Ban(target.to_string()))
                    }
                } else if let Some(ip_str) = trimmed.strip_prefix("/unban ") {
                    let ip_str = ip_str.trim();
                    if let Ok(ip) = ip_str.parse::<IpAddr>() {
                        Ok(ServerUserInput::Unban(ip))
                    } else {
                        Err(UserInputError::InvalidCommand)
                    }
                } else if trimmed == "/banlist" {
                    Ok(ServerUserInput::BanList)
                } else if trimmed.starts_with('/') {
                    Err(UserInputError::InvalidCommand)
                } else {
                    // Ignore non-command input on server
                    Err(UserInputError::InvalidCommand)
                }
            }
        }
    }
}

impl TryFrom<String> for ServerUserInput {
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
        let input = ServerUserInput::try_from("/quit");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ServerUserInput::Quit));
    }

    #[test]
    fn test_quit_short_command() {
        let input = ServerUserInput::try_from("/q");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ServerUserInput::Quit));
    }

    #[test]
    fn test_help_command() {
        let input = ServerUserInput::try_from("/help");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ServerUserInput::Help));
    }

    #[test]
    fn test_list_command() {
        let input = ServerUserInput::try_from("/list");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ServerUserInput::ListUsers));
    }

    #[test]
    fn test_invalid_command() {
        let input = ServerUserInput::try_from("/unknown");
        assert!(input.is_err());
        assert!(matches!(input.unwrap_err(), UserInputError::InvalidCommand));
    }

    #[test]
    fn test_whitespace_trimming() {
        let input = ServerUserInput::try_from("  /help  ");
        assert!(input.is_ok());
        assert!(matches!(input.unwrap(), ServerUserInput::Help));
    }

    #[test]
    fn test_kick_command() {
        let input = ServerUserInput::try_from("/kick Alice");
        assert!(input.is_ok());
        match input.unwrap() {
            ServerUserInput::Kick(username) => assert_eq!(username, "Alice"),
            _ => panic!("Expected Kick variant"),
        }
    }

    #[test]
    fn test_kick_command_with_whitespace() {
        let input = ServerUserInput::try_from("/kick   Bob  ");
        assert!(input.is_ok());
        match input.unwrap() {
            ServerUserInput::Kick(username) => assert_eq!(username, "Bob"),
            _ => panic!("Expected Kick variant"),
        }
    }

    #[test]
    fn test_kick_command_no_username() {
        let input = ServerUserInput::try_from("/kick");
        assert!(input.is_err());
    }

    #[test]
    fn test_kick_command_empty_username() {
        let input = ServerUserInput::try_from("/kick   ");
        assert!(input.is_err());
    }
}
