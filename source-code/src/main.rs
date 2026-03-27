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

use config::load_shell_config;
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
        println!("hsh 0.3.5 — HackerOS Shell");
        println!("License: BSD-3-Clause");
        std::process::exit(0);
    }

    // ── hsh -c "command" ────────────────────────────────────────────────────
    if let Some(pos) = args.iter().position(|a| a == "-c") {
        match args.get(pos + 1) {
            Some(cmd) => {
                let home      = env::var("HOME").unwrap_or_default();
                let hk_config = load_shell_config();
                let aliases   = config::get_aliases(&hk_config);
                let mut prev_dir  = None::<PathBuf>;
                let mut jobs      = JobTable::new();
                let mut vars      = ShellVars::new();
                let mut hints     = SmartHints::load(&format!("{}/.hsh-hints.json",        home));
                let mut history   = ShellHistory::load(&format!("{}/.hsh-history-ts.json", home));
                let path_cache    = PathCache::new(&format!("{}/.hsh-path-cache.json",     home));
                let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> =
                Editor::with_config(Config::builder().build())?;
                rl.set_helper(Some(ShellHelper::new(Theme::load())));
                let code = execute_command(
                    cmd, &aliases, &mut rl, &mut prev_dir,
                    &mut jobs, &mut vars, &mut hints, &mut history,
                    &path_cache, dry_run,
                ).await.unwrap_or(1);
                hints.save(&format!("{}/.hsh-hints.json",        home));
                history.save(&format!("{}/.hsh-history-ts.json", home));
                std::process::exit(code);
            }
            None => { eprintln!("hsh: -c requires an argument"); std::process::exit(1); }
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

        // ── Paths ────────────────────────────────────────────────────────────────
        let home            = env::var("HOME").unwrap_or_default();
    let history_rl_path = format!("{}/.hsh-history",          home);
    let history_ts_path = format!("{}/.hsh-history-ts.json",  home);
    let hints_path      = format!("{}/.hsh-hints.json",        home);
    let path_cache_path = format!("{}/.hsh-path-cache.json",   home);

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
    // włączenie Ctrl+R (domyślnie działa, ale dla pewności)
    rl.bind_sequence(KeyEvent::ctrl('r'), Cmd::HistorySearchForward);
    let _ = rl.load_history(&history_rl_path);

    // ── Config ───────────────────────────────────────────────────────────────
    let hk_config  = load_shell_config();
    let aliases    = config::get_aliases(&hk_config);
    let prompt_cfg = config::get_prompt_config(&hk_config);

    // ── State ────────────────────────────────────────────────────────────────
    let mut prev_dir         = None::<PathBuf>;
    let mut last_exit_code   = 0i32;
    let mut last_duration_ms = None::<u128>;
    let mut jobs             = JobTable::new();
    let mut vars             = ShellVars::new();
    let mut shell_history    = ShellHistory::load(&history_ts_path);
    let mut smart_hints      = SmartHints::load(&hints_path);

    // ustaw PWD na start
    vars.set_pwd();

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

        // ── Helper state przed readline ───────────────────────────────────
        {
            let h = rl.helper_mut().expect("no helper");
            h.colored_prompt = prompt.clone();

            // Synchronizuj snapshot z SmartHints (podpowiedzi inline)
            h.sync_hints(&smart_hints);

            // Next-command hint na pustej linii
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

                // ── Specjalne komendy meta ────────────────────────────────
                if trimmed == "hsh-settings" {
                    run_settings();
                    let new_theme = Theme::load();
                    rl.helper_mut().expect("no helper").theme = new_theme;
                    continue;
                }

                if trimmed == "hsh-docs" || trimmed.starts_with("hsh-docs ") {
                    let rest = trimmed.strip_prefix("hsh-docs").unwrap_or("").trim();
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    docs::run_docs(&parts);
                    continue;
                }

                rl.add_history_entry(&line);

                let prev_cmd = shell_history.last_command().unwrap_or_default();
                shell_history.add(trimmed);
                smart_hints.record(&prev_cmd, trimmed);

                // ── Sync hints po komendzie ───────────────────────────────
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

    shell_history.save(&history_ts_path);
    smart_hints.save(&hints_path);
    rl.save_history(&history_rl_path)?;
    Ok(())
}
