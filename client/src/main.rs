mod client;
mod completer;
mod input;
mod readline_helper;

use client::ChatClient;
use shared::logger;
use std::env;
use std::io::{self, Write};

const DEFAULT_SERVER: &str = "tls://milesrust.chat:8443";
const DEFAULT_NAME: &str = "Guest";

/// Restore terminal to a sane state (cursor visible, line buffered, echo on)
fn restore_terminal() {
    // Show cursor (ANSI escape sequence)
    print!("\x1B[?25h");
    // Reset all attributes
    print!("\x1B[0m");
    let _ = io::stdout().flush();

    // Also restore terminal from raw mode using stty
    // This ensures the terminal is fully restored even if rustyline
    // left it in an inconsistent state
    #[cfg(unix)]
    {
        use std::process::Command;
        let _ = Command::new("stty").arg("sane").status();
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (chat_server, chat_name) = get_server_info()?;

    let mut client = ChatClient::new(&chat_server, chat_name)
        .await
        .map_err(|e| {
            logger::log_error(&format!("Failed to create client: {:?}", e));
            io::Error::other(format!("Failed to create client: {e:?}"))
        })?;

    client
        .join_server()
        .await
        .map_err(|e| io::Error::other(format!("Failed to join server: {e:?}")))?;

    // Run client with Ctrl+C handling
    tokio::select! {
        result = client.run() => {
            restore_terminal();
            result
        }
        _ = tokio::signal::ctrl_c() => {
            restore_terminal();
            println!(); // New line after ^C
            logger::log_info("Interrupted, exiting...");
            Ok(())
        }
    }
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

fn get_server_info() -> io::Result<(String, String)> {
    // Check for environment variables first
    let server = match env::var("CHAT_SERVER") {
        Ok(val) if !val.is_empty() => {
            logger::log_info(&format!("Using server from CHAT_SERVER: {}", val));
            val
        }
        _ => prompt_input("Enter Chat Server", DEFAULT_SERVER)?,
    };

    let name = match env::var("CHAT_USERNAME") {
        Ok(val) if !val.is_empty() => {
            logger::log_info(&format!("Using username from CHAT_USERNAME: {}", val));
            val
        }
        _ => prompt_input("Enter Chat Name", DEFAULT_NAME)?,
    };

    Ok((server, name))
}
