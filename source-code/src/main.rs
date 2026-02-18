use chrono::Local;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hint, Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{Cmd, CompletionType, Config, Context, EditMode, Editor, KeyEvent};
use rustyline_derive::{Completer, Helper, Highlighter, Hinter, Validator};
use std::borrow::Cow::{self, Borrowed, Owned};
use std::collections::HashMap;
use std::env;
use std::fs::{self, metadata, read_dir, read_to_string};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use brush_core::Shell as BrushShell;
use hk_parser::{load_hk_file, resolve_interpolations, HkConfig, HkError, HkValue};
use libc::getuid;
use shlex;
use std::process::ExitStatus;

#[derive(Helper, Completer, Highlighter, Hinter, Validator)]
struct ShellHelper {
    highlighter: MatchingBracketHighlighter,
    validator: MatchingBracketValidator,
    hinter: HistoryHinter,
    completer: FilenameCompleter,
    colored_prompt: String,
    commands_cache: Vec<String>,
}

impl ShellHelper {
    fn new() -> Self {
        let mut commands_cache = Vec::new();
        if let Ok(path) = env::var("PATH") {
            for dir in path.split(':') {
                if let Ok(entries) = read_dir(dir) {
                    for entry in entries.flatten() {
                        let file_name = entry.file_name().to_string_lossy().to_string();
                        commands_cache.push(file_name);
                    }
                }
            }
        }

        ShellHelper {
            highlighter: MatchingBracketHighlighter::new(),
            validator: MatchingBracketValidator::new(),
            hinter: HistoryHinter {},
            completer: FilenameCompleter::new(),
            colored_prompt: "".to_owned(),
            commands_cache,
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
        if let Some(history_hint) = self.hinter.hint(line, pos, ctx) {
            return Some(history_hint);
        }

        let trimmed = line.trim();

        if trimmed.is_empty() {
            return None;
        }

        let (before_space, after_space) = match trimmed.rfind(' ') {
            Some(p) => (&trimmed[..p], &trimmed[p + 1..]),
            None => ("", trimmed),
        };

        if after_space.is_empty() {
            return None;
        }

        if before_space.is_empty() {
            // Suggest commands
            let prefix = after_space;
            if let Some(cmd) = self.commands_cache.iter().find(|c| c.starts_with(prefix)) {
                return Some(cmd[prefix.len()..].to_string());
            }
        } else {
            // Suggest files
            let path = Path::new(after_space);
            let parent = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            };

            let prefix = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

            if let Ok(entries) = read_dir(parent) {
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

impl Highlighter for ShellHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let dangerous_patterns = vec!["rm -rf /", "rm -rf /*", "dd if=/dev/zero of=/dev/sda", "mkfs /dev/sda"];

        if dangerous_patterns.iter().any(|p| line.contains(p)) {
            return Owned(format!("\x1b[5;41m{}\x1b[0m", line));
        }

        if let Some(parts) = shlex::split(line) {
            let mut highlighted = String::new();
            for (i, token) in parts.iter().enumerate() {
                let color = if i == 0 {
                    if self.command_exists(token) {
                        "\x1b[32m" // green
                    } else {
                        "\x1b[31m" // red
                    }
                } else if token.starts_with('-') || token.starts_with("--") {
                    "\x1b[33m" // yellow
                } else if is_path_like(token) && Path::new(&expand_tilde(token)).exists() {
                    "\x1b[36m" // cyan
                } else {
                    "\x1b[0m" // default
                };
                highlighted.push_str(&format!("{}{}\x1b[0m ", color, token));
            }
            Owned(highlighted.trim_end().to_string())
        } else {
            Borrowed(line)
        }
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        if default {
            Borrowed(&self.colored_prompt)
        } else {
            Borrowed(prompt)
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Owned("\x1b[90m".to_owned() + hint + "\x1b[0m")
    }

    fn highlight_char(&self, line: &str, _pos: usize) -> bool {
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

fn is_path_like(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('./') || s.starts_with('../') || s.starts_with('~') || s.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_')
}

fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        if let Ok(home) = env::var("HOME") {
            format!("{}{}", home, &s[1..])
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    }
}

fn get_git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn load_config() -> HkConfig {
    let home = env::var("HOME").unwrap_or_default();
    let config_path = format!("{}/.hshrc", home);
    let mut config = load_hk_file(&config_path).unwrap_or_else(|_| IndexMap::new());
    resolve_interpolations(&mut config).ok();
    config
}

fn get_aliases(config: &HkConfig) -> HashMap<String, String> {
    config
        .get("aliases")
        .and_then(|v| v.as_map().ok())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
                .collect()
        })
        .unwrap_or_default()
}

fn ensure_executable(file_path: &str) {
    let path = Path::new(file_path);
    if let Ok(meta) = metadata(path) {
        let mut perms = meta.permissions();
        if (perms.mode() & 0o111) == 0 {
            perms.set_mode(perms.mode() | 0o111);
            if let Err(e) = fs::set_permissions(path, perms) {
                eprintln!("Failed to set executable permissions: {}", e);
            }
        }
    }
}

fn handle_builtin(cmd: &str, rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>) -> bool {
    let trimmed = cmd.trim();
    match trimmed {
        "exit" => std::process::exit(0),
        "history" => {
            for (i, entry) in rl.history().iter().rev().enumerate() {
                println!("{}: {}", i + 1, entry);
            }
            true
        }
        "hsh-help" => {
            println!("hsh - HackerOS Shell");
            println!("Built-ins:");
            println!("  exit - exit the shell");
            println!("  history - show command history");
            println!("  hsh-help - show this help");
            println!("Features:");
            println!("  Auto-chmod for .sh files");
            println!("  Auto hl run for .hl files");
            println!("  Git branch in prompt");
            println!("  Aliases from ~/.hshrc");
            println!("  Smart suggestions");
            println!("  Syntax highlighting");
            println!("  Auto-sudo for system files");
            true
        }
        _ => false,
    }
}

fn is_root() -> bool {
    unsafe { getuid() == 0 }
}

fn check_auto_sudo(input: &str) -> String {
    let mut new_input = input.trim().to_string();

    if let Some(parts) = shlex::split(&new_input) {
        if parts.is_empty() {
            return new_input;
        }
        let cmd = &parts[0];
        if ["vi", "vim", "nano"].contains(&cmd.as_str()) && parts.len() > 1 {
            let file = &parts[1];
            if (file.starts_with("/etc/") || file.starts_with("/usr/bin/")) && !is_root() {
                print!("This file requires root privileges. Use sudo? [y/n] ");
                io::stdout().flush().ok();
                let mut answer = String::new();
                io::stdin().read_line(&mut answer).ok();
                if answer.trim().to_lowercase() == "y" {
                    new_input = format!("sudo {}", parts.join(" "));
                }
            }
        }
    }

    new_input
}

fn execute_command(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    shell: &mut BrushShell,
) -> io::Result<()> {
    let mut trimmed = input.trim().to_string();

    // Expand aliases
    let parts: Vec<String> = shlex::split(&trimmed).unwrap_or_default();
    if !parts.is_empty() {
        if let Some(alias_value) = aliases.get(&parts[0]) {
            trimmed = format!("{} {}", alias_value, parts[1..].join(" "));
        }
    }

    let trimmed_ref = trimmed.as_str();

    if handle_builtin(trimmed_ref, rl) {
        return Ok(());
    }

    trimmed = check_auto_sudo(&trimmed);

    let trimmed_ref = trimmed.as_str();

    if trimmed_ref.starts_with("export ") {
        let export_str = &trimmed_ref[7..].trim();
        if let Some(eq_pos) = export_str.find('=') {
            let name = export_str[..eq_pos].trim().to_string();
            let value = export_str[eq_pos + 1..].trim().to_string();
            env::set_var(name, value);
            return Ok(());
        }
    }

    if trimmed_ref.ends_with(".sh") {
        ensure_executable(trimmed_ref);
    } else if trimmed_ref.ends_with(".hl") {
        trimmed = format!("hl run {}", trimmed_ref);
    }

    // Execute using brush-shell
    match shell.execute(&trimmed) {
        Ok(status) => {
            if !status.success() {
                if let Some(code) = status.code() {
                    eprintln!("Command failed with exit code: {}", code);
                }
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Error executing command: {}", e);
            Ok(())
        }
    }
}

fn main() -> rustyline::Result<()> {
    let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();
    let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> = Editor::with_config(config);
    let helper = ShellHelper::new();
    rl.set_helper(Some(helper));

    // Bind Ctrl+L to clear screen
    rl.bind_sequence(KeyEvent::ctrl('l'), Cmd::ClearScreen);

    // Bind Alt+P for fast-look preview
    rl.bind_sequence("Alt-p", |rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>| {
        let line = rl.buffer().to_string();
        let last_word = line.split_whitespace().last().unwrap_or("");
        if !last_word.is_empty() {
            let path = Path::new(last_word);
            if path.exists() {
                if let Ok(content) = read_to_string(path) {
                    let preview = content.lines().take(5).collect::<Vec<_>>().join("\n");
                    println!("\nPreview of {}:\n{}\n", last_word, preview);
                } else {
                    println!("\nCannot read file {}\n", last_word);
                }
            } else {
                println!("\nFile {} does not exist\n", last_word);
            }
        }
        rl.print_crlf().ok();
        rl.redisplay();
        true
    });

    let home = env::var("HOME").unwrap_or_default();
    let history_path = format!("{}/.hsh-history", home);
    if rl.load_history(&history_path).is_err() {
        println!("No previous history.");
    }

    let hk_config = load_config();
    let aliases = get_aliases(&hk_config);

    let env_map: HashMap<String, String> = env::vars().collect();
    let mut shell = brush_core::Shell::new(env_map);

    loop {
        let current_dir = env::current_dir().unwrap_or(PathBuf::from("/"));
        let git_info = get_git_branch()
            .map(|b| format!(" \x1b[1;33m({})\x1b[0m", b))
            .unwrap_or_default();
        let time = Local::now().format("%H:%M").to_string();
        let prompt = format!(
            "\x1b[1;36m[{}]\x1b[0m \x1b[1;34m{}\x1b[0m{} \x1b[1;32mhsh>\x1b[0m ",
            time, current_dir.display(), git_info
        );
        rl.helper_mut().expect("No helper").colored_prompt = prompt.clone();

        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let trimmed_line = line.trim();
                if !trimmed_line.is_empty() {
                    rl.add_history_entry(&line);
                }
                execute_command(&line, &aliases, &mut rl, &mut shell)?;
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
    rl.save_history(&history_path)
}
