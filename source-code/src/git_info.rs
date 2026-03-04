use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

#[derive(Clone, Default)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub dirty: bool,       // uncommitted changes
    pub ahead: u32,        // commits ahead of remote
    pub behind: u32,       // commits behind remote
}

impl GitInfo {
    pub fn format(&self, git_symbol: &str, git_color: &str) -> String {
        let Some(ref branch) = self.branch else { return String::new() };

        let dirty_marker = if self.dirty { " \x1b[31m✗\x1b[0m" } else { "" };
        let sync = match (self.ahead, self.behind) {
            (0, 0) => String::new(),
            (a, 0) => format!(" \x1b[32m↑{}\x1b[0m", a),
            (0, b) => format!(" \x1b[31m↓{}\x1b[0m", b),
            (a, b) => format!(" \x1b[33m↕{}/{}\x1b[0m", a, b),
        };

        format!(
            "{}({} {}{}{})\x1b[0m",
                git_color, git_symbol, branch, dirty_marker, sync
        )
    }
}

/// Spawns a background task to fetch git info.
/// Returns a watch receiver that gets updated when ready.
/// Call this BEFORE rendering the prompt — result may be "stale" for one frame.
pub fn spawn_git_watcher() -> watch::Receiver<GitInfo> {
    let (tx, rx) = watch::channel(GitInfo::default());

    tokio::spawn(async move {
        loop {
            let info = fetch_git_info().await;
            let _ = tx.send(info);
            // Re-check every 2 seconds
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    rx
}

async fn fetch_git_info() -> GitInfo {
    // Branch
    let branch_out = tokio::process::Command::new("git")
    .args(["rev-parse", "--abbrev-ref", "HEAD"])
    .stderr(std::process::Stdio::null())
    .output()
    .await;

    let branch = match branch_out {
        Ok(out) if out.status.success() => {
            let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if b == "HEAD" {
                // Detached — show short hash
                short_hash().await.map(|h| format!(":{}", h)).or(Some(b))
            } else {
                Some(b)
            }
        }
        _ => return GitInfo::default(),
    };

    // Dirty check (fast: only index + worktree)
    let dirty = tokio::process::Command::new("git")
    .args(["status", "--porcelain", "--untracked-files=no"])
    .stderr(std::process::Stdio::null())
    .output()
    .await
    .map(|o| !o.stdout.is_empty())
    .unwrap_or(false);

    // Ahead/behind
    let (ahead, behind) = ahead_behind().await;

    GitInfo { branch, dirty, ahead, behind }
}

async fn short_hash() -> Option<String> {
    let out = tokio::process::Command::new("git")
    .args(["rev-parse", "--short", "HEAD"])
    .stderr(std::process::Stdio::null())
    .output()
    .await
    .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

async fn ahead_behind() -> (u32, u32) {
    let out = tokio::process::Command::new("git")
    .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"])
    .stderr(std::process::Stdio::null())
    .output()
    .await;

    if let Ok(o) = out {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = s.trim().split_whitespace().collect();
            if parts.len() == 2 {
                let behind = parts[0].parse().unwrap_or(0);
                let ahead = parts[1].parse().unwrap_or(0);
                return (ahead, behind);
            }
        }
    }
    (0, 0)
}
