use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use ansi_str::AnsiStr;
use chrono::Local;
use sysinfo::System;
use terminal_size::terminal_size;

use crate::config::get_segment_order;
use crate::git_info::GitInfo;

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
    git_info: &GitInfo,
) -> String {
    // ── colours / symbols from config (with defaults) ─────────────────────────
    let time_color   = prompt_cfg.get("time_color")  .cloned().unwrap_or("\x1b[1;36m".into());
    let dir_symbol   = prompt_cfg.get("dir_symbol")  .cloned().unwrap_or("\u{1F4C1}".into());
    let dir_color    = prompt_cfg.get("dir_color")   .cloned().unwrap_or("\x1b[1;34m".into());
    let git_symbol   = prompt_cfg.get("git_symbol")  .cloned().unwrap_or("\u{E0A0}".into());
    let git_color    = prompt_cfg.get("git_color")   .cloned().unwrap_or("\x1b[1;33m".into());
    let prompt_color = prompt_cfg.get("prompt_color").cloned().unwrap_or("\x1b[1;32m".into());
    let error_sym    = prompt_cfg.get("error_symbol").cloned().unwrap_or("\u{2718}".into());
    let root_sym     = prompt_cfg.get("root_symbol") .cloned().unwrap_or("\u{26A1}".into());

    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let time        = Local::now().format("%H:%M").to_string();
    let segments    = get_segment_order(prompt_cfg);

    // ── left-side segments ────────────────────────────────────────────────────
    let mut left_parts: Vec<String> = Vec::new();

    for seg in &segments {
        match seg.as_str() {
            "time" => {
                left_parts.push(format!("{}[{}]\x1b[0m", time_color, time));
            }
            "dir" => {
                left_parts.push(format!(
                    "{}{} {}\x1b[0m",
                    dir_color, dir_symbol, current_dir.display()
                ));
            }
            "git" => {
                let git_str = git_info.format(&git_symbol, &git_color);
                if !git_str.is_empty() {
                    left_parts.push(git_str);
                }
            }
            "mem_cpu" => { /* handled in right prompt */ }
            _ => {}
        }
    }

    // ── right-side: mem / cpu ─────────────────────────────────────────────────
    let used_mem_gb = system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let cpu_usage   = system.cpus().first().map(|c| c.cpu_usage()).unwrap_or(0.0);
    let rprompt     = format!("mem:{:.1}GB cpu:{:.0}%", used_mem_gb, cpu_usage);

    // ── subshell depth indicator  (())  ((()))  etc. ──────────────────────────
    let depth_indicator = if shell_depth > 0 {
        let n = shell_depth + 1;
        format!(
            " \x1b[90m{}{}\x1b[0m",
            "(".repeat(n),
                ")".repeat(n)
        )
    } else {
        String::new()
    };

    // ── first line: left + right aligned ──────────────────────────────────────
    let left_first = format!("╭─ {}{}", left_parts.join(" "), depth_indicator);
    let left_len   = left_first.ansi_strip().len();
    let rp_len     = rprompt.len();
    let term_w     = terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(80);
    let spaces     = term_w.saturating_sub(left_len + rp_len);

    let first_line = format!("{}{}{}", left_first, " ".repeat(spaces), rprompt);

    // ── optional duration line ────────────────────────────────────────────────
    let duration_line = last_duration_ms
    .map(|ms| format!("  \x1b[90m{}\x1b[0m\n", format_duration(ms)))
    .unwrap_or_default();

    // ── second line: prompt character ─────────────────────────────────────────
    let error_part = if last_exit_code != 0 {
        format!("\x1b[31m{}\x1b[0m ", error_sym)
    } else {
        String::new()
    };
    let root_part = if is_root() {
        format!("{} ", root_sym)
    } else {
        String::new()
    };

    let second_line = format!(
        "{}╰─ {}{}hsh❯\x1b[0m ",
        prompt_color, error_part, root_part
    );

    format!("{}\n{}{}", first_line, duration_line, second_line)
}
