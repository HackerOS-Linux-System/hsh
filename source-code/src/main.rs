mod arithmetic;
mod builtins;
mod builtins_native;
mod config;
mod docs;
mod execute;
mod git_info;
mod helper;
mod history;
mod jobs;
mod path_cache;
mod prompt;
mod redirect;
mod script;
mod security;
mod settings;
mod smarthints;
mod theme;
mod vars;

use std::env;
use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::{Cmd, CompletionType, Config, EditMode, Editor, KeyEvent};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio::process::Command as TokioCommand;

use config::{load_shell_config, get_history_path, get_env_vars, get_shell_options};
use execute::execute_command;
use git_info::spawn_git_watcher;
use helper::ShellHelper;
use history::ShellHistory;
use jobs::JobTable;
use path_cache::PathCache;
use settings::run_settings;
use smarthints::SmartHints;
use theme::Theme;
use vars::ShellVars;

#[tokio::main]
async fn main() -> rustyline::Result<()> {
    let args: Vec<String> = env::args().collect();
    let dry_run = args.contains(&"--dry-run".to_string());

    // ── hsh --version ────────────────────────────────────────────────────────
    if args.contains(&"--version".to_string()) || args.contains(&"-V".to_string()) {
        println!("hsh 0.4.0 — HackerOS Shell");
        println!("License: BSD-3-Clause");
        std::process::exit(0);
    }

    // ── hsh --check script.sh ─────────────────────────────────────────────────
    if let Some(pos) = args.iter().position(|a| a == "--check") {
        if let Some(path) = args.get(pos + 1) {
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    let checks = script::validate_script(&content);
                    if checks.is_empty() {
                        println!("\x1b[38;5;114m✓\x1b[0m {} — brak błędów składni", path);
                        std::process::exit(0);
                    } else {
                        script::print_syntax_errors(path, &checks);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("hsh: --check: {}: {}", path, e);
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!("hsh: --check wymaga ścieżki do pliku");
            std::process::exit(1);
        }
    }

    // ── Wczytaj konfigurację (generuje .hshrc jeśli brak) ───────────────────
    let hk_config  = load_shell_config();
    let aliases    = config::get_aliases(&hk_config);
    let prompt_cfg = config::get_prompt_config(&hk_config);

    // Zastosuj zmienne środowiskowe z [env]
    let env_vars = get_env_vars(&hk_config);
    for (k, v) in &env_vars {
        if env::var(k).is_err() { // Nie nadpisuj istniejących
            env::set_var(k, v);
        }
    }

    // Ścieżka historii z konfiguracji (fallback: ~/.hsh-history)
    let home = env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let history_ts_path = get_history_path(&hk_config);
    // rustyline historia (dla Ctrl+R) — osobny plik .hsh-history-rl
    let history_rl_path = format!("{}/.hsh-history-rl", home);
    let hints_path      = format!("{}/.hsh-hints.json",        home);
    let path_cache_path = format!("{}/.hsh-path-cache.json",   home);

    // ── hsh -c "command" ────────────────────────────────────────────────────
    if let Some(pos) = args.iter().position(|a| a == "-c") {
        match args.get(pos + 1) {
            Some(cmd) => {
                let mut prev_dir  = None::<PathBuf>;
                let mut jobs      = JobTable::new();
                let mut vars      = ShellVars::new();

                // Zastosuj opcje powłoki z konfiguracji
                apply_shell_options(&mut vars, &hk_config);

                let mut hints   = SmartHints::load(&hints_path);
                let mut history = ShellHistory::load(&history_ts_path);
                let path_cache  = PathCache::new(&path_cache_path);
                let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> =
                    Editor::with_config(Config::builder().build())?;
                rl.set_helper(Some(ShellHelper::new(Theme::load())));
                let code = execute_command(
                    cmd, &aliases, &mut rl, &mut prev_dir,
                    &mut jobs, &mut vars, &mut hints, &mut history,
                    &path_cache, dry_run,
                ).await.unwrap_or(1);
                hints.save(&hints_path);
                history.save(&history_ts_path);
                std::process::exit(code);
            }
            None => { eprintln!("hsh: -c requires an argument"); std::process::exit(1); }
        }
    }

    // ── hsh script.sh [args...] ──────────────────────────────────────────────
    // Jeśli pierwszy argument jest plikiem .sh — wykonaj skrypt
    if args.len() >= 2 && !args[1].starts_with('-') {
        let script_path = &args[1];
        if script_path.ends_with(".sh") || script_path.ends_with(".hsh") {
            return run_script_file(
                script_path,
                &args[2..],
                &aliases,
                &hints_path,
                &history_ts_path,
                &path_cache_path,
                dry_run,
                &hk_config,
            ).await;
        }
    }

    // ── MOTD ─────────────────────────────────────────────────────────────────
    if let Ok(mut child) = TokioCommand::new("sh")
        .arg("-c")
        .arg("/usr/share/HackerOS/Archived/MOTD/hackeros-motd")
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        let _ = child.wait().await;
    }

    // ── PathCache ────────────────────────────────────────────────────────────
    let mut path_cache = PathCache::new(&path_cache_path);

    // ── Rustyline ────────────────────────────────────────────────────────────
    let rl_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();

    let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> =
        Editor::with_config(rl_config)?;
    rl.set_helper(Some(ShellHelper::new(Theme::load())));
    rl.bind_sequence(KeyEvent::ctrl('l'), Cmd::ClearScreen);
    rl.bind_sequence(KeyEvent::ctrl('r'), Cmd::HistorySearchForward);
    let _ = rl.load_history(&history_rl_path);

    // ── State ────────────────────────────────────────────────────────────────
    let mut prev_dir         = None::<PathBuf>;
    let mut last_exit_code   = 0i32;
    let mut last_duration_ms = None::<u128>;
    let mut jobs             = JobTable::new();
    let mut vars             = ShellVars::new();

    // Zastosuj opcje powłoki z konfiguracji
    apply_shell_options(&mut vars, &hk_config);

    // Wczytaj historię z poprawnej ścieżki
    let mut shell_history = ShellHistory::load(&history_ts_path);
    let mut smart_hints   = SmartHints::load(&hints_path);

    vars.set_pwd();

    // Załaduj historię rustyline z wpisów hsh (synchronizacja)
    // To zapewnia działanie Ctrl+R z pełną historią
    for entry in shell_history.entries.iter().rev().take(500).rev() {
        let _ = rl.add_history_entry(&entry.command);
    }

    let git_rx = spawn_git_watcher();

    let mut system = System::new_with_specifics(
        RefreshKind::new()
            .with_memory(MemoryRefreshKind::everything())
            .with_cpu(CpuRefreshKind::everything()),
    );

    let shell_depth: usize = env::var("HSH_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    env::set_var("HSH_DEPTH", (shell_depth + 1).to_string());

    // ════════════════════════════════════════════════════════════════════════
    // REPL
    // ════════════════════════════════════════════════════════════════════════
    loop {
        system.refresh_memory();
        system.refresh_cpu_usage();
        path_cache.refresh_if_stale();

        let git_info = git_rx.borrow().clone();

        let prompt = prompt::build_prompt(
            &prompt_cfg,
            last_exit_code,
            last_duration_ms,
            shell_depth,
            &system,
            &git_info,
        );

        // ── Helper state ─────────────────────────────────────────────────────
        {
            let h = rl.helper_mut().expect("no helper");
            h.colored_prompt = prompt.clone();
            h.sync_hints(&smart_hints);
            h.next_hint = shell_history
                .last_command()
                .and_then(|last| {
                    smart_hints
                        .suggest_next(&last)
                        .map(|s| format!("\x1b[38;5;236m{}\x1b[0m", s))
                });
        }

        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }

                // ── Specjalne komendy meta ───────────────────────────────────
                if trimmed == "hsh-settings" {
                    run_settings();
                    let new_theme = Theme::load();
                    rl.helper_mut().expect("no helper").theme = new_theme;
                    continue;
                }

                if trimmed == "hsh-docs" || trimmed.starts_with("hsh-docs ") {
                    let rest  = trimmed.strip_prefix("hsh-docs").unwrap_or("").trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    docs::run_docs(&parts);
                    continue;
                }

                // Dodaj do historii rustyline (dla Ctrl+R)
                rl.add_history_entry(&line);

                let prev_cmd = shell_history.last_command().unwrap_or_default();
                shell_history.add(trimmed);
                smart_hints.record(&prev_cmd, trimmed);

                {
                    let h = rl.helper_mut().expect("no helper");
                    h.sync_hints(&smart_hints);
                }

                let t0 = std::time::Instant::now();

                last_exit_code = execute_command(
                    &line, &aliases, &mut rl, &mut prev_dir,
                    &mut jobs, &mut vars, &mut smart_hints,
                    &mut shell_history, &path_cache, dry_run,
                )
                .await
                .unwrap_or(1);

                vars.last_exit = last_exit_code;

                last_duration_ms = {
                    let ms = t0.elapsed().as_millis();
                    if ms >= 2000 { Some(ms) } else { None }
                };

                if last_exit_code == 127 {
                    let first_word = trimmed.split_whitespace().next().unwrap_or("");
                    if let Some(suggestion) =
                        smart_hints.spellcheck(first_word, &path_cache.commands)
                    {
                        let corrected = trimmed.replacen(first_word, suggestion, 1);
                        eprintln!(
                            "  \x1b[38;5;220m❓ Czy chodziło Ci o: \x1b[1m{}\x1b[0m\x1b[38;5;220m?\x1b[0m",
                            corrected
                        );
                    }
                }

                jobs.check_finished();
            }

            Err(ReadlineError::Interrupted) => {
                vars.last_exit   = 130;
                last_exit_code   = 130;
                last_duration_ms = None;
            }

            Err(ReadlineError::Eof) => {
                eprintln!("\x1b[38;5;244mexit\x1b[0m");
                break;
            }

            Err(err) => {
                eprintln!("hsh: readline error: {:?}", err);
                break;
            }
        }
    }

    // ── Zapis przy wyjściu ───────────────────────────────────────────────────
    shell_history.save(&history_ts_path);
    smart_hints.save(&hints_path);
    rl.save_history(&history_rl_path)?;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Uruchomienie skryptu .sh jako pliku
