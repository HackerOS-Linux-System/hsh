use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use rustyline::Editor;

use crate::helper::ShellHelper;
use crate::history::ShellHistory;
use crate::jobs::JobTable;
use crate::vars;

fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

pub fn expand_tilde(s: &str) -> String {
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

/// Returns Some(exit_code) if handled, None if not a builtin
pub fn handle_builtin(
    cmd: &str,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    shell_history: &ShellHistory,
    aliases: &HashMap<String, String>,
    dry_run: bool,
) -> Option<i32> {
    let trimmed = cmd.trim();

    // cd
    if trimmed == "cd" || trimmed.starts_with("cd ") {
        let dir_str = trimmed.strip_prefix("cd").unwrap_or("").trim();
        let target_dir = if dir_str.is_empty() {
            env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else if dir_str == "-" {
            if let Some(pd) = prev_dir.take() {
                pd.to_string_lossy().to_string()
            } else {
                println!("cd: no previous directory");
                return Some(1);
            }
        } else {
            expand_tilde(dir_str)
        };
        if dry_run {
            println!("[dry-run] cd {}", target_dir);
            return Some(0);
        }
        let current = env::current_dir().unwrap_or(PathBuf::from("/"));
        if env::set_current_dir(&target_dir).is_ok() {
            *prev_dir = Some(current);
            Some(0)
        } else {
            eprintln!("cd: no such file or directory: {}", target_dir);
            Some(1)
        }
    }
    // exit
    else if trimmed == "exit" || trimmed.starts_with("exit ") {
        let code: i32 = trimmed
        .strip_prefix("exit")
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(0);
        std::process::exit(code);
    }
    // history
    else if trimmed == "history" || trimmed.starts_with("history ") {
        let arg = trimmed.strip_prefix("history").unwrap_or("").trim();
        if arg.is_empty() {
            shell_history.print_all();
        } else {
            // fuzzy search
            let results = shell_history.fuzzy_search(arg);
            for entry in results.iter().take(20) {
                println!(
                    "  {}  {}",
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                         entry.command
                );
            }
        }
        Some(0)
    }
    // which / type
    else if trimmed.starts_with("which ") || trimmed.starts_with("type ") {
        let name = if trimmed.starts_with("which ") {
            trimmed.strip_prefix("which ").unwrap_or("").trim()
        } else {
            trimmed.strip_prefix("type ").unwrap_or("").trim()
        };
        resolve_type(name, aliases);
        Some(0)
    }
    // jobs
    else if trimmed == "jobs" {
        jobs.list();
        Some(0)
    }
    // fg
    else if trimmed == "fg" || trimmed.starts_with("fg ") {
        let id: usize = trimmed
        .strip_prefix("fg")
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(1);
        if let Some(pid) = jobs.fg(id) {
            use nix::sys::wait::waitpid;
            use nix::unistd::Pid;
            let _ = waitpid(Pid::from_raw(pid as i32), None);
            jobs.mark_done(id);
        } else {
            eprintln!("fg: job {} not found", id);
            return Some(1);
        }
        Some(0)
    }
    // export
    else if trimmed.starts_with("export ") {
        let export_str = trimmed.strip_prefix("export ").unwrap_or("").trim();
        if let Some(eq_pos) = export_str.find('=') {
            let name = export_str[..eq_pos].trim();
            let value = export_str[eq_pos + 1..].trim();
            if dry_run {
                println!("[dry-run] export {}={}", name, value);
            } else {
                env::set_var(name, value);
            }
        }
        Some(0)
    }
    // hsh-help
    else if trimmed == "hsh-help" {
        print_help();
        Some(0)
    }
    else {
        None
    }
}

fn resolve_type(name: &str, aliases: &HashMap<String, String>) {
    // Check builtins
    let builtins = ["cd", "exit", "history", "which", "type", "jobs", "fg", "bg", "export", "hsh-help", "source"];
    if builtins.contains(&name) {
        println!("{} is a shell builtin", name);
        return;
    }
    // Check aliases
    if let Some(val) = aliases.get(name) {
        println!("{} is aliased to '{}'", name, val);
        return;
    }
    // Check PATH
    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            let full = Path::new(dir).join(name);
            if full.exists() {
                println!("{} is {}", name, full.display());
                return;
            }
        }
    }
    println!("{}: not found", name);
}

fn print_help() {
    println!("\x1b[1;32mhsh\x1b[0m — HackerOS Shell v0.2");
    println!();
    println!("\x1b[1mBuilt-in commands:\x1b[0m");
    println!("  cd [dir|-]       Change directory (- goes back)");
    println!("  exit [code]      Exit shell");
    println!("  history [query]  Show history; with query: fuzzy search");
    println!("  which/type NAME  Show if alias, builtin, or binary");
    println!("  jobs             List background jobs");
    println!("  fg [id]          Bring job to foreground");
    println!("  export KEY=VAL   Set environment variable");
    println!("  source FILE      Execute file in current shell");
    println!("  hsh-help         This help");
    println!();
    println!("\x1b[1mFeatures:\x1b[0m");
    println!("  Syntax highlighting with dangerous-command detection");
    println!("  Timestamped history with fuzzy search");
    println!("  Inline env vars:  FOO=bar command");
    println!("  Glob expansion:   ls *.rs");
    println!("  Background jobs:  command &");
    println!("  Configurable prompt via ~/.hshrc [prompt]");
    println!("  Dry-run mode:     hsh --dry-run");
    println!("  -c flag:          hsh -c 'command'");
}
