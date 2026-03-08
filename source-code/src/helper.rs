use std::borrow::Cow::{self, Borrowed, Owned};
use std::collections::HashMap;
use std::env;
use std::fs::read_dir;
use std::path::Path;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::Context;
use rustyline_derive::Helper;

use crate::security::highlight_dangerous;
use crate::smarthints::SmartHints;
use crate::theme::Theme;

fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        if let Ok(home) = env::var("HOME") {
            return format!("{}{}", home, &s[1..]);
        }
    }
    s.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────

#[derive(Helper)]
pub struct ShellHelper {
    pub colored_prompt: String,
    pub next_hint:      Option<String>,
    pub theme:          Theme,
    pub commands_cache: Vec<String>,
    /// prefixes snapshot: first_word → [(full_cmd, count)] sorted desc
    pub hints_snapshot: HashMap<String, Vec<(String, u64)>>,
    /// sequences snapshot: prev_cmd → best_next_cmd
    pub seq_snapshot:   HashMap<String, String>,
    hinter:             HistoryHinter,
    completer:          FilenameCompleter,
}

impl ShellHelper {
    pub fn new(theme: Theme) -> Self {
        let mut commands_cache = vec![
            "cd", "exit", "history", "which", "type", "jobs",
            "fg", "export", "source", "hsh-help", "test",
            "hsh-settings", "hsh-docs",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

        if let Ok(path) = env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = read_dir(dir) {
                    for entry in entries.flatten() {
                        commands_cache.push(
                            entry.file_name().to_string_lossy().to_string(),
                        );
                    }
                }
            }
        }
        commands_cache.sort();
        commands_cache.dedup();

        ShellHelper {
            colored_prompt: String::new(),
            next_hint:      None,
            theme,
            commands_cache,
            hints_snapshot: HashMap::new(),
            seq_snapshot:   HashMap::new(),
            hinter:         HistoryHinter {},
            completer:      FilenameCompleter::new(),
        }
    }

    /// Synchronizuj snapshot z SmartHints — wywołuj po każdej komendzie
    pub fn sync_hints(&mut self, hints: &SmartHints) {
        self.hints_snapshot.clear();
        for (word, map) in hints.prefixes_ref() {
            let mut v: Vec<(String, u64)> = map
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            self.hints_snapshot.insert(word.clone(), v);
        }

        self.seq_snapshot.clear();
        for (prev, map) in hints.sequences_ref() {
            if let Some((best, _)) = map.iter().max_by_key(|(_, v)| *v) {
                self.seq_snapshot.insert(prev.clone(), best.clone());
            }
        }
    }

    /// Inline completion hint — wywoływane z hint() przy każdym znaku
    fn inline_hint(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() { return None; }
        let first_word = trimmed.split_whitespace().next()?;
        let entries = self.hints_snapshot.get(first_word)?;
        for (cmd, _) in entries {
            if cmd.starts_with(trimmed) && cmd.len() > trimmed.len() {
                return Some(cmd[trimmed.len()..].to_string());
            }
        }
        None
    }

    fn command_exists(&self, cmd: &str) -> bool {
        self.commands_cache.iter().any(|c| c == cmd) || Path::new(cmd).exists()
    }
}

