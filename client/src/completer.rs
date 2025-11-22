use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};
use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Client command and username completer
pub struct ClientCompleter {
    commands: Vec<String>,
    users: Arc<RwLock<HashSet<String>>>,
}

impl ClientCompleter {
    pub fn new(users: Arc<RwLock<HashSet<String>>>) -> Self {
        Self {
            commands: vec![
                "/help".to_string(),
                "/quit".to_string(),
                "/list".to_string(),
                "/dm".to_string(),
                "/r".to_string(),
                "/send".to_string(),
                "/rename".to_string(),
            ],
            users,
        }
    }

    fn get_candidates(&self, line: &str) -> Vec<String> {
        let trimmed = line.trim_start();

        // If line starts with /dm or /send and has a space, complete usernames
        if trimmed.starts_with("/dm ") || trimmed.starts_with("/send ") {
            let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
            if parts.len() == 2 {
                // Complete username after /dm or /send
                let cmd = parts[0];
                let prefix = parts[1];
                let users = self.users.read().unwrap();
                return users
                    .iter()
                    .filter(|u| u.starts_with(prefix))
                    .map(|u| format!("{} {}", cmd, u))
                    .collect();
            }
        }

        // Complete commands
        if trimmed.starts_with('/') {
            self.commands
                .iter()
                .filter(|cmd| cmd.starts_with(trimmed))
                .cloned()
                .collect()
        } else {
            vec![]
        }
    }
}

impl Completer for ClientCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let candidates = self.get_candidates(&line[..pos]);

        let pairs: Vec<Pair> = candidates
            .into_iter()
            .map(|c| Pair {
                display: c.clone(),
                replacement: c,
            })
            .collect();

        Ok((0, pairs))
    }
}

impl Hinter for ClientCompleter {
    type Hint = String;

    fn hint(&self, line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        let candidates = self.get_candidates(line);
        if candidates.len() == 1 {
            let candidate = &candidates[0];
            if candidate.starts_with(line) && candidate.len() > line.len() {
                return Some(candidate[line.len()..].to_string());
            }
        }
        None
    }
}

impl Highlighter for ClientCompleter {}

impl Validator for ClientCompleter {}

impl Helper for ClientCompleter {}
