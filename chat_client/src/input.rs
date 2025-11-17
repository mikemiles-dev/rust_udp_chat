use std::io;

#[derive(Debug)]
pub enum UserInput {
    Help,
    ListUsers,
    Message(String),
    DirectMessage { recipient: String, message: String },
    Reply(String),
    Quit,
}

#[derive(Debug)]
pub enum UserInputError {
    IoError,
    InvalidCommand,
    InvalidUser,
}

impl From<io::Error> for UserInputError {
    fn from(_: io::Error) -> Self {
        UserInputError::IoError
    }
}

impl TryFrom<&str> for UserInput {
    type Error = UserInputError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        match parts.first().copied().unwrap_or("") {
            "/quit" => Ok(UserInput::Quit),
            "/list" => Ok(UserInput::ListUsers),
            "/help" => Ok(UserInput::Help),
            "/dm" => {
                if parts.len() < 3 {
                    Err(UserInputError::InvalidCommand)
                } else {
                    let recipient = parts[1].to_string();
                    let message = parts[2..].join(" ");
                    Ok(UserInput::DirectMessage { recipient, message })
                }
            }
            "/r" => {
                if parts.len() < 2 {
                    Err(UserInputError::InvalidCommand)
                } else {
                    let message = parts[1..].join(" ");
                    Ok(UserInput::Reply(message))
                }
            }
            _ => {
                if trimmed.starts_with('/') {
                    Err(UserInputError::InvalidCommand)
                } else {
                    Ok(UserInput::Message(trimmed.to_string()))
                }
            }
        }
    }
}

pub async fn get_user_input<R>(reader: &mut R) -> Result<UserInput, UserInputError>
where
    R: tokio::io::AsyncBufReadExt + Unpin,
{
    let mut input_line = String::new();

    match reader.read_line(&mut input_line).await {
        Ok(0) => Ok(UserInput::Quit),
        Ok(_) => UserInput::try_from(input_line.as_str()),
        Err(e) => Err(e.into()),
    }
}
