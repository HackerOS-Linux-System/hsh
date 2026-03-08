use std::collections::HashMap;
use std::env;
use std::fs::read_to_string;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use rustyline::Editor;

use crate::arithmetic::expand_arithmetic;
use crate::builtins::handle_builtin;
use crate::builtins_native::dispatch_native;
use crate::helper::ShellHelper;
use crate::history::ShellHistory;
use crate::jobs::JobTable;
use crate::path_cache::PathCache;
use crate::redirect::{apply_redirections, parse_redirections, Redirect};
use crate::script::{builtin_test, FunctionTable, Node, Parser};
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
    let mut functions = FunctionTable::new();
    run_line(
        input, aliases, rl, prev_dir, jobs, vars,
        smart_hints, shell_history, path_cache,
        &mut functions, dry_run,
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
    functions: &mut FunctionTable,
    dry_run: bool,
) -> io::Result<i32> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(0);
    }

    if is_script_construct(trimmed) {
        return run_script_node(
            trimmed, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        )
        .await;
    }

    let stmts = split_compound(trimmed);

    if stmts.len() == 1 {
        return run_single(
            trimmed, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        )
        .await;
    }

    let mut last_code = 0i32;
    for (stmt, op) in stmts {
        let stmt = stmt.trim().to_string();
        if stmt.is_empty() { continue; }

        last_code = Box::pin(run_single(
            &stmt, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        ))
        .await?;

        match op.as_deref() {
            Some("&&") if last_code != 0 => break,
            Some("||") if last_code == 0 => break,
            _ => {}
        }
    }
    Ok(last_code)
}

// ─────────────────────────────────────────────────────────────────────────────
// Script construct runner
// ─────────────────────────────────────────────────────────────────────────────

async fn run_script_node(
    input: &str,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    functions: &mut FunctionTable,
    dry_run: bool,
) -> io::Result<i32> {
    let mut parser = Parser::new(input);
    let nodes = parser.parse();
    let mut last = 0i32;
    for node in nodes {
        last = Box::pin(exec_node(
            &node, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        ))
        .await?;
    }
    Ok(last)
}

// ─────────────────────────────────────────────────────────────────────────────
// AST node executor
// ─────────────────────────────────────────────────────────────────────────────

async fn exec_node(
    node: &Node,
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    functions: &mut FunctionTable,
    dry_run: bool,
) -> io::Result<i32> {
    match node {
        Node::Command(cmd) => {
            Box::pin(run_single(
                cmd, aliases, rl, prev_dir, jobs, vars,
                smart_hints, shell_history, path_cache, functions, dry_run,
            ))
            .await
        }

        Node::Sequence(nodes) => {
            let mut last = 0i32;
            for n in nodes {
                last = Box::pin(exec_node(
                    n, aliases, rl, prev_dir, jobs, vars,
                    smart_hints, shell_history, path_cache, functions, dry_run,
                ))
                .await?;
            }
            Ok(last)
        }

        Node::If { condition, then_body, elif_branches, else_body } => {
            let cond_code = Box::pin(exec_node(
                condition, aliases, rl, prev_dir, jobs, vars,
                smart_hints, shell_history, path_cache, functions, dry_run,
            ))
            .await?;

            if cond_code == 0 {
                run_nodes(then_body, aliases, rl, prev_dir, jobs, vars,
                          smart_hints, shell_history, path_cache, functions, dry_run).await
            } else {
                for (elif_cond, elif_body) in elif_branches {
                    let ec = Box::pin(exec_node(
                        elif_cond, aliases, rl, prev_dir, jobs, vars,
                        smart_hints, shell_history, path_cache, functions, dry_run,
                    ))
                    .await?;
                    if ec == 0 {
                        return run_nodes(elif_body, aliases, rl, prev_dir, jobs, vars,
                                         smart_hints, shell_history, path_cache, functions, dry_run).await;
                    }
                }
                if let Some(eb) = else_body {
                    run_nodes(eb, aliases, rl, prev_dir, jobs, vars,
                              smart_hints, shell_history, path_cache, functions, dry_run).await
                } else {
                    Ok(0)
                }
            }
        }

        Node::While { condition, body } => {
            let mut last = 0i32;
            loop {
                let cond = Box::pin(exec_node(
                    condition, aliases, rl, prev_dir, jobs, vars,
                    smart_hints, shell_history, path_cache, functions, dry_run,
                ))
                .await?;
                if cond != 0 { break; }
                last = run_nodes(body, aliases, rl, prev_dir, jobs, vars,
                                 smart_hints, shell_history, path_cache, functions, dry_run).await?;
            }
            Ok(last)
        }

        Node::For { var, items, body } => {
            let mut last = 0i32;
            let expanded_items = expand_for_items(items, vars);
            for item in &expanded_items {
                vars.set(var, item);
                env::set_var(var, item);
                last = run_nodes(body, aliases, rl, prev_dir, jobs, vars,
                                 smart_hints, shell_history, path_cache, functions, dry_run).await?;
            }
            Ok(last)
        }

        Node::Case { word, arms } => {
            let word = vars.expand(word);
            for (patterns, body) in arms {
                for pat in patterns {
                    if glob_match(pat, &word) {
                        return run_nodes(body, aliases, rl, prev_dir, jobs, vars,
                                         smart_hints, shell_history, path_cache, functions, dry_run).await;
                    }
                }
            }
            Ok(0)
        }

        Node::FunctionDef { name, body } => {
            functions.define(name, body.clone());
            Ok(0)
        }
    }
}

