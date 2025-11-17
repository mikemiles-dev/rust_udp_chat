use std::io;

#[derive(Debug)]
pub enum UserInput {
    Help,
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
        match trimmed.split_whitespace().next().unwrap_or("") {
            "/quit" => Ok(UserInput::Quit),
            "/help" => Ok(UserInput::Help),
            _ => Ok(UserInput::Message(trimmed.to_string())),
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
        Err(e) => Err(UserInputError::IoError(e)),
    }
}
