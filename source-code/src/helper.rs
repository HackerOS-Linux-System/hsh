use std::borrow::Cow::{self, Borrowed, Owned};
use std::env;
use std::fs::read_dir;
use std::path::Path;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::Context;
use rustyline_derive::Helper;

use crate::security::highlight_dangerous;

fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        if let Ok(home) = env::var("HOME") {
            return format!("{}{}", home, &s[1..]);
        }
    }
    s.to_string()
}

fn is_path_like(s: &str) -> bool {
    s.starts_with('/')
    || s.starts_with("./")
    || s.starts_with("../")
    || s.starts_with('~')
    || s.chars()
    .all(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_')
}

#[derive(Helper)]
pub struct ShellHelper {
    pub colored_prompt: String,
    pub next_hint: Option<String>,
    commands_cache: Vec<String>,
    hinter: HistoryHinter,
    completer: FilenameCompleter,
    validator: MatchingBracketValidator,
}

impl ShellHelper {
    pub fn new() -> Self {
        let mut commands_cache = Vec::new();

        // Built-ins always available
        for b in &[
            "cd", "exit", "history", "which", "type", "jobs",
            "fg", "export", "source", "hsh-help",
        ] {
            commands_cache.push(b.to_string());
        }

        // Scan $PATH
        if let Ok(path) = env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = read_dir(dir) {
                    for entry in entries.flatten() {
                        commands_cache.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }

        ShellHelper {
            colored_prompt: String::new(),
            next_hint: None,
            commands_cache,
            hinter: HistoryHinter {},
            completer: FilenameCompleter::new(),
            validator: MatchingBracketValidator::new(),
        }
    }

    fn command_exists(&self, cmd: &str) -> bool {
        self.commands_cache.contains(&cmd.to_string()) || Path::new(cmd).exists()
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for ShellHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        // History hint takes priority
        if let Some(h) = self.hinter.hint(line, pos, ctx) {
            return Some(h);
        }

        // Next-command smart hint
        if let Some(ref nh) = self.next_hint {
            if line.trim().is_empty() {
                return Some(nh.clone());
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let (before_space, after_space) = match trimmed.rfind(' ') {
            Some(p) => (&trimmed[..p], &trimmed[p + 1..]),
            None    => ("", trimmed),
        };

        if after_space.is_empty() {
            return None;
        }

        if before_space.is_empty() {
            // Suggest command completion
            if let Some(cmd) = self
                .commands_cache
                .iter()
                .find(|c| c.starts_with(after_space))
                {
                    return Some(cmd[after_space.len()..].to_string());
                }
        } else {
            // Suggest file completion
            let path   = Path::new(after_space);
            let parent = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            let prefix = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

            if let Ok(entries) = read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let s    = name.to_string_lossy();
                    if s.starts_with(prefix) && s != prefix {
                        return Some(s[prefix.len()..].to_string());
                    }
                }
            }
        }

        None
    }
}

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if let Some(highlighted) = highlight_dangerous(line) {
            return Owned(highlighted);
        }

        let mut out = String::new();
        let mut i   = 0;
        let mut is_cmd = true;

        while i < line.len() {
            let c = line.as_bytes()[i] as char;

            if c.is_whitespace() {
                out.push(c);
                i += 1;
                continue;
            }

            if c == '"' {
                let start = i; i += 1;
                while i < line.len() && (line.as_bytes()[i] as char) != '"' { i += 1; }
                if i < line.len() { i += 1; }
                out.push_str(&format!("\x1b[35m{}\x1b[0m", &line[start..i]));
                is_cmd = false;
            } else if c == '\'' {
                let start = i; i += 1;
                while i < line.len() && (line.as_bytes()[i] as char) != '\'' { i += 1; }
                if i < line.len() { i += 1; }
                out.push_str(&format!("\x1b[35m{}\x1b[0m", &line[start..i]));
                is_cmd = false;
            } else if c == '$' {
                let start = i; i += 1;
                while i < line.len() {
                    let nc = line.as_bytes()[i] as char;
                    if !nc.is_alphanumeric() && nc != '_' { break; }
                    i += 1;
                }
                out.push_str(&format!("\x1b[94m{}\x1b[0m", &line[start..i]));
                is_cmd = false;
            } else if "&|;><".contains(c) {
                let start = i;
                let next  = line.as_bytes().get(i + 1).map(|&b| b as char);
                if (c == '&' && next == Some('&')) || (c == '|' && next == Some('|')) {
                    i += 2;
                    out.push_str(&format!("\x1b[95m{}\x1b[0m", &line[start..i]));
                } else {
                    i += 1;
                    let color = match c {
                        ';'        => "\x1b[33m",
                        '|'|'>'|'<'=> "\x1b[1;37m",
                        _          => "",
                    };
                    out.push_str(&format!("{}{}\x1b[0m", color, &line[start..i]));
                }
                is_cmd = true;
            } else {
                let start = i;
                while i < line.len() {
                    let nc = line.as_bytes()[i] as char;
                    if nc.is_whitespace() || "&|;><\"'$".contains(nc) { break; }
                    i += 1;
                }
                let word  = &line[start..i];
                let color = if is_cmd {
                    if self.command_exists(word) { "\x1b[32m" } else { "\x1b[31m" }
                } else if word.starts_with('-') {
                    "\x1b[33m"
                } else if is_path_like(word) && Path::new(&expand_tilde(word)).exists() {
                    "\x1b[36m"
                } else {
                    ""
                };
                out.push_str(&format!("{}{}\x1b[0m", color, word));
                is_cmd = false;
            }
        }

        Owned(out)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        if default { Borrowed(&self.colored_prompt) } else { Borrowed(prompt) }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned("\x1b[90m".to_owned() + hint + "\x1b[0m")
    }

    fn highlight_char(&self, line: &str, _pos: usize, _forced: bool) -> bool {
        !line.is_empty()
    }
}

impl Validator for ShellHelper {
    fn validate(
        &self,
        ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<rustyline::validate::ValidationResult> {
        self.validator.validate(ctx)
    }

    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}
