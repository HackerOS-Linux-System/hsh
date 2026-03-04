use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

const CACHE_TTL_SECS: u64 = 300; // 5 minutes

#[derive(Serialize, Deserialize)]
struct PathCacheData {
    commands: Vec<String>,
    built_at: u64,
    path_hash: u64,
}

pub struct PathCache {
    pub commands: Vec<String>,
    cache_file: String,
    path_hash: u64,
}

impl PathCache {
    pub fn new(cache_file: &str) -> Self {
        let path_hash = hash_path_var();
        let commands = Self::load_or_build(cache_file, path_hash);
        PathCache {
            commands,
            cache_file: cache_file.to_string(),
            path_hash,
        }
    }

    fn load_or_build(cache_file: &str, path_hash: u64) -> Vec<String> {
        // Try loading from disk
        if let Ok(data) = fs::read_to_string(cache_file) {
            if let Ok(cached) = serde_json::from_str::<PathCacheData>(&data) {
                let now = now_secs();
                let age = now.saturating_sub(cached.built_at);
                if age < CACHE_TTL_SECS && cached.path_hash == path_hash {
                    return cached.commands;
                }
            }
        }
        // Rebuild
        let commands = build_commands();
        let data = PathCacheData {
            commands: commands.clone(),
            built_at: now_secs(),
            path_hash,
        };
        if let Ok(json) = serde_json::to_string(&data) {
            let _ = fs::write(cache_file, json);
        }
        commands
    }

    /// Rebuild if PATH changed
    pub fn refresh_if_stale(&mut self) {
        let current_hash = hash_path_var();
        if current_hash != self.path_hash {
            self.path_hash = current_hash;
            self.commands = build_commands();
            // Save to cache
            let data = PathCacheData {
                commands: self.commands.clone(),
                built_at: now_secs(),
                path_hash: self.path_hash,
            };
            if let Ok(json) = serde_json::to_string(&data) {
                let _ = fs::write(&self.cache_file, json);
            }
        }
    }

    pub fn contains(&self, cmd: &str) -> bool {
        self.commands.contains(&cmd.to_string())
    }

    /// Find first binary matching prefix
    pub fn find_prefix(&self, prefix: &str) -> Option<&str> {
        self.commands.iter().find(|c| c.starts_with(prefix)).map(|s| s.as_str())
    }
}

fn build_commands() -> Vec<String> {
    let mut seen = HashSet::new();
    let mut commands = Vec::new();
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if seen.insert(name.clone()) {
                        // Only keep executables
                        if let Ok(meta) = entry.metadata() {
                            use std::os::unix::fs::PermissionsExt;
                            if meta.permissions().mode() & 0o111 != 0 {
                                commands.push(name);
                            }
                        }
                    }
                }
            }
        }
    }
    commands.sort_unstable();
    commands
}

fn hash_path_var() -> u64 {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    env::var("PATH").unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

fn now_secs() -> u64 {
    SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or(Duration::ZERO)
    .as_secs()
}