// ─── Completer ────────────────────────────────────────────────────────────────

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before_cursor = &line[..pos];
        let trimmed       = before_cursor.trim_start();

        // Pierwsze słowo → complete commands
        if !trimmed.contains(' ') {
            let prefix = trimmed;
            let mut matches: Vec<Pair> = self
            .commands_cache
            .iter()
            .filter(|c| c.starts_with(prefix) && c.as_str() != prefix)
            .map(|c| Pair { display: c.clone(), replacement: c.clone() })
            .collect();
            matches.sort_by(|a, b| {
                a.display.len().cmp(&b.display.len())
                .then(a.display.cmp(&b.display))
            });
            if !matches.is_empty() {
                let start = line.len() - prefix.len();
                return Ok((start, matches));
            }
        }

        // Subkomendy dla znanych narzędzi
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if let Some(&cmd) = parts.first() {
            let partial = parts.get(1).copied().unwrap_or("");
            if let Some(subs) = subcommand_completions(cmd, partial) {
                let start   = line.rfind(' ').map(|p| p + 1).unwrap_or(pos);
                let part    = &line[start..pos];
                let matches: Vec<Pair> = subs
                .into_iter()
                .filter(|s| s.starts_with(part))
                .map(|s| Pair { display: s.clone(), replacement: s })
                .collect();
                if !matches.is_empty() {
                    return Ok((start, matches));
                }
            }
        }

        // Historia jako Tab-completion
        if let Some(hint) = self.inline_hint(line) {
            let full = format!("{}{}", line, hint);
            return Ok((0, vec![Pair { display: full.clone(), replacement: full }]));
        }

        // Fallback: pliki
        self.completer.complete(line, pos, ctx)
    }
}

fn subcommand_completions(cmd: &str, partial: &str) -> Option<Vec<String>> {
    let subs: &[&str] = match cmd {
        "git"       => &["add", "commit", "push", "pull", "status", "log", "diff",
        "branch", "checkout", "merge", "rebase", "stash", "clone",
        "fetch", "tag", "show", "reset", "restore", "switch", "init"],
        "cargo"     => &["build", "run", "test", "check", "clippy", "fmt", "doc",
        "publish", "update", "add", "remove", "clean", "new", "init"],
        "systemctl" => &["start", "stop", "restart", "enable", "disable", "status",
        "reload", "daemon-reload", "list-units", "is-active", "mask"],
        "apt"       => &["install", "remove", "update", "upgrade", "search",
        "show", "list", "purge", "autoremove", "dist-upgrade"],
        "docker"    => &["run", "build", "pull", "push", "ps", "images", "stop",
        "start", "exec", "logs", "rm", "rmi", "compose", "inspect"],
        "npm"       => &["install", "run", "start", "test", "build", "publish",
        "update", "uninstall", "init", "ci", "audit"],
        "pip"       => &["install", "uninstall", "list", "show", "freeze",
        "search", "download", "wheel"],
        "kubectl"   => &["get", "apply", "delete", "describe", "logs", "exec",
        "port-forward", "scale", "rollout", "config"],
        "make"      => &["all", "clean", "install", "test", "build", "run"],
        _           => return None,
    };
    Some(
        subs.iter()
        .filter(|s| s.starts_with(partial))
        .map(|s| s.to_string())
        .collect(),
    )
}

// ─── Hinter ───────────────────────────────────────────────────────────────────

impl Hinter for ShellHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        // Hint tylko na końcu linii (jak fish)
        if pos < line.len() { return None; }

        // 1. Historia rustyline — najwyższy priorytet
        if let Some(h) = self.hinter.hint(line, pos, ctx) {
            return Some(h);
        }

        // 2. Smart inline hint z historii komend (jak fish)
        if !line.trim().is_empty() {
            if let Some(suffix) = self.inline_hint(line) {
                return Some(suffix);
            }
        }

        // 3. Pusta linia → następna komenda
        if line.trim().is_empty() {
            if let Some(ref nh) = self.next_hint {
                return Some(nh.clone());
            }
        }

        None
    }
}

