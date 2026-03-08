use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use chrono::Local;
use sysinfo::System;

use crate::git_info::GitInfo;
use crate::theme::Theme;

fn is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}

fn shorten_path(path: &PathBuf) -> String {
    if let Ok(home) = env::var("HOME") {
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix(&home) {
            return format!("~{}", rest);
        }
    }
    path.to_string_lossy().to_string()
}

fn format_duration(ms: u128) -> String {
    if ms >= 60_000 {
        format!("{}m {}s", ms / 60_000, (ms % 60_000) / 1000)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

pub fn build_prompt(
    _prompt_cfg: &HashMap<String, String>,
    last_exit_code: i32,
    last_duration_ms: Option<u128>,
    shell_depth: usize,
    system: &System,
    git_info: &GitInfo,
) -> String {
    let t   = Theme::load();
    let rst = "\x1b[0m";
    let dim = "\x1b[38;5;240m";

    let dir  = shorten_path(&env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    let time = Local::now().format("%H:%M:%S").to_string();

    // ── Git segment ───────────────────────────────────────────────────────────
    let git_seg = {
        let gs = git_info.format(" ", &t.git_color);
        if gs.is_empty() {
            String::new()
        } else {
            format!("  {}{}  ", t.sep, gs)
        }
    };

    // ── Duration ──────────────────────────────────────────────────────────────
    let dur_seg = last_duration_ms
    .map(|ms| format!("  {}{}{}  ", t.duration_color, format_duration(ms), rst))
    .unwrap_or_default();

    // ── Exit code ─────────────────────────────────────────────────────────────
    let exit_seg = if last_exit_code != 0 {
        format!("  {}✗ [{}]{}  ", t.error_color, last_exit_code, rst)
    } else {
        String::new()
    };

    // ── Root ──────────────────────────────────────────────────────────────────
    let root_seg = if is_root() {
        format!("  {}⚡{}  ", t.error_color, rst)
    } else {
        String::new()
    };

    // ── Depth ─────────────────────────────────────────────────────────────────
    let depth_seg = if shell_depth > 0 {
        format!("  {}[{}]{}  ", dim, shell_depth + 1, rst)
    } else {
        String::new()
    };

    // ── Sysinfo — tylko gdy CPU > 70% lub RAM > 80% ───────────────────────────
    let used_gb  = system.used_memory() as f64 / 1_073_741_824.0;
    let total_gb = system.total_memory() as f64 / 1_073_741_824.0;
    let cpu      = system.cpus().first().map(|c| c.cpu_usage()).unwrap_or(0.0);
    let mem_pct  = if total_gb > 0.0 { used_gb / total_gb * 100.0 } else { 0.0 };

    let sys_seg = if cpu > 70.0 || mem_pct > 80.0 {
        format!(
            "  {}{}cpu:{:.0}% mem:{:.0}%{}  ",
            t.sep, t.duration_color, cpu, mem_pct, rst
        )
    } else {
        String::new()
    };

    // ── Prompt char ───────────────────────────────────────────────────────────
    let pc_color = if last_exit_code == 0 { &t.prompt_ok } else { &t.prompt_err };
    let pc       = format!("{}{}{} ", pc_color, t.prompt_char, rst);

    // ── Assemble — wszystkie segmenty już zawierają swoje kolory ─────────────
    //
    //  HH:MM:SS  ~/dir  ⎇ branch  [✗ N]  took 1.2s  ❯
    //
    let mut prompt = String::new();
    prompt.push_str(&t.time_color);
    prompt.push_str(&time);
    prompt.push_str(rst);
    prompt.push_str("  ");
    prompt.push_str(&t.dir_color);
    prompt.push_str(&dir);
    prompt.push_str(rst);
    prompt.push_str(&git_seg);
    prompt.push_str(&sys_seg);
    prompt.push_str(&depth_seg);
    prompt.push_str(&root_seg);
    prompt.push_str(&exit_seg);
    prompt.push_str(&dur_seg);
    prompt.push_str(&pc);
    prompt
}