async fn run_nodes(
    nodes: &[Node],
    aliases: &HashMap<String, String>,
    rl: &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir: &mut Option<PathBuf>,
    jobs: &mut JobTable,
    vars: &mut ShellVars,
    smart_hints: &mut SmartHints,
    shell_history: &mut ShellHistory,
    path_cache: &PathCache,
    functions: &mut FunctionTable,
    dry_run: bool,
) -> io::Result<i32> {
    let mut last = 0i32;
    for node in nodes {
        last = Box::pin(exec_node(
            node, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        ))
        .await?;
    }
    Ok(last)
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
    functions: &mut FunctionTable,
    dry_run: bool,
) -> io::Result<i32> {

    // 1. Variable expansion + arithmetic $((…))
    let expanded = vars.expand(input);
    let all_vars = vars.all();
    let expanded = expand_arithmetic(&expanded, &all_vars);
    let input    = expanded.as_str();

    // 2. source / .
    if let Some(path) = strip_source_prefix(input) {
        let path = expand_tilde(&path);
        if dry_run { println!("[dry-run] source {}", path); return Ok(0); }
        return run_source(
            &path, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        ).await;
    }

    // 3. Shell builtins
    if let Some(code) = handle_builtin(
        input, rl, prev_dir, jobs, shell_history, aliases, dry_run,
    ) {
        return Ok(code);
    }

    // 4. test / [ ]
    {
        let parts: Vec<String> = shlex::split(input).unwrap_or_default();
        if matches!(parts.first().map(|s| s.as_str()), Some("test") | Some("[")) {
            return Ok(builtin_test(&parts));
        }
    }

    // 5. User-defined functions
    {
        let parts: Vec<String> = shlex::split(input).unwrap_or_default();
        if let Some(fname) = parts.first() {
            if functions.contains(fname) {
                let body = functions.get(fname).cloned().unwrap_or_default();
                for (i, arg) in parts[1..].iter().enumerate() {
                    vars.set(&(i + 1).to_string(), arg);
                    env::set_var((i + 1).to_string(), arg);
                }
                return run_nodes(&body, aliases, rl, prev_dir, jobs, vars,
                                 smart_hints, shell_history, path_cache, functions, dry_run).await;
            }
        }
    }

    // 6. Inline env assignments
    let (inline_env, rest) = parse_inline_env(input);
    if rest.is_empty() {
        for (k, v) in &inline_env { vars.set(k, v); env::set_var(k, v); }
        return Ok(0);
    }

    // 7. Alias expansion
    let rest = expand_alias(&rest, aliases);

    // 8. Auto-sudo
    let rest = check_auto_sudo(&rest);

    // 9. Dangerous command guard
    if !dry_run && !confirm_dangerous(&rest) {
        println!("Command aborted.");
        return Ok(1);
    }

    // 10. Background flag
    let (background, rest) = strip_background_flag(&rest);
    let rest = rest.trim().to_string();

    // 11. .sh chmod  /  .hl rewrite
    maybe_chmod(&rest);
    let rest = maybe_hl_run(rest);

    // 12. Dry-run
    if dry_run {
        println!("[dry-run]{} {}", if background { " [bg]" } else { "" }, rest);
        return Ok(0);
    }

    // 13. Pipeline or simple
    let stages = split_pipeline(&rest);
    if stages.len() == 1 {
        run_simple(&rest, &inline_env, jobs, background).await
    } else {
        run_pipeline(&stages, &inline_env, jobs, background).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Simple command — native redirections via pre_exec + dup2
// ─────────────────────────────────────────────────────────────────────────────

async fn run_simple(
    cmd: &str,
    inline_env: &[(String, String)],
                    jobs: &mut JobTable,
                    background: bool,
) -> io::Result<i32> {

    let (clean_cmd, redirects) = parse_redirections(cmd);
    let heredoc_bodies: HashMap<String, String> = HashMap::new();

    let raw: Vec<String> = shlex::split(&clean_cmd).unwrap_or_default();
    if raw.is_empty() { return Ok(0); }
    let parts   = expand_globs(raw.into_iter().map(|a| expand_tilde(&a)).collect());
    let program = parts[0].clone();
    let argv: Vec<String> = parts[1..].to_vec();

    // Native builtins (no process spawn needed)
    if let Some(code) = dispatch_native(&program, &argv) {
        return Ok(code);
    }

    // Build std::process::Command (need pre_exec for redirections)
    let redirects_for_child: Vec<Redirect> = redirects;
    let heredocs_for_child: HashMap<String, String> = heredoc_bodies;

    let mut builder = std::process::Command::new(&program);
    builder.args(&argv);
    for (k, v) in inline_env { builder.env(k, v); }

    if !redirects_for_child.is_empty() {
        let r = redirects_for_child.clone();
        let h = heredocs_for_child.clone();
        unsafe {
            builder.pre_exec(move || {
                apply_redirections(&r, &h)
            });
        }
    }

    match builder.spawn() {
        Ok(mut child) => {
            if background {
                let pid = child.id();
                jobs.add(pid, &format!("{} {}", program, argv.join(" ")));
                Ok(0)
            } else {
                let status = child.wait()?;
                Ok(status.code().unwrap_or(1))
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("hsh: {}: command not found", program);
            Ok(127)
        }
        Err(e) => {
            eprintln!("hsh: {}: {}", program, e);
            Ok(1)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Native pipeline
// ─────────────────────────────────────────────────────────────────────────────

async fn run_pipeline(
    stages: &[String],
    inline_env: &[(String, String)],
                      jobs: &mut JobTable,
                      background: bool,
) -> io::Result<i32> {
    if stages.is_empty() { return Ok(0); }
    if stages.len() == 1 {
        return run_simple(&stages[0], inline_env, jobs, background).await;
    }

    let mut children: Vec<std::process::Child> = Vec::with_capacity(stages.len());
    let mut prev_stdout: Option<std::process::ChildStdout> = None;

    for (i, stage) in stages.iter().enumerate() {
        let is_last  = i == stages.len() - 1;
        let is_first = i == 0;

        let (clean_stage, redirects) = parse_redirections(stage);
        let heredoc_bodies: HashMap<String, String> = HashMap::new();

        let stdin_cfg: Stdio = prev_stdout.take()
        .map(Stdio::from)
        .unwrap_or_else(Stdio::inherit);
        let stdout_cfg: Stdio = if is_last { Stdio::inherit() } else { Stdio::piped() };

        let raw: Vec<String> = shlex::split(&clean_stage).unwrap_or_default();
        let parts = expand_globs(raw.into_iter().map(|a| expand_tilde(&a)).collect());
        if parts.is_empty() { continue; }

        let redirects_for_child: Vec<Redirect> = redirects;
        let heredocs_for_child: HashMap<String, String> = heredoc_bodies;

        let mut cmd = std::process::Command::new(&parts[0]);
        cmd.args(&parts[1..]).stdin(stdin_cfg).stdout(stdout_cfg);
        if is_first { for (k, v) in inline_env { cmd.env(k, v); } }

        if !redirects_for_child.is_empty() {
            let r = redirects_for_child.clone();
            let h = heredocs_for_child.clone();
            unsafe {
                cmd.pre_exec(move || {
                    apply_redirections(&r, &h)
                });
            }
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                eprintln!("hsh: {}: command not found", &parts[0]);
                return Ok(127);
            }
            Err(e) => { eprintln!("hsh: {}: {}", &parts[0], e); return Ok(1); }
        };

        if !is_last { prev_stdout = child.stdout.take(); }
        children.push(child);
    }

    if background {
        if let Some(first) = children.first() {
            jobs.add(first.id(), &stages.join(" | "));
        }
        return Ok(0);
    }

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
// Source
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
    functions: &mut FunctionTable,
    dry_run: bool,
) -> io::Result<i32> {
    let contents = read_to_string(file_path).map_err(|e| {
        eprintln!("hsh: source: {}: {}", file_path, e);
        e
    })?;
    let mut last_code = 0i32;
    for line in contents.lines() {
        let tl = line.trim();
        if tl.is_empty() || tl.starts_with('#') { continue; }
        last_code = Box::pin(run_line(
            line, aliases, rl, prev_dir, jobs, vars,
            smart_hints, shell_history, path_cache, functions, dry_run,
        ))
        .await?;
    }
    Ok(last_code)
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilities
// ─────────────────────────────────────────────────────────────────────────────

pub fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        if let Ok(home) = env::var("HOME") {
            return format!("{}{}", home, &s[1..]);
        }
    }
    s.to_string()
}

fn expand_globs(args: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for arg in args {
        if arg.contains('*') || arg.contains('?') || (arg.contains('{') && arg.contains('}')) {
            match glob::glob(&arg) {
                Ok(paths) => {
                    let exp: Vec<String> = paths
                    .filter_map(|p| p.ok())
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                    if exp.is_empty() { result.push(arg); } else { result.extend(exp); }
                }
                Err(_) => result.push(arg),
            }
        } else {
            result.push(arg);
        }
    }
    result
}

fn expand_for_items(items: &[String], vars: &ShellVars) -> Vec<String> {
    let mut result = Vec::new();
    for item in items {
        let expanded = vars.expand(item);
        if expanded.contains('*') || expanded.contains('?') {
            if let Ok(paths) = glob::glob(&expanded) {
                let v: Vec<String> = paths
                .filter_map(|p| p.ok())
                .map(|p| p.to_string_lossy().to_string())
                .collect();
                if !v.is_empty() { result.extend(v); continue; }
            }
        }
        result.push(expanded);
    }
    result
}

fn split_pipeline(input: &str) -> Vec<String> {
    let mut stages  = Vec::new();
    let mut current = String::new();
    let mut in_s    = false;
    let mut in_d    = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            '\'' if !in_d => { in_s = !in_s; current.push('\''); i += 1; }
            '"'  if !in_s => { in_d = !in_d; current.push('"');  i += 1; }
            '|'  if !in_s && !in_d => {
                if chars.get(i + 1) == Some(&'|') {
                    current.push_str("||"); i += 2;
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

fn split_compound(input: &str) -> Vec<(String, Option<String>)> {
    let mut result  = Vec::new();
    let mut current = String::new();
    let mut in_s    = false;
    let mut in_d    = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    macro_rules! flush {
        ($op:expr) => {{
            let s = current.trim().to_string();
            if !s.is_empty() { result.push((s, $op)); }
            current = String::new();
        }};
    }

    while i < chars.len() {
        match chars[i] {
            '\'' if !in_d => { in_s = !in_s; current.push('\''); i += 1; }
            '"'  if !in_s => { in_d = !in_d; current.push('"');  i += 1; }
            ';'  if !in_s && !in_d => { flush!(Some(";".into())); i += 1; }
            '&'  if !in_s && !in_d && chars.get(i+1) == Some(&'&') => {
                flush!(Some("&&".into())); i += 2;
            }
            '|'  if !in_s && !in_d && chars.get(i+1) == Some(&'|') => {
                flush!(Some("||".into())); i += 2;
            }
            c => { current.push(c); i += 1; }
        }
    }
    flush!(None);
    if result.is_empty() { vec![(input.trim().to_string(), None)] } else { result }
}

fn is_script_construct(input: &str) -> bool {
    let first = input.split_whitespace().next().unwrap_or("");
    matches!(first, "if" | "for" | "while" | "case")
    || input.contains("() {")
    || input.contains("(){")
    || (input.contains("()") && input.contains('{'))
}

fn strip_source_prefix(input: &str) -> Option<String> {
    input.strip_prefix("source ")
    .or_else(|| input.strip_prefix(". "))
    .map(|s| s.trim().to_string())
}

fn strip_background_flag(input: &str) -> (bool, String) {
    let t = input.trim_end();
    if t.ends_with('&') && !t.ends_with("&&") {
        (true, t[..t.len() - 1].trim().to_string())
    } else {
        (false, t.to_string())
    }
}

fn expand_alias(input: &str, aliases: &HashMap<String, String>) -> String {
    let parts: Vec<String> = shlex::split(input).unwrap_or_default();
    if let Some(first) = parts.first() {
        if let Some(val) = aliases.get(first.as_str()) {
            let rest = parts[1..].join(" ");
            return if rest.is_empty() { val.clone() } else { format!("{} {}", val, rest) };
        }
    }
    input.to_string()
}

fn check_auto_sudo(input: &str) -> String {
    if unsafe { libc::getuid() == 0 } { return input.to_string(); }
    let parts = shlex::split(input).unwrap_or_default();
    if parts.len() < 2 { return input.to_string(); }
    if !["vi", "vim", "nano", "emacs"].contains(&parts[0].as_str()) {
        return input.to_string();
    }
    let file = &parts[1];
    if !file.starts_with("/etc/") && !file.starts_with("/usr/")
        && !file.starts_with("/var/") && !file.starts_with("/boot/") {
            return input.to_string();
        }
        eprint!("\x1b[1;33m⚠  '{}' requires root. Use sudo? [y/n] \x1b[0m", file);
    io::stdout().flush().ok();
    let mut ans = String::new();
    io::stdin().read_line(&mut ans).ok();
    if ans.trim().eq_ignore_ascii_case("y") {
        format!("sudo {}", input)
    } else {
        input.to_string()
    }
}

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

fn maybe_hl_run(cmd: String) -> String {
    let first = cmd.split_whitespace().next().unwrap_or("").to_string();
    if first.ends_with(".hl") { format!("hl run {}", cmd) } else { cmd }
}

fn glob_match(pattern: &str, word: &str) -> bool {
    if pattern == "*" { return true; }
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == word;
    }
    match_glob(
        &pattern.chars().collect::<Vec<_>>(), 0,
               &word.chars().collect::<Vec<_>>(), 0,
    )
}

fn match_glob(pat: &[char], pi: usize, s: &[char], si: usize) -> bool {
    if pi == pat.len() { return si == s.len(); }
    if pat[pi] == '*' {
        // Skip consecutive stars
        let next_pi = {
            let mut np = pi + 1;
            while np < pat.len() && pat[np] == '*' { np += 1; }
            np
        };
        if next_pi == pat.len() { return true; }
        for skip in si..=s.len() {
            if match_glob(pat, next_pi, s, skip) { return true; }
        }
        return false;
    }
    if si >= s.len() { return false; }
    if pat[pi] == '?' || pat[pi] == s[si] {
        match_glob(pat, pi + 1, s, si + 1)
    } else {
        false
    }
}
