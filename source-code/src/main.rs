use ansi_str::AnsiStr;
use chrono::Local;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{MatchingBracketValidator, Validator};
use rustyline::{Cmd, CompletionType, Config, Context, EditMode, Editor, KeyEvent};
use rustyline_derive::Helper;
use std::borrow::Cow::{self, Borrowed, Owned};
use std::collections::HashMap;
use std::env;
use std::fs::{self, metadata, read_dir, read_to_string};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use hk_parser::{load_hk_file, resolve_interpolations, HkConfig};
use indexmap::IndexMap;
use libc::getuid;
use shlex;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, System, RefreshKind};
use terminal_size::terminal_size;
#[derive(Helper)]
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
                // Double quoted string
                let start = i;
                i += 1;
                while i < line.len() && (line.as_bytes()[i] as char) != '"' {
                    i += 1;
                }
                if i < line.len() {
                    i += 1;
                }
                let string_part = &line[start..i];
                highlighted.push_str(&format!("\x1b[35m{}\x1b[0m", string_part));
                is_command_position = false;
            } else if c == '\'' {
                // Single quoted string
                let start = i;
                i += 1;
                while i < line.len() && (line.as_bytes()[i] as char) != '\'' {
                    i += 1;
                }
                if i < line.len() {
                    i += 1;
                }
                let string_part = &line[start..i];
                highlighted.push_str(&format!("\x1b[35m{}\x1b[0m", string_part));
                is_command_position = false;
            } else if c == '$' {
                // Variable
                let start = i;
                i += 1;
                while i < line.len() {
                    let next_c = line.as_bytes()[i] as char;
                    if !next_c.is_alphanumeric() && next_c != '_' {
                        break;
                    }
                    i += 1;
                }
                let var_part = &line[start..i];
                highlighted.push_str(&format!("\x1b[94m{}\x1b[0m", var_part));
                is_command_position = false;
            } else if "&|;>".contains(c) || c == '<' {
                // Operators
                let start = i;
                if i + 1 < line.len() {
                    let next_c = line.as_bytes()[i + 1] as char;
                    if (c == '&' && next_c == '&') || (c == '|' && next_c == '|') {
                        i += 2;
                        let op = &line[start..i];
                        highlighted.push_str(&format!("\x1b[95m{}\x1b[0m", op)); // magenta for && ||
                    } else {
                        i += 1;
                        let op = &line[start..i];
                        if op == ";" {
                            highlighted.push_str(&format!("\x1b[33m{}\x1b[0m", op)); // yellow for ;
                        } else if op == "|" || op == ">" || op == "<" {
                            highlighted.push_str(&format!("\x1b[1;37m{}\x1b[0m", op)); // white bold for | > <
                        } else {
                            highlighted.push_str(op);
                        }
                    }
                } else {
                    i += 1;
                    let op = &line[start..i];
                    if op == ";" {
                        highlighted.push_str(&format!("\x1b[33m{}\x1b[0m", op));
                    } else if op == "|" || op == ">" || op == "<" {
                        highlighted.push_str(&format!("\x1b[1;37m{}\x1b[0m", op));
                    } else {
                        highlighted.push_str(op);
                    }
                }
                is_command_position = true;
            } else {
                // Word or other
                let start = i;
                while i < line.len() {
                    let next_c = line.as_bytes()[i] as char;
                    if next_c.is_whitespace() || "&|;><\"'$".contains(next_c) {
                        break;
                    }
                    i += 1;
                }
                let word = &line[start..i];
                let color = if is_command_position {
                    if self.command_exists(word) {
                        "\x1b[32m" // green
                    } else {
                        "\x1b[31m" // red
                    }
                } else if word.starts_with('-') || word.starts_with("--") {
                    "\x1b[33m" // yellow for options
                } else if is_path_like(word) && Path::new(&expand_tilde(word)).exists() {
                    "\x1b[36m" // cyan for paths
                } else {
                    "" // default
                };
                highlighted.push_str(&format!("{}{}\x1b[0m", color, word));
                is_command_position = false;
            }
        }
        Owned(highlighted)
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
fn is_path_like(s: &str) -> bool {
    s.starts_with('/')
    || s.starts_with("./")
    || s.starts_with("../")
    || s.starts_with('~')
    || s.chars().all(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_')
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
fn get_prompt_config(config: &HkConfig) -> HashMap<String, String> {
    config
    .get("prompt")
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
fn handle_builtin(cmd: &str, rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>, prev_dir: &mut Option<PathBuf>) -> bool {
    let trimmed = cmd.trim();
    if trimmed.starts_with("cd") {
        let dir_str = trimmed.strip_prefix("cd").unwrap_or("").trim();
        let target_dir = if dir_str.is_empty() {
            env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else if dir_str == "-" {
            if let Some(pd) = prev_dir.take() {
                pd.to_string_lossy().to_string()
            } else {
                println!("No previous directory");
                return true;
            }
        } else {
            expand_tilde(dir_str)
        };
        let current = env::current_dir().unwrap_or(PathBuf::from("/"));
        if env::set_current_dir(&target_dir).is_ok() {
            *prev_dir = Some(current);
        } else {
            println!("cd: no such file or directory: {}", dir_str);
        }
        true
    } else {
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
                println!(" exit - exit the shell");
                println!(" history - show command history");
                println!(" hsh-help - show this help");
                println!(" cd [dir] - change directory");
                println!("Features:");
                println!(" Auto-chmod for .sh files");
                println!(" Auto hl run for .hl files");
                println!(" Git branch in prompt");
                println!(" Aliases from ~/.hshrc");
                println!(" Smart suggestions");
                println!(" Syntax highlighting");
                println!(" Auto-sudo for system files");
                println!(" Configurable prompt in ~/.hshrc [prompt] section");
                true
            }
            _ => false,
        }
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
async fn execute_command(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
) -> io::Result<i32> {
    let mut trimmed = input.trim().to_string();
    // Expand aliases
    let parts: Vec<String> = shlex::split(&trimmed).unwrap_or_default();
    if !parts.is_empty() {
        if let Some(alias_value) = aliases.get(&parts[0]) {
            trimmed = format!("{} {}", alias_value, parts[1..].join(" "));
        }
    }
    let trimmed_ref = trimmed.as_str();
    if trimmed_ref.starts_with("source ") || trimmed_ref.starts_with(". ") {
        let offset = if trimmed_ref.starts_with("source ") { 7 } else { 2 };
        let file_path = expand_tilde(&trimmed_ref[offset..].trim());
        let contents = read_to_string(&file_path)?;
        let mut last_code = 0;
        for line in contents.lines() {
            let trimmed_line = line.trim();
            if !trimmed_line.is_empty() && !trimmed_line.starts_with('!') {
                last_code = Box::pin(execute_command(line, aliases, rl, prev_dir)).await?;
            }
        }
        return Ok(last_code);
    }
    if handle_builtin(trimmed_ref, rl, prev_dir) {
        return Ok(0);
    }
    trimmed = check_auto_sudo(&trimmed);
    let trimmed_ref = trimmed.as_str();
    if trimmed_ref.starts_with("export ") {
        let export_str = &trimmed_ref[7..].trim();
        if let Some(eq_pos) = export_str.find('=') {
            let name = export_str[..eq_pos].trim().to_string();
            let value = export_str[eq_pos + 1..].trim().to_string();
            env::set_var(name, value);
            return Ok(0);
        }
    }
    if trimmed_ref.ends_with(".sh") {
        ensure_executable(trimmed_ref);
    } else if trimmed_ref.ends_with(".hl") {
        trimmed = format!("hl run {}", trimmed_ref);
    }
    // Execute using spawn for interactivity
    let status = tokio::process::Command::new("sh")
    .arg("-c")
    .arg(&trimmed)
    .spawn()?
    .wait()
    .await?;
    Ok(status.code().unwrap_or(1))
}
#[tokio::main]
async fn main() -> rustyline::Result<()> {
    // Run MOTD script if exists
    if let Ok(mut child) = tokio::process::Command::new("sh")
        .arg("-c")
        .arg("/usr/share/HackerOS/Archived/MOTD/hackeros-motd")
        .spawn() {
            let _ = child.wait().await;
        }

        let config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();
        let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> = Editor::with_config(config)?;
        let helper = ShellHelper::new();
        rl.set_helper(Some(helper));
        // Bind Ctrl+L to clear screen
        rl.bind_sequence(KeyEvent::ctrl('l'), Cmd::ClearScreen);
        let home = env::var("HOME").unwrap_or_default();
        let history_path = format!("{}/.hsh-history", home);
        if rl.load_history(&history_path).is_err() {
            println!("No previous history.");
        }
        let hk_config = load_config();
        let aliases = get_aliases(&hk_config);
        let prompt_cfg = get_prompt_config(&hk_config);
        let mut prev_dir: Option<PathBuf> = None;
        let mut last_exit_code = 0;
        let mut system = System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::everything()).with_cpu(CpuRefreshKind::everything()));
        loop {
            system.refresh_memory();
            system.refresh_cpu();
            let current_dir = env::current_dir().unwrap_or(PathBuf::from("/"));
            let git_branch = get_git_branch();
            let time_color = prompt_cfg.get("time_color").cloned().unwrap_or("\x1b[1;36m".to_string());
            let dir_symbol = prompt_cfg.get("dir_symbol").cloned().unwrap_or("\u{1F4C1}".to_string());
            let dir_color = prompt_cfg.get("dir_color").cloned().unwrap_or("\x1b[1;34m".to_string());
            let git_symbol = prompt_cfg.get("git_symbol").cloned().unwrap_or("\u{E0A0}".to_string());
            let git_color = prompt_cfg.get("git_color").cloned().unwrap_or("\x1b[1;33m".to_string());
            let prompt_color = prompt_cfg.get("prompt_color").cloned().unwrap_or("\x1b[1;32m".to_string());
            let error_symbol_str = prompt_cfg.get("error_symbol").cloned().unwrap_or("\u{2718}".to_string());
            let root_symbol_str = prompt_cfg.get("root_symbol").cloned().unwrap_or("\u{26A1}".to_string());
            let git_info = git_branch
            .map(|b| format!("{}({} {}){}", git_color, git_symbol, b, "\x1b[0m"))
            .unwrap_or_default();
            let time = Local::now().format("%H:%M").to_string();
            let root_symbol = if is_root() { format!("{} ", root_symbol_str) } else { "".to_string() };
            let error_symbol = if last_exit_code != 0 { format!("\x1b[31m{}\x1b[0m ", error_symbol_str) } else { "".to_string() };
            let used_mem_gb = system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
            let cpu_usage = system.cpus().first().map(|c| c.cpu_usage()).unwrap_or(0.0);
            let rprompt = format!("mem: {:.1}GB  cpu: {:.0}%", used_mem_gb, cpu_usage);
            let left_first_line = format!(
                "╭─ {time_color}[{}]\x1b[0m {dir_color}{} {}\x1b[0m{}",
                time, dir_symbol, current_dir.display(), git_info
            );
            let left_len = left_first_line.ansi_strip().len();
            let rprompt_len = rprompt.ansi_strip().len(); // no ansi in rprompt
            let term_width = terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(80);
            let spaces = if term_width > left_len + rprompt_len { term_width - left_len - rprompt_len } else { 0 };
            let first_line = format!("{}{}{}", left_first_line, " ".repeat(spaces), rprompt);
            let second_line = format!("{prompt_color}╰─ {}{}hsh❯\x1b[0m ", error_symbol, root_symbol);
            let prompt = format!("{}\n{}", first_line, second_line);
            rl.helper_mut().expect("No helper").colored_prompt = prompt.clone();
            let readline = rl.readline(&prompt);
            match readline {
                Ok(line) => {
                    let trimmed_line = line.trim();
                    if !trimmed_line.is_empty() {
                        rl.add_history_entry(&line);
                    }
                    last_exit_code = execute_command(&line, &aliases, &mut rl, &mut prev_dir).await.unwrap_or(1);
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
