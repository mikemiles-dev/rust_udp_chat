mod client;
mod completer;
mod input;
mod readline_helper;

use chat_shared::logger;
use client::ChatClient;
use std::io::{self, Write};

const DEFAULT_SERVER: &str = "127.0.0.1:8080";
const DEFAULT_NAME: &str = "Guest";

#[tokio::main]
async fn main() -> io::Result<()> {
    let (chat_server, chat_name) = prompt_server_info()?;

    let mut client = ChatClient::new(&chat_server, chat_name).await
        .map_err(|e| io::Error::other(format!("Failed to create client: {e:?}")))?;

    client.join_server().await
        .map_err(|e| io::Error::other(format!("Failed to join server: {e:?}")))?;

    client.run().await
}

fn prompt_input(prompt: &str, default: &str) -> io::Result<String> {
    logger::log_info(&format!("{} (default: {}):", prompt, default));
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    Ok(if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    })
}

fn prompt_server_info() -> io::Result<(String, String)> {
    let server = prompt_input("Enter Chat Server", DEFAULT_SERVER)?;
    let name = prompt_input("Enter Chat Name", DEFAULT_NAME)?;
    Ok((server, name))
}
