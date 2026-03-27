use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use rustyline::Editor;

use crate::helper::ShellHelper;
use crate::history::ShellHistory;
use crate::jobs::JobTable;
use crate::vars::ShellVars;

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

fn parse_job_id(s: &str) -> Option<usize> {
    if s.starts_with('%') {
        s[1..].parse().ok()
    } else if s == "%" {
        Some(1)
    } else {
        None
    }
}

pub fn handle_builtin(
    cmd: &str,
    _rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    shell_history: &ShellHistory,
    aliases: &HashMap<String, String>,
    dry_run: bool,
    vars: &mut ShellVars,
    _heredoc_bodies: &HashMap<String, String>,
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
            vars.set_pwd();
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
        let id_str = trimmed.strip_prefix("fg").unwrap_or("").trim();
        let id = if id_str.is_empty() { 1 } else { parse_job_id(id_str).unwrap_or(1) };
        if let Some(pid) = jobs.fg(id) {
            use nix::sys::wait::waitpid;
            use nix::unistd::Pid;
            let _ = waitpid(Pid::from_raw(pid as i32), None);
            jobs.mark_done(id);
            Some(0)
        } else {
            eprintln!("fg: job {} not found", id);
            Some(1)
        }
    }
    // bg
    else if trimmed == "bg" || trimmed.starts_with("bg ") {
        let id_str = trimmed.strip_prefix("bg").unwrap_or("").trim();
        let id = if id_str.is_empty() { 1 } else { parse_job_id(id_str).unwrap_or(1) };
        if jobs.bg(id) {
            println!("[{}] continued", id);
            Some(0)
        } else {
            eprintln!("bg: job {} not found", id);
            Some(1)
        }
    }
    // stop
    else if trimmed == "stop" || trimmed.starts_with("stop ") {
        let id_str = trimmed.strip_prefix("stop").unwrap_or("").trim();
        let id = if id_str.is_empty() { 1 } else { parse_job_id(id_str).unwrap_or(1) };
        if jobs.stop(id) {
            println!("[{}] stopped", id);
            Some(0)
        } else {
            eprintln!("stop: job {} not found", id);
            Some(1)
        }
    }
    // kill
    else if trimmed.starts_with("kill ") {
        let args: Vec<&str> = trimmed.strip_prefix("kill ").unwrap_or("").split_whitespace().collect();
        if args.is_empty() { eprintln!("kill: usage: kill [-SIG] %id"); return Some(1); }
        let mut signal = nix::sys::signal::Signal::SIGTERM;
        let mut id_str = args[0];
        if args[0].starts_with('-') {
            let sig_name = &args[0][1..];
            signal = match sig_name {
                "TERM" => nix::sys::signal::Signal::SIGTERM,
                "KILL" => nix::sys::signal::Signal::SIGKILL,
                "INT"  => nix::sys::signal::Signal::SIGINT,
                "STOP" => nix::sys::signal::Signal::SIGSTOP,
                "CONT" => nix::sys::signal::Signal::SIGCONT,
                _ => { eprintln!("kill: unknown signal {}", sig_name); return Some(1); }
            };
            id_str = args.get(1).unwrap_or(&"");
        }
        let id = parse_job_id(id_str).unwrap_or(1);
        if jobs.send_signal(id, signal) {
            println!("[{}] killed", id);
            Some(0)
        } else {
            eprintln!("kill: job {} not found", id);
            Some(1)
        }
    }
    // wait
    else if trimmed.starts_with("wait ") {
        let id_str = trimmed.strip_prefix("wait ").unwrap_or("").trim();
        let id = parse_job_id(id_str).unwrap_or(1);
        if let Some(pid) = jobs.fg(id) {
            use nix::sys::wait::waitpid;
            use nix::unistd::Pid;
            let status = waitpid(Pid::from_raw(pid as i32), None);
            jobs.mark_done(id);
            match status {
                Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => Some(code),
                Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                    eprintln!("wait: job terminated by signal {}", sig);
                    Some(128 + (sig as i32))
                }
                _ => Some(1),
            }
        } else {
            eprintln!("wait: job {} not found", id);
            Some(1)
        }
    }
    // export
    else if trimmed.starts_with("export ") {
        let export_str = trimmed.strip_prefix("export ").unwrap_or("").trim();
        if export_str == "-p" {
            let mut vars: Vec<(String, String)> = env::vars().collect();
            vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in vars {
                println!("export {}={}", k, v);
            }
            return Some(0);
        }
        if let Some(eq_pos) = export_str.find('=') {
            let name = export_str[..eq_pos].trim();
            let value = export_str[eq_pos + 1..].trim();
            if dry_run {
                println!("[dry-run] export {}={}", name, value);
            } else {
                env::set_var(name, value);
                vars.set(name, value);
            }
        }
        Some(0)
    }
    // alias
    else if trimmed == "alias" {
        for (k, v) in aliases {
            println!("alias {}='{}'", k, v);
        }
        Some(0)
    }
    // unalias
    else if trimmed.starts_with("unalias ") {
        let name = trimmed.strip_prefix("unalias ").unwrap_or("").trim();
        if name.is_empty() {
            eprintln!("unalias: usage: unalias name");
            return Some(1);
        }
        // W praktyce musielibyśmy modyfikować mapę aliasów, która jest tylko do odczytu w tej funkcji.
        // Dla uproszczenia wypisujemy komunikat.
        eprintln!("unalias not fully implemented (aliases are read-only)");
        Some(1)
    }
    // set
    else if trimmed.starts_with("set ") {
        let args: Vec<&str> = trimmed.strip_prefix("set ").unwrap_or("").split_whitespace().collect();
        for arg in args {
            match arg {
                "-e" => vars.set_option("e", true),
                "+e" => vars.set_option("e", false),
                "-x" => vars.set_option("x", true),
                "+x" => vars.set_option("x", false),
                "-u" => vars.set_option("u", true),
                "+u" => vars.set_option("u", false),
                _ => eprintln!("set: unknown option {}", arg),
            }
        }
        Some(0)
    }
    // pushd
    else if trimmed == "pushd" || trimmed.starts_with("pushd ") {
        let dir_str = trimmed.strip_prefix("pushd").unwrap_or("").trim();
        let target = if dir_str.is_empty() {
            env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else {
            expand_tilde(dir_str)
        };
        let current = env::current_dir().unwrap_or_default().to_string_lossy().to_string();
        vars.dir_stack.push(current);
        if env::set_current_dir(&target).is_ok() {
            vars.set_pwd();
            println!("{}", target);
            Some(0)
        } else {
            eprintln!("pushd: {}: no such directory", target);
            Some(1)
        }
    }
    // popd
    else if trimmed == "popd" {
        if let Some(prev) = vars.dir_stack.pop() {
            if env::set_current_dir(&prev).is_ok() {
                vars.set_pwd();
                println!("{}", prev);
                Some(0)
            } else {
                eprintln!("popd: cannot change to {}", prev);
                Some(1)
            }
        } else {
            eprintln!("popd: directory stack empty");
            Some(1)
        }
    }
    // dirs
    else if trimmed == "dirs" {
        let current = env::current_dir().unwrap_or_default().to_string_lossy().to_string();
        println!("{}", current);
        for d in &vars.dir_stack {
            println!("  {}", d);
        }
        Some(0)
    }
    // source / .
    else if trimmed.starts_with("source ") || trimmed.starts_with(". ") {
        let path = if trimmed.starts_with("source ") {
            trimmed.strip_prefix("source ").unwrap_or("").trim()
        } else {
            trimmed.strip_prefix(". ").unwrap_or("").trim()
        };
        if path.is_empty() {
            eprintln!("source: missing file");
            return Some(1);
        }
        // source jest obsługiwane w execute.rs, więc tutaj zwracamy None, żeby tam trafiło
        None
    }
    // hsh-help
    else if trimmed == "hsh-help" {
        print_help();
        Some(0)
    }
    // help (alias)
    else if trimmed == "help" {
        print_help();
        Some(0)
    }
    else {
        None
    }
}

