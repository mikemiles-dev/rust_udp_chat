use chrono::Local;
use colored::Colorize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn get_timestamp() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

pub fn log_info(message: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", get_timestamp()).dimmed(),
        "[INFO]".cyan().bold(),
        message
    );
}

pub fn log_success(message: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", get_timestamp()).dimmed(),
        "[OK]".green().bold(),
        message
    );
}

pub fn log_error(message: &str) {
    eprintln!(
        "{} {} {}",
        format!("[{}]", get_timestamp()).dimmed(),
        "[ERROR]".red().bold(),
        message
    );
}

pub fn log_warning(message: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", get_timestamp()).dimmed(),
        "[WARN]".yellow().bold(),
        message
    );
}

pub fn log_system(message: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", get_timestamp()).dimmed(),
        "[SYSTEM]".magenta().bold(),
        message
    );
}

pub fn log_chat(message: &str) {
    if let Some((username, msg)) = message.split_once(": ") {
        let colored_username = colorize_username(username);
        println!(
            "{} {} {}: {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[CHAT]".white().bold(),
            colored_username,
            msg
        );
    } else {
        println!(
            "{} {} {}",
            format!("[{}]", get_timestamp()).dimmed(),
            "[CHAT]".white().bold(),
            message
        );
    }
}

fn colorize_username(username: &str) -> colored::ColoredString {
    let mut hasher = DefaultHasher::new();
    username.hash(&mut hasher);
    let hash = hasher.finish();

    let colors = [
        colored::Color::Red,
        colored::Color::Green,
        colored::Color::Yellow,
        colored::Color::Blue,
        colored::Color::Magenta,
        colored::Color::Cyan,
        colored::Color::BrightRed,
        colored::Color::BrightGreen,
        colored::Color::BrightYellow,
        colored::Color::BrightBlue,
        colored::Color::BrightMagenta,
        colored::Color::BrightCyan,
    ];

    let color_index = (hash as usize) % colors.len();
    username.color(colors[color_index]).bold()
}
