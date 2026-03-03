use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use ansi_str::AnsiStr;
use chrono::Local;
use sysinfo::System;
use terminal_size::terminal_size;

use crate::config::get_segment_order;

pub fn get_git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
    .args(["rev-parse", "--abbrev-ref", "HEAD"])
    .stderr(std::process::Stdio::null())
    .output()
    .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

fn format_duration(ms: u128) -> String {
    if ms >= 60_000 {
        format!("took {}m{}s", ms / 60_000, (ms % 60_000) / 1000)
    } else {
        format!("took {:.1}s", ms as f64 / 1000.0)
    }
}

pub fn build_prompt(
    prompt_cfg: &HashMap<String, String>,
    last_exit_code: i32,
    last_duration_ms: Option<u128>,
    shell_depth: usize,
    system: &System,
) -> String {
    let time_color = prompt_cfg
    .get("time_color")
    .cloned()
    .unwrap_or("\x1b[1;36m".to_string());
    let dir_symbol = prompt_cfg
    .get("dir_symbol")
    .cloned()
    .unwrap_or("\u{1F4C1}".to_string());
    let dir_color = prompt_cfg
    .get("dir_color")
    .cloned()
    .unwrap_or("\x1b[1;34m".to_string());
    let git_symbol = prompt_cfg
    .get("git_symbol")
    .cloned()
    .unwrap_or("\u{E0A0}".to_string());
    let git_color = prompt_cfg
    .get("git_color")
    .cloned()
    .unwrap_or("\x1b[1;33m".to_string());
    let prompt_color = prompt_cfg
    .get("prompt_color")
    .cloned()
    .unwrap_or("\x1b[1;32m".to_string());
    let error_symbol_str = prompt_cfg
    .get("error_symbol")
    .cloned()
    .unwrap_or("\u{2718}".to_string());
    let root_symbol_str = prompt_cfg
    .get("root_symbol")
    .cloned()
    .unwrap_or("\u{26A1}".to_string());

    let current_dir = env::current_dir().unwrap_or(PathBuf::from("/"));
    let git_branch = get_git_branch();
    let time = Local::now().format("%H:%M").to_string();

    let segments = get_segment_order(prompt_cfg);

    let mut left_parts: Vec<String> = Vec::new();

    for seg in &segments {
        match seg.as_str() {
            "time" => {
                left_parts.push(format!("{}[{}]\x1b[0m", time_color, time));
            }
            "dir" => {
                left_parts.push(format!(
                    "{}{} {}\x1b[0m",
                    dir_color,
                    dir_symbol,
                    current_dir.display()
                ));
            }
            "git" => {
                if let Some(ref branch) = git_branch {
                    left_parts.push(format!(
                        "{}({} {})\x1b[0m",
                                            git_color, git_symbol, branch
                    ));
                }
            }
            "mem_cpu" => {
                // handled in right prompt
            }
            _ => {}
        }
    }

    let used_mem_gb = system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let cpu_usage = system
    .cpus()
    .first()
    .map(|c| c.cpu_usage())
    .unwrap_or(0.0);
    let rprompt = format!("mem:{:.1}GB cpu:{:.0}%", used_mem_gb, cpu_usage);

    // Depth indicator: shows (()) for subshells
    let depth_indicator = if shell_depth > 0 {
        format!(" \x1b[90m{}\x1b[0m", "(".repeat(shell_depth + 1) + &")".repeat(shell_depth + 1))
    } else {
        String::new()
    };

    let left_first_line = format!("╭─ {}{}", left_parts.join(" "), depth_indicator);

    let left_len = left_first_line.ansi_strip().len();
    let rprompt_len = rprompt.len();
    let term_width = terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(80);
    let spaces = term_width.saturating_sub(left_len + rprompt_len);

    let first_line = format!("{}{}{}", left_first_line, " ".repeat(spaces), rprompt);

    // Duration line
    let duration_part = if let Some(ms) = last_duration_ms {
        format!(" \x1b[90m{}\x1b[0m", format_duration(ms))
    } else {
        String::new()
    };

    let error_symbol = if last_exit_code != 0 {
        format!("\x1b[31m{}\x1b[0m ", error_symbol_str)
    } else {
        String::new()
    };
    let root_symbol = if is_root() {
        format!("{} ", root_symbol_str)
    } else {
        String::new()
    };

    let second_line = format!(
        "{}╰─ {}{}hsh❯\x1b[0m ",
        prompt_color, error_symbol, root_symbol
    );

    if duration_part.is_empty() {
        format!("{}\n{}", first_line, second_line)
    } else {
        format!("{}\n{}{}\n{}", first_line, prompt_color, duration_part, second_line)
    }
}
