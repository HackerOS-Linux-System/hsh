mod builtins;
mod config;
mod execute;
mod helper;
mod history;
mod jobs;
mod prompt;
mod security;
mod vars;

use std::env;
use std::path::PathBuf;

use rustyline::error::ReadlineError;
use rustyline::{Cmd, CompletionType, Config, EditMode, Editor, KeyEvent};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio::process::Command as TokioCommand;

use config::load_shell_config;
use execute::execute_command;
use helper::ShellHelper;
use history::ShellHistory;
use jobs::JobTable;

#[tokio::main]
async fn main() -> rustyline::Result<()> {
    let args: Vec<String> = env::args().collect();
    let dry_run = args.contains(&"--dry-run".to_string());

    // hsh -c "command"
    if let Some(pos) = args.iter().position(|a| a == "-c") {
        if let Some(cmd) = args.get(pos + 1) {
            let hk_config = load_shell_config();
            let aliases = config::get_aliases(&hk_config);
            let mut prev_dir: Option<PathBuf> = None;
            let mut jobs = JobTable::new();
            let mut vars = vars::ShellVars::new();
            let rl_cfg = Config::builder().build();
            let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> =
            Editor::with_config(rl_cfg)?;
            rl.set_helper(Some(ShellHelper::new()));
            let code =
            execute_command(cmd, &aliases, &mut rl, &mut prev_dir, &mut jobs, &mut vars, dry_run)
            .await
            .unwrap_or(1);
            std::process::exit(code);
        }
        eprintln!("hsh: -c requires an argument");
        std::process::exit(1);
    }

    // Run MOTD if exists
    if let Ok(mut child) = TokioCommand::new("sh")
        .arg("-c")
        .arg("/usr/share/HackerOS/Archived/MOTD/hackeros-motd")
        .spawn()
        {
            let _ = child.wait().await;
        }

        let rl_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(CompletionType::List)
        .edit_mode(EditMode::Emacs)
        .build();

        let mut rl: Editor<ShellHelper, rustyline::history::FileHistory> =
        Editor::with_config(rl_config)?;
        rl.set_helper(Some(ShellHelper::new()));
        rl.bind_sequence(KeyEvent::ctrl('l'), Cmd::ClearScreen);

        let home = env::var("HOME").unwrap_or_default();
        let history_path = format!("{}/.hsh-history", home);
        if rl.load_history(&history_path).is_err() {
            println!("No previous history.");
        }

        let hk_config = load_shell_config();
        let aliases = config::get_aliases(&hk_config);
        let prompt_cfg = config::get_prompt_config(&hk_config);

        let mut prev_dir: Option<PathBuf> = None;
        let mut last_exit_code: i32 = 0;
        let mut last_duration_ms: Option<u128> = None;
        let mut jobs = JobTable::new();
        let mut vars = vars::ShellVars::new();

        let mut shell_history = ShellHistory::load(&format!("{}/.hsh-history-ts", home));

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

        loop {
            system.refresh_memory();
            system.refresh_cpu_usage();

            let prompt = prompt::build_prompt(
                &prompt_cfg,
                last_exit_code,
                last_duration_ms,
                shell_depth,
                &system,
            );

            rl.helper_mut().expect("No helper").colored_prompt = prompt.clone();

            let readline = rl.readline(&prompt);
            match readline {
                Ok(line) => {
                    let trimmed_line = line.trim();
                    if !trimmed_line.is_empty() {
                        rl.add_history_entry(&line);
                        shell_history.add(trimmed_line);
                    }
                    let start = std::time::Instant::now();
                    last_exit_code =
                    execute_command(&line, &aliases, &mut rl, &mut prev_dir, &mut jobs, &mut vars, dry_run)
                    .await
                    .unwrap_or(1);
                    let elapsed = start.elapsed().as_millis();
                    last_duration_ms = if elapsed >= 2000 { Some(elapsed) } else { None };
                }
                Err(ReadlineError::Interrupted) => println!("^C"),
                Err(ReadlineError::Eof) => {
                    println!("exit");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }

        shell_history.save(&format!("{}/.hsh-history-ts", home));
        rl.save_history(&history_path)
}
