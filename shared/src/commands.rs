//! Centralized command definitions for client and server
//! This module provides a single source of truth for command metadata

/// Represents a command with its metadata
#[derive(Debug, Clone)]
pub struct Command {
    /// Primary command name (e.g., "/help")
    pub name: &'static str,
    /// Optional alias (e.g., "/h" for "/help")
    pub alias: Option<&'static str>,
    /// Short description for help text
    pub description: &'static str,
    /// Usage hint showing arguments (e.g., "<username> <message>")
    pub usage: Option<&'static str>,
}

impl Command {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            alias: None,
            description: "",
            usage: None,
        }
    }

    pub const fn with_alias(mut self, alias: &'static str) -> Self {
        self.alias = Some(alias);
        self
    }

    pub const fn with_description(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }

    pub const fn with_usage(mut self, usage: &'static str) -> Self {
        self.usage = Some(usage);
        self
    }

    /// Returns all names for this command (primary + alias)
    pub fn all_names(&self) -> Vec<&'static str> {
        let mut names = vec![self.name];
        if let Some(alias) = self.alias {
            names.push(alias);
        }
        names
    }

    /// Check if the given string matches this command's name or alias
    pub fn matches(&self, cmd: &str) -> bool {
        cmd == self.name || self.alias == Some(cmd)
    }

    /// Format command for help display
    pub fn help_line(&self) -> String {
        let mut line = self.name.to_string();
        if let Some(alias) = self.alias {
            line.push_str(&format!(" ({})", alias));
        }
        if let Some(usage) = self.usage {
            line.push_str(&format!(" {}", usage));
        }
        line.push_str(&format!(" - {}", self.description));
        line
    }
}

/// Client commands
pub mod client {
    use super::Command;

    pub const HELP: Command = Command::new("/help").with_description("Show this help message");

    pub const QUIT: Command = Command::new("/quit").with_description("Exit the chat");

    pub const LIST: Command =
        Command::new("/list").with_description("List all users (with statuses)");

    pub const DM: Command = Command::new("/dm")
        .with_usage("<username> <message>")
        .with_description("Send direct message");

    pub const REPLY: Command = Command::new("/r")
        .with_usage("<message>")
        .with_description("Reply to last direct message");

    pub const SEND: Command = Command::new("/send")
        .with_usage("<username> <filepath>")
        .with_description("Send a file (max 10MB)");

    pub const RENAME: Command = Command::new("/rename")
        .with_usage("<new_name>")
        .with_description("Change your username");

    pub const STATUS: Command = Command::new("/status")
        .with_usage("<message>")
        .with_description("Set your status (visible in /list)");

    pub const STATUS_CLEAR: Command = Command::new("/status").with_description("Clear your status");

    /// All client commands (for completion - excludes STATUS_CLEAR as it's same command)
    pub const ALL: &[Command] = &[HELP, LIST, DM, REPLY, SEND, RENAME, STATUS, QUIT];

    /// All help entries (includes STATUS_CLEAR for documentation)
    pub const HELP_ENTRIES: &[Command] = &[
        HELP,
        LIST,
        DM,
        REPLY,
        SEND,
        RENAME,
        STATUS,
        STATUS_CLEAR,
        QUIT,
    ];

    /// Get all command names for completion (includes aliases)
    pub fn completion_names() -> Vec<&'static str> {
        ALL.iter().flat_map(|cmd| cmd.all_names()).collect()
    }

    /// Generate help text for all commands
    pub fn help_text() -> Vec<String> {
        let mut lines = vec!["Available commands:".to_string()];
        for cmd in HELP_ENTRIES {
            lines.push(format!("  {}", cmd.help_line()));
        }
        lines
    }
}

/// Server commands
pub mod server {
    use super::Command;

    pub const HELP: Command = Command::new("/help")
        .with_alias("/h")
        .with_description("Show this help message");

    pub const QUIT: Command = Command::new("/quit")
        .with_alias("/q")
        .with_description("Shutdown the server");

    pub const LIST: Command = Command::new("/list").with_description("List all connected users");

    pub const KICK: Command = Command::new("/kick")
        .with_usage("<user>")
        .with_description("Kick a user from the server");

    pub const RENAME: Command = Command::new("/rename")
        .with_usage("<user> <newname>")
        .with_description("Rename a user");

    pub const BAN: Command = Command::new("/ban")
        .with_usage("<user|ip>")
        .with_description("Ban a user by name or IP address");

    pub const UNBAN: Command = Command::new("/unban")
        .with_usage("<ip>")
        .with_description("Unban an IP address");

    pub const BANLIST: Command = Command::new("/banlist").with_description("List all banned IPs");

    /// All server commands
    pub const ALL: &[Command] = &[LIST, KICK, RENAME, BAN, UNBAN, BANLIST, HELP, QUIT];

    /// Get all command names for completion (includes aliases)
    pub fn completion_names() -> Vec<&'static str> {
        ALL.iter().flat_map(|cmd| cmd.all_names()).collect()
    }

    /// Generate help text for all commands
    pub fn help_text() -> Vec<String> {
        let mut lines = vec!["Available server commands:".to_string()];
        for cmd in ALL {
            lines.push(format!("  {}", cmd.help_line()));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_completion_names() {
        let names = client::completion_names();
        assert!(names.contains(&"/help"));
        assert!(names.contains(&"/dm"));
        assert!(names.contains(&"/status"));
        assert_eq!(names.len(), 8); // 8 commands, no aliases
    }

    #[test]
    fn test_server_completion_names() {
        let names = server::completion_names();
        assert!(names.contains(&"/help"));
        assert!(names.contains(&"/h"));
        assert!(names.contains(&"/quit"));
        assert!(names.contains(&"/q"));
        assert!(names.contains(&"/ban"));
        assert_eq!(names.len(), 10); // 8 commands + 2 aliases
    }

    #[test]
    fn test_help_line_format() {
        let line = client::DM.help_line();
        assert!(line.contains("/dm"));
        assert!(line.contains("<username> <message>"));
        assert!(line.contains("Send direct message"));
    }

    #[test]
    fn test_client_help_text() {
        let help = client::help_text();
        assert!(help[0].contains("Available commands"));
        assert!(help.len() > 1);
    }

    #[test]
    fn test_server_help_text() {
        let help = server::help_text();
        assert!(help[0].contains("Available server commands"));
        assert!(help.len() > 1);
    }
}
