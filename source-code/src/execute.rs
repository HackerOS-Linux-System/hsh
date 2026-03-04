use std::collections::HashMap;
use std::env;
use std::fs::read_to_string;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use rustyline::Editor;

use crate::builtins::handle_builtin;
use crate::builtins_native::dispatch_native;
use crate::helper::ShellHelper;
use crate::history::ShellHistory;
use crate::jobs::JobTable;
use crate::path_cache::PathCache;
use crate::security::confirm_dangerous;
use crate::smarthints::SmartHints;
use crate::vars::{parse_inline_env, ShellVars};

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub async fn execute_command(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    dry_run: bool,
) -> io::Result<i32> {
    run_line(
        input, aliases, rl, prev_dir, jobs, vars,
        smart_hints, shell_history, path_cache, dry_run,
    )
    .await
}

// ─────────────────────────────────────────────────────────────────────────────
// Statement-level runner  (;  &&  ||)
// ─────────────────────────────────────────────────────────────────────────────

async fn run_line(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    dry_run: bool,
) -> io::Result<i32> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(0);
    }

    let stmts = split_compound(trimmed);

    // Fast path — single statement
    if stmts.len() == 1 {
        return run_single(
            trimmed, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, dry_run,
        )
        .await;
    }

    let mut last_code = 0i32;
    for (stmt, op) in stmts {
        let stmt = stmt.trim().to_string();
        if stmt.is_empty() {
            continue;
        }
        last_code = Box::pin(run_single(
            &stmt, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, dry_run,
        ))
        .await?;

        match op.as_deref() {
            Some("&&") if last_code != 0 => break, // AND short-circuit
            Some("||") if last_code == 0 => break, // OR  short-circuit
            _ => {}
        }
    }
    Ok(last_code)
}

// ─────────────────────────────────────────────────────────────────────────────
// Single statement runner
// ─────────────────────────────────────────────────────────────────────────────

