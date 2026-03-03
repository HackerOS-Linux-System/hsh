use std::borrow::Cow::{self, Borrowed, Owned};
use std::env;
use std::path::Path;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{Context, Helper};
use rustyline_derive::Helper;

use crate::builtins::expand_tilde;
use crate::security::highlight_dangerous;

#[derive(Helper)]
pub struct ShellHelper {
    pub colored_prompt: String,
    commands_cache: Vec<String>,
    hinter: HistoryHinter,
    completer: FilenameCompleter,
    validator: MatchingBracketValidator,
}

impl ShellHelper {
    pub fn new() -> Self {
        let mut commands_cache = Vec::new();
        // Built-ins
        for b in &["cd", "exit", "history", "which", "type", "jobs", "fg", "export", "source", "hsh-help"] {
            commands_cache.push(b.to_string());
        }
        if let Ok(path) = env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        commands_cache.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }
        ShellHelper {
            colored_prompt: String::new(),
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
    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for ShellHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        if let Some(h) = self.hinter.hint(line, pos, ctx) {
            return Some(h);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() { return None; }

        let (before_space, after_space) = match trimmed.rfind(' ') {
            Some(p) => (&trimmed[..p], &trimmed[p + 1..]),
            None => ("", trimmed),
        };
        if after_space.is_empty() { return None; }

        if before_space.is_empty() {
            if let Some(cmd) = self.commands_cache.iter().find(|c| c.starts_with(after_space)) {
                return Some(cmd[after_space.len()..].to_string());
            }
        } else {
            let path = Path::new(after_space);
            let parent = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            let prefix = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name();
                    let file_str = file_name.to_string_lossy();
                    if file_str.starts_with(prefix) && file_str != prefix {
                        return Some(file_str[prefix.len()..].to_string());
                    }
                }
            }
        }
        None
    }
}

fn is_path_like(s: &str) -> bool {
    s.starts_with('/')
    || s.starts_with("./")
    || s.starts_with("../")
    || s.starts_with('~')
    || s.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_')
}

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // Dangerous pattern override
        if let Some(highlighted) = highlight_dangerous(line) {
            return Owned(highlighted);
        }

        let mut highlighted = String::new();
        let mut i = 0;
        let mut is_command_position = true;

        while i < line.len() {
            let c = line.as_bytes()[i] as char;
            if c.is_whitespace() {
                highlighted.push(c);
                i += 1;
                continue;
            }
            if c == '"' {
                let start = i; i += 1;
                while i < line.len() && (line.as_bytes()[i] as char) != '"' { i += 1; }
                if i < line.len() { i += 1; }
                highlighted.push_str(&format!("\x1b[35m{}\x1b[0m", &line[start..i]));
                is_command_position = false;
            } else if c == '\'' {
                let start = i; i += 1;
                while i < line.len() && (line.as_bytes()[i] as char) != '\'' { i += 1; }
                if i < line.len() { i += 1; }
                highlighted.push_str(&format!("\x1b[35m{}\x1b[0m", &line[start..i]));
                is_command_position = false;
            } else if c == '$' {
                let start = i; i += 1;
                while i < line.len() {
                    let nc = line.as_bytes()[i] as char;
                    if !nc.is_alphanumeric() && nc != '_' { break; }
                    i += 1;
                }
                highlighted.push_str(&format!("\x1b[94m{}\x1b[0m", &line[start..i]));
                is_command_position = false;
            } else if "&|;><".contains(c) {
                let start = i;
                let next = if i + 1 < line.len() { Some(line.as_bytes()[i + 1] as char) } else { None };
                if (c == '&' && next == Some('&')) || (c == '|' && next == Some('|')) {
                    i += 2;
                    highlighted.push_str(&format!("\x1b[95m{}\x1b[0m", &line[start..i]));
                } else {
                    i += 1;
                    let op = &line[start..i];
                    let color = match op {
                        ";" => "\x1b[33m",
                        "|" | ">" | "<" => "\x1b[1;37m",
                        _ => "",
                    };
                    highlighted.push_str(&format!("{}{}\x1b[0m", color, op));
                }
                is_command_position = true;
            } else {
                let start = i;
                while i < line.len() {
                    let nc = line.as_bytes()[i] as char;
                    if nc.is_whitespace() || "&|;><\"'$".contains(nc) { break; }
                    i += 1;
                }
                let word = &line[start..i];
                let color = if is_command_position {
                    if self.command_exists(word) { "\x1b[32m" } else { "\x1b[31m" }
                } else if word.starts_with('-') {
                    "\x1b[33m"
                } else if is_path_like(word) && Path::new(&expand_tilde(word)).exists() {
                    "\x1b[36m"
                } else {
                    ""
                };
                highlighted.push_str(&format!("{}{}\x1b[0m", color, word));
                is_command_position = false;
            }
        }
        Owned(highlighted)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, default: bool) -> Cow<'b, str> {
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
    fn validate(&self, ctx: &mut rustyline::validate::ValidationContext) -> rustyline::Result<rustyline::validate::ValidationResult> {
        self.validator.validate(ctx)
    }
    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}
