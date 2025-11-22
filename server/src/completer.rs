use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};
use shared::commands::server as commands;

/// Server command completer
pub struct ServerCompleter {
    commands: Vec<&'static str>,
}

impl ServerCompleter {
    pub fn new() -> Self {
        Self {
            commands: commands::completion_names(),
        }
    }

    fn get_candidates(&self, line: &str) -> Vec<String> {
        let trimmed = line.trim_start();

        if trimmed.starts_with('/') {
            self.commands
                .iter()
                .filter(|cmd| cmd.starts_with(trimmed))
                .map(|s| s.to_string())
                .collect()
        } else {
            vec![]
        }
    }
}

impl Completer for ServerCompleter {
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

impl Hinter for ServerCompleter {
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

impl Highlighter for ServerCompleter {}

impl Validator for ServerCompleter {}

impl Helper for ServerCompleter {}