async fn run_single(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    dry_run: bool,
) -> io::Result<i32> {

    // 1. Variable expansion
    let expanded = vars.expand(input);
    let input = expanded.as_str();

    // 2. source / .
    if let Some(path) = strip_source_prefix(input) {
        let path = expand_tilde(&path);
        if dry_run {
            println!("[dry-run] source {}", path);
            return Ok(0);
        }
        return run_source(
            &path, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, dry_run,
        )
        .await;
    }

    // 3. Shell builtins (cd, exit, history, jobs, fg, export, which, …)
    if let Some(code) = handle_builtin(
        input, rl, prev_dir, jobs, shell_history, aliases, dry_run,
    ) {
        return Ok(code);
    }

    // 4. Inline env assignments  (FOO=bar  or  FOO=bar cmd …)
    let (inline_env, rest) = parse_inline_env(input);
    if rest.is_empty() {
        // Pure assignment — write to shell vars AND exported env
        for (k, v) in &inline_env {
            vars.set(k, v);
            env::set_var(k, v);
        }
        return Ok(0);
    }

    // 5. Alias expansion
    let rest = expand_alias(&rest, aliases);

    // 6. Auto-sudo for privileged system files
    let rest = check_auto_sudo(&rest);

    // 7. Dangerous command guard
    if !dry_run && !confirm_dangerous(&rest) {
        println!("Command aborted.");
        return Ok(1);
    }

    // 8. Background flag  (cmd &)
    let (background, rest) = strip_background_flag(&rest);
    let rest = rest.trim().to_string();

    // 9. .sh auto-chmod  /  .hl auto-rewrite
    maybe_chmod(&rest);
    let rest = maybe_hl_run(rest);

    // 10. Dry-run output & early return
    if dry_run {
        println!(
            "[dry-run]{} {}",
            if background { " [bg]" } else { "" },
                rest
        );
        return Ok(0);
    }

    // 11. Pipeline split → dispatch
    let stages = split_pipeline(&rest);
    if stages.len() == 1 {
        run_simple(&rest, &inline_env, jobs, background).await
    } else {
        run_pipeline(&stages, &inline_env, jobs, background).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Simple command  (single process, no pipeline)
// ─────────────────────────────────────────────────────────────────────────────

async fn run_simple(
    cmd: &str,
    inline_env: &[(String, String)],
                    jobs: &mut JobTable,
                    background: bool,
) -> io::Result<i32> {

    // Parse → tilde-expand → glob-expand
    let raw: Vec<String> = shlex::split(cmd).unwrap_or_default();
    if raw.is_empty() {
        return Ok(0);
    }
    let parts = expand_globs(
        raw.into_iter().map(|a| expand_tilde(&a)).collect(),
    );

    let program = &parts[0];
    let argv    = &parts[1..];

    // Native builtins first — no dependency on any installed coreutils
    if let Some(code) = dispatch_native(program, argv) {
        return Ok(code);
    }

    // Delegate redirections to sh (native I/O redirection is a future feature)
    if has_redirections(cmd) {
        return spawn_sh(cmd, inline_env, jobs, background).await;
    }

    // Native process spawn via tokio
    let mut builder = tokio::process::Command::new(program);
    builder.args(argv);
    for (k, v) in inline_env {
        builder.env(k, v);
    }

    match builder.spawn() {
        Ok(mut child) => {
            if background {
                let pid   = child.id().unwrap_or(0);
                let label = format!("{} {}", program, argv.join(" "));
                jobs.add(pid, &label);
                Ok(0)
            } else {
                Ok(child.wait().await?.code().unwrap_or(1))
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("hsh: {}: command not found", program);
            Ok(127)  // standard exit code — triggers spellcheck in main
        }
        Err(e) => {
            eprintln!("hsh: {}: {}", program, e);
            Ok(1)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Native pipeline  (cmd1 | cmd2 | … | cmdN)
// ─────────────────────────────────────────────────────────────────────────────

async fn run_pipeline(
    stages: &[String],
    inline_env: &[(String, String)],
                      jobs: &mut JobTable,
                      background: bool,
) -> io::Result<i32> {
    if stages.is_empty() {
        return Ok(0);
    }
    if stages.len() == 1 {
        return spawn_sh(&stages[0], inline_env, jobs, background).await;
    }

    let mut children: Vec<std::process::Child> = Vec::with_capacity(stages.len());
    let mut prev_stdout: Option<std::process::ChildStdout> = None;

    for (i, stage) in stages.iter().enumerate() {
        let is_last  = i == stages.len() - 1;
        let is_first = i == 0;

        let stdin_cfg: Stdio = prev_stdout
        .take()
        .map(Stdio::from)
        .unwrap_or_else(Stdio::inherit);

        let stdout_cfg: Stdio = if is_last {
            Stdio::inherit()
        } else {
            Stdio::piped()
        };

        // Build argv for this stage
        let raw: Vec<String> = shlex::split(stage).unwrap_or_default();
        let parts = expand_globs(
            raw.into_iter().map(|a| expand_tilde(&a)).collect(),
        );
        if parts.is_empty() {
            continue;
        }

        let mut cmd = std::process::Command::new(&parts[0]);
        cmd.args(&parts[1..]).stdin(stdin_cfg).stdout(stdout_cfg);

        // Inline env only on first stage
        if is_first {
            for (k, v) in inline_env {
                cmd.env(k, v);
            }
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // sh fallback for this one stage
                let mut fb = std::process::Command::new("sh");
                fb.arg("-c").arg(stage)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit());
                match fb.spawn() {
                    Ok(c)   => c,
                    Err(e2) => {
                        eprintln!("hsh: {}: {}", &parts[0], e2);
                        return Ok(127);
                    }
                }
            }
            Err(e) => {
                eprintln!("hsh: {}: {}", &parts[0], e);
                return Ok(1);
            }
        };

        if !is_last {
            prev_stdout = child.stdout.take();
        }
        children.push(child);
    }

    if background {
        // Store first child's PID in job table, detach all
        if let Some(first) = children.first() {
            // std::process::Child::id() returns u32 directly — no Option
            let pid = first.id();
            jobs.add(pid, &stages.join(" | "));
        }
        return Ok(0);
    }

    // Wait for all; return exit code of last stage
    let mut last_code = 0i32;
    for mut child in children {
        match child.wait() {
            Ok(s)  => last_code = s.code().unwrap_or(1),
            Err(e) => eprintln!("hsh: wait: {}", e),
        }
    }
    Ok(last_code)
}

// ─────────────────────────────────────────────────────────────────────────────
// Source file
// ─────────────────────────────────────────────────────────────────────────────

async fn run_source(
    file_path: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    dry_run: bool,
) -> io::Result<i32> {
    let contents = read_to_string(file_path).map_err(|e| {
        eprintln!("hsh: source: {}: {}", file_path, e);
        e
    })?;

    let mut last_code = 0i32;
    for line in contents.lines() {
        let tl = line.trim();
        if tl.is_empty() || tl.starts_with('#') {
            continue;
        }
        last_code = Box::pin(run_line(
            line, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, dry_run,
        ))
        .await?;
    }
    Ok(last_code)
}

// ─────────────────────────────────────────────────────────────────────────────
// sh fallback  (for redirections or unknown binaries in pipeline)
// ─────────────────────────────────────────────────────────────────────────────

async fn spawn_sh(
    cmd: &str,
    inline_env: &[(String, String)],
                  jobs: &mut JobTable,
                  background: bool,
) -> io::Result<i32> {
    let mut child = tokio::process::Command::new("sh")
    .arg("-c")
    .arg(cmd)
    .envs(inline_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    .spawn()?;

    if background {
        jobs.add(child.id().unwrap_or(0), cmd);
        return Ok(0);
    }
    Ok(child.wait().await?.code().unwrap_or(1))
}

// ─────────────────────────────────────────────────────────────────────────────
// Utility functions
// ─────────────────────────────────────────────────────────────────────────────

/// `~/foo` → `/home/user/foo`
pub fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        if let Ok(home) = env::var("HOME") {
            return format!("{}{}", home, &s[1..]);
        }
    }
    s.to_string()
}

/// Expand `*`, `?`, `{a,b}` in argument list.
/// Unmatched globs pass through unchanged (POSIX behaviour).
fn expand_globs(args: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for arg in args {
        if arg.contains('*') || arg.contains('?')
            || (arg.contains('{') && arg.contains('}'))
            {
                match glob::glob(&arg) {
                    Ok(paths) => {
                        let expanded: Vec<String> = paths
                        .filter_map(|p| p.ok())
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                        if expanded.is_empty() {
                            result.push(arg); // no match → keep literal
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

/// Split `cmd1 | cmd2 | cmd3` respecting quotes.
/// Does NOT split on `||`.
fn split_pipeline(input: &str) -> Vec<String> {
    let mut stages = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '\'' if !in_double => { in_single = !in_single; current.push('\''); i += 1; }
            '"'  if !in_single => { in_double = !in_double; current.push('"');  i += 1; }
            '|'  if !in_single && !in_double => {
                if chars.get(i + 1) == Some(&'|') {
                    // `||` — compound operator, not a pipe, keep in current
                    current.push_str("||");
                    i += 2;
                } else {
                    let s = current.trim().to_string();
                    if !s.is_empty() { stages.push(s); }
                    current = String::new();
                    i += 1;
                }
            }
            c => { current.push(c); i += 1; }
        }
    }

    let s = current.trim().to_string();
    if !s.is_empty() { stages.push(s); }
    stages
}

/// Split compound command into `(statement, Option<operator>)` pairs.
/// Recognises `;`, `&&`, `||` outside quotes.
fn split_compound(input: &str) -> Vec<(String, Option<String>)> {
    let mut result: Vec<(String, Option<String>)> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    // Flush current buffer into result with the given operator, then reset.
    macro_rules! flush {
        ($op:expr) => {{
            let s = current.trim().to_string();
            if !s.is_empty() {
                result.push((s, $op));
            }
            current = String::new();
        }};
    }

    while i < chars.len() {
        match chars[i] {
            '\'' if !in_double => { in_single = !in_single; current.push('\''); i += 1; }
            '"'  if !in_single => { in_double = !in_double; current.push('"');  i += 1; }

            ';' if !in_single && !in_double => {
                flush!(Some(";".into()));
                i += 1;
            }
            '&' if !in_single && !in_double && chars.get(i + 1) == Some(&'&') => {
                flush!(Some("&&".into()));
                i += 2;
            }
            '|' if !in_single && !in_double && chars.get(i + 1) == Some(&'|') => {
                flush!(Some("||".into()));
                i += 2;
            }
            c => { current.push(c); i += 1; }
        }
    }

    // Final flush (no trailing operator)
    let s = current.trim().to_string();
    if !s.is_empty() {
        result.push((s, None));
    }

    if result.is_empty() {
        vec![(input.trim().to_string(), None)]
    } else {
        result
    }
}

fn strip_source_prefix(input: &str) -> Option<String> {
    input
    .strip_prefix("source ")
    .or_else(|| input.strip_prefix(". "))
    .map(|s| s.trim().to_string())
}

/// Quick scan for `>` / `<` outside quotes.
fn has_redirections(cmd: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    for c in cmd.chars() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"'  if !in_single => in_double = !in_double,
            '>' | '<' if !in_single && !in_double => return true,
            _ => {}
        }
    }
    false
}

/// Strip trailing `&` (but not `&&`).
/// Returns `(is_background, stripped_cmd)`.
fn strip_background_flag(input: &str) -> (bool, String) {
    let t = input.trim_end();
    if t.ends_with('&') && !t.ends_with("&&") {
        (true, t[..t.len() - 1].trim().to_string())
    } else {
        (false, t.to_string())
    }
}

/// Replace first word with alias value if one exists.
fn expand_alias(input: &str, aliases: &HashMap<String, String>) -> String {
    let parts: Vec<String> = shlex::split(input).unwrap_or_default();
    if let Some(first) = parts.first() {
        if let Some(val) = aliases.get(first.as_str()) {
            let rest = parts[1..].join(" ");
            return if rest.is_empty() {
                val.clone()
            } else {
                format!("{} {}", val, rest)
            };
        }
    }
    input.to_string()
}

/// Prompt for sudo when editing privileged files with a text editor.
fn check_auto_sudo(input: &str) -> String {
    if unsafe { libc::getuid() == 0 } {
        return input.to_string();
    }
    let parts = shlex::split(input).unwrap_or_default();
    if parts.len() < 2 {
        return input.to_string();
    }
    if !["vi", "vim", "nano", "emacs"].contains(&parts[0].as_str()) {
        return input.to_string();
    }
    let file = &parts[1];
    if !file.starts_with("/etc/")
        && !file.starts_with("/usr/")
        && !file.starts_with("/var/")
        && !file.starts_with("/boot/")
        {
            return input.to_string();
        }
        eprint!(
            "\x1b[1;33m⚠  '{}' requires root. Use sudo? [y/n] \x1b[0m",
            file
        );
    io::stdout().flush().ok();
    let mut ans = String::new();
    io::stdin().read_line(&mut ans).ok();
    if ans.trim().eq_ignore_ascii_case("y") {
        format!("sudo {}", input)
    } else {
        input.to_string()
    }
}

/// Auto-chmod +x for .sh files before executing.
fn maybe_chmod(cmd: &str) {
    let first = cmd.split_whitespace().next().unwrap_or("");
    if first.ends_with(".sh") {
        let p = Path::new(first);
        if let Ok(meta) = p.metadata() {
            let mut perms = meta.permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(perms.mode() | 0o111);
                let _ = std::fs::set_permissions(p, perms);
            }
        }
    }
}

/// Rewrite `file.hl` → `hl run file.hl`.
fn maybe_hl_run(cmd: String) -> String {
    let first = cmd.split_whitespace().next().unwrap_or("").to_string();
    if first.ends_with(".hl") {
        format!("hl run {}", cmd)
    } else {
        cmd
    }
}