fn resolve_type(name: &str, aliases: &HashMap<String, String>) {
    let builtins = [
        "cd", "exit", "history", "which", "type", "jobs", "fg", "bg", "stop",
        "kill", "wait", "export", "alias", "unalias", "set", "pushd", "popd",
        "dirs", "source", "hsh-help", "help"
    ];
    if builtins.contains(&name) {
        println!("{} is a shell builtin", name);
        return;
    }
    if let Some(val) = aliases.get(name) {
        println!("{} is aliased to '{}'", name, val);
        return;
    }
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
    println!("\x1b[1;32mhsh\x1b[0m — HackerOS Shell v0.3.5");
    println!();
    println!("\x1b[1mBuilt-in commands:\x1b[0m");
    println!("  cd [dir|-]       Change directory (- goes back)");
    println!("  exit [code]      Exit shell");
    println!("  history [query]  Show history; with query: fuzzy search");
    println!("  which/type NAME  Show if alias, builtin, or binary");
    println!("  jobs             List background jobs");
    println!("  fg [id]          Bring job to foreground");
    println!("  bg [id]          Resume job in background");
    println!("  stop [id]        Suspend job");
    println!("  kill [-SIG] %id  Send signal to job");
    println!("  wait [id]        Wait for job to finish");
    println!("  export KEY=VAL   Set environment variable");
    println!("  export -p        Print all exported variables");
    println!("  alias            List aliases");
    println!("  set [-e] [-x] [-u]  Set shell options");
    println!("  pushd [dir]      Push directory onto stack");
    println!("  popd             Pop directory from stack");
    println!("  dirs             Show directory stack");
    println!("  source FILE      Execute file in current shell");
    println!("  help             Show this help");
    println!("  hsh-help         Alias for help");
    println!();
    println!("\x1b[1mNative commands (built into hsh):\x1b[0m");
    println!("  echo  pwd  ls  cat  mkdir  rm  cp  mv  touch  env");
    println!("  grep  head  tail  wc  uname  find  xargs  printf");
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
