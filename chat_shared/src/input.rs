#[derive(Debug)]
pub enum UserInputError {
    IoError,
    InvalidCommand,
    #[allow(dead_code)]
    InvalidUser,
}

impl From<std::io::Error> for UserInputError {
    fn from(_: std::io::Error) -> Self {
        UserInputError::IoError
    }
}

#[allow(async_fn_in_trait)]
pub trait UserInput: Sized {
    fn get_quit_command() -> Self;

    async fn get_user_input<R, U>(reader: &mut R) -> Result<U, UserInputError>
    where
        R: tokio::io::AsyncBufReadExt + Unpin,
        U: UserInput + TryFrom<String, Error = UserInputError>,
    {
        let mut input_line = String::new();

        match reader.read_line(&mut input_line).await {
            Ok(0) => Ok(U::get_quit_command()),
            Ok(_) => U::try_from(input_line.trim().to_string()),
            Err(e) => Err(e.into()),
        }
    }
}