// ─────────────────────────────────────────────────────────────────────────────

async fn run_script_file(
    script_path:    &str,
    script_args:    &[String],
    aliases:        &std::collections::HashMap<String, String>,
    hints_path:     &str,
    history_path:   &str,
    path_cache_path: &str,
    dry_run:        bool,
    hk_config:      &hk_parser::HkConfig,
) -> rustyline::Result<()> {
    let mut prev_dir  = None::<PathBuf>;
    let mut jobs      = JobTable::new();
    let mut vars      = ShellVars::new();

    apply_shell_options(&mut vars, hk_config);

    // Argumenty pozycyjne skryptu ($1, $2, ...)
    vars.positional = script_args.to_vec();
    for (i, arg) in script_args.iter().enumerate() {
        vars.set(&(i + 1).to_string(), arg);
        env::set_var((i + 1).to_string(), arg);
    }
    vars.set("0", script_path);

    let mut hints   = SmartHints::load(hints_path);
    let mut history = ShellHistory::load(history_path);
    let path_cache  = PathCache::new(path_cache_path);

    let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> =
        Editor::with_config(Config::builder().build())?;
    rl.set_helper(Some(ShellHelper::new(Theme::load())));

    let code = execute_command(
        &format!("source {}", script_path),
        aliases,
        &mut rl,
        &mut prev_dir,
        &mut jobs,
        &mut vars,
        &mut hints,
        &mut history,
        &path_cache,
        dry_run,
    )
    .await
    .unwrap_or(1);

    std::process::exit(code);
}

// ─────────────────────────────────────────────────────────────────────────────
// Pomocnicze: zastosuj opcje z [shell] do ShellVars
// ─────────────────────────────────────────────────────────────────────────────

fn apply_shell_options(vars: &mut ShellVars, config: &hk_parser::HkConfig) {
    let opts = get_shell_options(config);
    if opts.get("errexit").map(|v| v == "true").unwrap_or(false) {
        vars.set_option("e", true);
    }
    if opts.get("xtrace").map(|v| v == "true").unwrap_or(false) {
        vars.set_option("x", true);
    }
    if opts.get("nounset").map(|v| v == "true").unwrap_or(false) {
        vars.set_option("u", true);
    }
}