// ─── Highlighter ──────────────────────────────────────────────────────────────

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if let Some(highlighted) = highlight_dangerous(line) {
            return Owned(highlighted);
        }

        let t     = &self.theme;
        let reset = "\x1b[0m";

        let mut out    = String::with_capacity(line.len() + 128);
        let mut is_cmd = true;
        let mut in_s   = false;
        let mut in_d   = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];

            if c.is_whitespace() && !in_s && !in_d {
                out.push(c);
                i += 1;
                continue;
            }

            match c {
                // ── Double quote ──────────────────────────────────────────────
                '"' if !in_s => {
                    in_d = !in_d;
                    if in_d {
                        let s = format!("{}\"", t.string_color);
                        out.push_str(&s);
                    } else {
                        let s = format!("\"{}", reset);
                        out.push_str(&s);
                    }
                    i += 1;
                }

                // ── Single quote ──────────────────────────────────────────────
                '\'' if !in_d => {
                    in_s = !in_s;
                    if in_s {
                        let s = format!("{}'", t.string_color);
                        out.push_str(&s);
                    } else {
                        let s = format!("'{}", reset);
                        out.push_str(&s);
                    }
                    i += 1;
                }

                // ── Inside single quotes ──────────────────────────────────────
                _ if in_s => { out.push(c); i += 1; }

                // ── Inside double quotes ──────────────────────────────────────
                _ if in_d => {
                    if c == '$' {
                        i += 1;
                        let var_open = format!("{}$", t.var_color);
                        out.push_str(&var_open);
                        let start = i;
                        while i < chars.len()
                            && (chars[i].is_alphanumeric() || chars[i] == '_')
                            {
                                out.push(chars[i]);
                                i += 1;
                            }
                            if i == start {
                                if let Some(&sc) = chars.get(i) {
                                    if "?#@$*!0".contains(sc) { out.push(sc); i += 1; }
                                }
                            }
                            out.push_str(&t.string_color);
                    } else {
                        out.push(c); i += 1;
                    }
                }

                // ── $VAR / $() / $(()) ────────────────────────────────────────
                '$' => {
                    i += 1;
                    if chars.get(i) == Some(&'(') {
                        let open = format!("{}$(", t.var_color);
                        out.push_str(&open);
                        i += 1;
                        let mut depth = 1i32;
                        while i < chars.len() {
                            out.push(chars[i]);
                            if chars[i] == '(' { depth += 1; }
                            if chars[i] == ')' {
                                depth -= 1;
                                if depth == 0 { i += 1; break; }
                            }
                            i += 1;
                        }
                        out.push_str(reset);
                    } else if chars.get(i) == Some(&'{') {
                        let open = format!("{}${{", t.var_color);
                        out.push_str(&open);
                        i += 1;
                        while i < chars.len() && chars[i] != '}' {
                            out.push(chars[i]); i += 1;
                        }
                        if i < chars.len() { out.push('}'); i += 1; }
                        out.push_str(reset);
                    } else {
                        let var_start = i;
                        if i < chars.len() && "?#@$*!0123456789".contains(chars[i]) {
                            let s = format!("{}${}{}", t.var_color, chars[i], reset);
                            out.push_str(&s);
                            i += 1;
                        } else {
                            while i < chars.len()
                                && (chars[i].is_alphanumeric() || chars[i] == '_')
                                {
                                    i += 1;
                                }
                                if i == var_start {
                                    out.push('$');
                                } else {
                                    let name: String = chars[var_start..i].iter().collect();
                                    let s = format!("{}${}{}", t.var_color, name, reset);
                                    out.push_str(&s);
                                }
                        }
                    }
                    is_cmd = false;
                }

                // ── Operatory ─────────────────────────────────────────────────
                ';' => {
                    let s = format!("{};{}", t.op_color, reset);
                    out.push_str(&s);
                    is_cmd = true; i += 1;
                }
                '|' if chars.get(i + 1) == Some(&'|') => {
                    let s = format!("{}||{}", t.op_color, reset);
                    out.push_str(&s);
                    is_cmd = true; i += 2;
                }
                '&' if chars.get(i + 1) == Some(&'&') => {
                    let s = format!("{}&&{}", t.op_color, reset);
                    out.push_str(&s);
                    is_cmd = true; i += 2;
                }
                '|' => {
                    let s = format!("{}|{}", t.op_color, reset);
                    out.push_str(&s);
                    is_cmd = true; i += 1;
                }
                '>' | '<' => {
                    let s = format!("{}{}{}", t.op_color, c, reset);
                    out.push_str(&s);
                    i += 1;
                }
                '&' => {
                    let s = format!("{}&{}", t.op_color, reset);
                    out.push_str(&s);
                    i += 1;
                }

                // ── Słowa: komendy, flagi, ścieżki ────────────────────────────
                _ => {
                    let word_start = i;
                    while i < chars.len() {
                        let nc = chars[i];
                        if nc.is_whitespace()
                            || (!in_s && !in_d && "&|;><\"'$".contains(nc))
                            {
                                break;
                            }
                            i += 1;
                    }
                    let word: String = chars[word_start..i].iter().collect();

                    let color: &str = if is_cmd {
                        if self.command_exists(&word) { &t.cmd_ok_color }
                        else                          { &t.cmd_err_color }
                    } else if word.starts_with('-') {
                        &t.flag_color
                    } else if word.starts_with('/')
                        || word.starts_with("~/")
                        || word.starts_with("./")
                        {
                            if Path::new(&expand_tilde(&word)).exists() { &t.path_color }
                            else { reset }
                        } else {
                            reset
                        };

                    out.push_str(color);
                    out.push_str(&word);
                    out.push_str(reset);
                    is_cmd = false;
                }
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
        Owned(format!("{}{}\x1b[0m", self.theme.hint_color, hint))
    }

    fn highlight_char(&self, line: &str, _pos: usize, _forced: bool) -> bool {
        !line.is_empty()
    }
}

