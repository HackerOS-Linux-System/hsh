use std::collections::HashMap;
use std::env;
use std::fs::{metadata, read_to_string};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use rustyline::Editor;
use tokio::process::Command as TokioCmd;

use crate::builtins::{expand_tilde, handle_builtin};
use crate::helper::ShellHelper;
use crate::history::ShellHistory;
use crate::jobs::JobTable;
use crate::security::confirm_dangerous;
use crate::vars::{parse_inline_env, ShellVars};

fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

fn ensure_executable(file_path: &str) {
    let path = Path::new(file_path);
    if let Ok(meta) = metadata(path) {
        let mut perms = meta.permissions();
        if (perms.mode() & 0o111) == 0 {
            perms.set_mode(perms.mode() | 0o111);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
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
                eprint!("This file requires root privileges. Use sudo? [y/n] ");
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

/// Expand globs in a list of arguments
fn expand_globs(args: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for arg in args {
        // Only expand if it looks like a glob
        if arg.contains('*') || arg.contains('?') || (arg.contains('{') && arg.contains('}')) {
            let pattern = expand_tilde(&arg);
            match glob::glob(&pattern) {
                Ok(paths) => {
                    let expanded: Vec<String> = paths
                    .filter_map(|p| p.ok())
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                    if expanded.is_empty() {
                        result.push(arg); // keep original if no match
                    } else {
                        result.extend(expanded);
                    }
                }
                Err(_) => result.push(arg),
            }
        } else {
            result.push(arg);
        }
    }
    result
}

/// Split a command line into pipeline stages by `|`
fn split_pipeline(input: &str) -> Vec<String> {
    // Naive split respecting quotes
    let mut stages = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in input.chars() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '|' if !in_single && !in_double => {
                stages.push(current.trim().to_string());
                current = String::new();
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        stages.push(current.trim().to_string());
    }
    stages
}

/// Check if command ends with `&` (background)
fn is_background(input: &str) -> (bool, &str) {
    let trimmed = input.trim_end();
    if trimmed.ends_with('&') {
        (true, trimmed[..trimmed.len() - 1].trim())
    } else {
        (false, trimmed)
    }
}

pub async fn execute_command(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    dry_run: bool,
) -> io::Result<i32> {
    // We need a dummy ShellHistory for builtins (it's for printing only)
    let home = env::var("HOME").unwrap_or_default();
    let ts_path = format!("{}/.hsh-history-ts", home);
    let shell_history = ShellHistory::load(&ts_path);

    execute_line(input, aliases, rl, prev_dir, jobs, vars, &shell_history, dry_run).await
}

async fn execute_line(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    shell_history: &ShellHistory,
    dry_run: bool,
) -> io::Result<i32> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(0);
    }

    // Handle compound commands separated by ; or && or ||
    // Simple sequential: split by ; first (outside quotes)
    let statements = split_statements(trimmed);
    if statements.len() > 1 {
        let mut code = 0;
        for stmt in statements {
            code = Box::pin(execute_line(
                &stmt,
                aliases,
                rl,
                prev_dir,
                jobs,
                vars,
                shell_history,
                dry_run,
            ))
            .await?;
        }
        return Ok(code);
    }

    // Expand variables
    let expanded = vars.expand(trimmed);
    let trimmed = expanded.as_str();

    // source
    if trimmed.starts_with("source ") || trimmed.starts_with(". ") {
        let offset = if trimmed.starts_with("source ") { 7 } else { 2 };
        let file_path = expand_tilde(trimmed[offset..].trim());
        if dry_run {
            println!("[dry-run] source {}", file_path);
            return Ok(0);
        }
        let contents = read_to_string(&file_path)?;
        let mut last_code = 0;
        for line in contents.lines() {
            let tl = line.trim();
            if !tl.is_empty() && !tl.starts_with('#') {
                last_code = Box::pin(execute_line(
                    line,
                    aliases,
                    rl,
                    prev_dir,
                    jobs,
                    vars,
                    shell_history,
                    dry_run,
                ))
                .await?;
            }
        }
        return Ok(last_code);
    }

    // Builtin check (before alias/glob expansion)
    if let Some(code) = handle_builtin(trimmed, rl, prev_dir, jobs, shell_history, aliases, dry_run) {
        return Ok(code);
    }

    // Inline env vars: FOO=bar command
    let (inline_env, rest) = parse_inline_env(trimmed);
    let trimmed = if rest.is_empty() {
        // pure assignment(s), no command
        for (k, v) in &inline_env {
            vars.set(k, v);
        }
        return Ok(0);
    } else {
        rest
    };

    // Alias expansion
    let parts: Vec<String> = shlex::split(&trimmed).unwrap_or_default();
    let trimmed = if !parts.is_empty() {
        if let Some(alias_value) = aliases.get(&parts[0]) {
            format!("{} {}", alias_value, parts[1..].join(" "))
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    };

    // Auto-sudo
    let trimmed = check_auto_sudo(&trimmed);

    // Dangerous command check
    if !dry_run && !confirm_dangerous(&trimmed) {
        println!("Command aborted.");
        return Ok(1);
    }

    // Background job
    let (background, trimmed_cmd) = is_background(&trimmed);
    let trimmed_cmd = trimmed_cmd.to_string();

    // .sh auto-chmod
    if trimmed_cmd.ends_with(".sh") {
        ensure_executable(&trimmed_cmd);
    }

    // .hl auto-run
    let trimmed_cmd = if trimmed_cmd.ends_with(".hl") {
        format!("hl run {}", trimmed_cmd)
    } else {
        trimmed_cmd
    };

    // Pipeline stages
    let stages = split_pipeline(&trimmed_cmd);

    if dry_run {
        println!("[dry-run] {}", trimmed_cmd);
        return Ok(0);
    }

    if stages.len() == 1 {
        // Simple command
        run_simple(&trimmed_cmd, &inline_env, jobs, background).await
    } else {
        // Native pipeline
        run_pipeline(&stages, &inline_env, background).await
    }
}

async fn run_simple(
    cmd: &str,
    inline_env: &[(String, String)],
                    jobs: &mut JobTable,
                    background: bool,
) -> io::Result<i32> {
    // Handle redirections via sh -c for now (complex parsing)
    // But spawn natively for better job control
    let mut parts = shlex::split(cmd).unwrap_or_default();
    if parts.is_empty() {
        return Ok(0);
    }

    // If contains redirections, delegate to sh
    if cmd.contains('>') || cmd.contains('<') {
        let mut child = TokioCmd::new("sh")
        .arg("-c")
        .arg(cmd)
        .envs(inline_env.iter().map(|(k, v)| (k, v)))
        .spawn()?;
        if background {
            let pid = child.id().unwrap_or(0);
            jobs.add(pid, cmd);
            return Ok(0);
        }
        let status = child.wait().await?;
        return Ok(status.code().unwrap_or(1));
    }

    let program = expand_tilde(&parts[0]);
    let args: Vec<String> = parts.drain(1..).map(|a| expand_tilde(&a)).collect();
    let args = expand_globs(args);

    let mut cmd_builder = TokioCmd::new(&program);
    cmd_builder.args(&args);
    for (k, v) in inline_env {
        cmd_builder.env(k, v);
    }

    let mut child = cmd_builder.spawn()?;

    if background {
        let pid = child.id().unwrap_or(0);
        let cmd_str = format!("{} {}", program, args.join(" "));
        jobs.add(pid, &cmd_str);
        return Ok(0);
    }

    let status = child.wait().await?;
    Ok(status.code().unwrap_or(1))
}

/// Run a native pipeline: cmd1 | cmd2 | cmd3
async fn run_pipeline(stages: &[String], inline_env: &[(String, String)], background: bool) -> io::Result<i32> {
    if stages.is_empty() {
        return Ok(0);
    }
    if stages.len() == 1 {
        let mut child = TokioCmd::new("sh")
        .arg("-c")
        .arg(&stages[0])
        .envs(inline_env.iter().map(|(k, v)| (k, v)))
        .spawn()?;
        let status = child.wait().await?;
        return Ok(status.code().unwrap_or(1));
    }

    let mut children = Vec::new();
    let mut prev_stdout: Option<std::process::ChildStdout> = None;

    for (i, stage) in stages.iter().enumerate() {
        let is_last = i == stages.len() - 1;

        let stdin_cfg = if let Some(prev) = prev_stdout.take() {
            Stdio::from(prev)
        } else {
            Stdio::inherit()
        };

        let stdout_cfg = if is_last {
            Stdio::inherit()
        } else {
            Stdio::piped()
        };

        // Use std::process for piping (tokio's piped stdio is trickier)
        let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(stage)
        .stdin(stdin_cfg)
        .stdout(stdout_cfg)
        .envs(inline_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .spawn()?;

        if !is_last {
            prev_stdout = child.stdout.take();
        }
        children.push(child);
    }

    // Wait for all children; return exit code of last
    let mut last_code = 0;
    for mut child in children {
        let status = child.wait()?;
        last_code = status.code().unwrap_or(1);
    }
    Ok(last_code)
}

/// Split on `;` `&&` `||` outside quotes
fn split_statements(input: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            ';' if !in_single && !in_double => {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    statements.push(s);
                }
                current = String::new();
            }
            '&' if !in_single && !in_double && i + 1 < chars.len() && chars[i + 1] == '&' => {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    statements.push(s);
                }
                current = " &&__COND__ ".to_string(); // placeholder handled below
                i += 2;
                continue;
            }
            '|' if !in_single && !in_double && i + 1 < chars.len() && chars[i + 1] == '|' => {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    statements.push(s);
                }
                current = String::new();
                i += 2;
                continue;
            }
            _ => current.push(c),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        statements.push(current.trim().to_string());
    }

    // If only one statement (no separators found), return it as-is
    if statements.is_empty() {
        vec![input.to_string()]
    } else {
        statements
    }
}