// ─── Validator ────────────────────────────────────────────────────────────────

impl Validator for ShellHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        match check_completeness(ctx.input()) {
            InputState::Complete      => Ok(ValidationResult::Valid(None)),
            InputState::Incomplete(_) => Ok(ValidationResult::Incomplete),
            InputState::Invalid(msg)  => Ok(ValidationResult::Invalid(Some(
                format!("\x1b[31m  ← {}\x1b[0m", msg),
            ))),
        }
    }
    fn validate_while_typing(&self) -> bool { false }
}

#[derive(Debug)]
enum InputState { Complete, Incomplete(String), Invalid(String) }

fn check_completeness(input: &str) -> InputState {
    let mut if_d   = 0i32; let mut for_d  = 0i32;
    let mut wh_d   = 0i32; let mut case_d = 0i32;
    let mut br_d   = 0i32;
    let mut in_s   = false; let mut in_d  = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_d => in_s = !in_s,
            '"'  if !in_s => in_d = !in_d,
            _ => {}
        }
    }
    if in_s { return InputState::Incomplete("unclosed '".into()); }
    if in_d { return InputState::Incomplete("unclosed \"".into()); }

    for tok in input.split_whitespace() {
        match tok {
            "if"    => if_d   += 1,
            "fi"    => if_d   -= 1,
            "for"   => for_d  += 1,
            "while" => wh_d   += 1,
            "done"  => {
                if for_d > 0      { for_d -= 1; }
                else if wh_d > 0  { wh_d  -= 1; }
            }
            "case"  => case_d += 1,
            "esac"  => case_d -= 1,
            "{"     => br_d   += 1,
            "}"     => br_d   -= 1,
            _ => {}
        }
    }

    if if_d   > 0 { return InputState::Incomplete(format!("brakuje 'fi' ({})",   if_d));   }
    if for_d  > 0 { return InputState::Incomplete(format!("brakuje 'done' ({})", for_d));  }
    if wh_d   > 0 { return InputState::Incomplete(format!("brakuje 'done' ({})", wh_d));   }
    if case_d > 0 { return InputState::Incomplete(format!("brakuje 'esac' ({})", case_d)); }
    if br_d   > 0 { return InputState::Incomplete(format!("brakuje '}}' ({})",   br_d));   }
    if input.trim_end().ends_with('\\') {
        return InputState::Incomplete("kontynuacja linii".into());
    }
    InputState::Complete
}
