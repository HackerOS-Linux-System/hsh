use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

use hk_parser::{load_hk_file, resolve_interpolations, HkConfig};
use indexmap::IndexMap;

/// Domyślna zawartość pliku .hshrc w formacie hk
const DEFAULT_HSHRC: &str = r#"# ─────────────────────────────────────────────────────────────────────────────
# ~/.hshrc — HackerOS Shell konfiguracja (format: hk-parser 0.3.0)
# Wygenerowano automatycznie przez hsh 0.4.0
# ─────────────────────────────────────────────────────────────────────────────

[shell]
version = "0.4.0"
history_file   = "~/.hsh-history"
history_limit  = 10000
hints_file     = "~/.hsh-hints.json"
path_cache     = "~/.hsh-path-cache.json"
theme_file     = "~/.config/hackeros/hsh/theme.json"
default_theme  = "default"
# Opcje powłoki (jak set -e / set -x)
errexit        = false
xtrace         = false
nounset        = false

[prompt]
# Kolejność segmentów promptu (oddzielone przecinkiem)
segment_order  = "time, dir, git, mem_cpu"
# Znak promptu można nadpisać tutaj (jeśli pusty — używa motywu)
# prompt_char  = "❯"
# Czy pokazywać czas trwania komendy (w ms) gdy > 2000ms
show_duration  = true
# Czy pokazywać exit code gdy != 0
show_exit_code = true

[aliases]
# Skróty komend
ll    = "ls -la"
la    = "ls -a"
l     = "ls -l"
..    = "cd .."
...   = "cd ../.."
....  = "cd ../../.."
grep  = "grep --color=auto"
diff  = "diff --color=auto"
mkdir = "mkdir -p"
# Git
gs    = "git status"
ga    = "git add"
gc    = "git commit -m"
gp    = "git push"
gl    = "git log --oneline --graph --decorate"
gd    = "git diff"
gco   = "git checkout"
gb    = "git branch"
# Cargo
cb    = "cargo build"
cr    = "cargo run"
ct    = "cargo test"
cc    = "cargo check"
ccl   = "cargo clippy"
# System
cls   = "clear"
q     = "exit"
h     = "history"
j     = "jobs"

[env]
# Dodatkowe zmienne środowiskowe ładowane przy starcie
# EDITOR = "nano"
# VISUAL = "nano"
# PAGER  = "less"
# Przykład:
# MY_VAR = "wartość"

[completion]
# Czy uzupełniać pliki ukryte (zaczynające się od .)
show_hidden    = false
# Maks liczba podpowiedzi do wyświetlenia
max_completions = 20

[safety]
# Lista wzorców uznawanych za niebezpieczne (poza wbudowanymi)
# extra_dangerous = ["sudo rm -rf"]
confirm_dangerous = true

[scripts]
# Katalogi przeszukiwane przy source / . (oprócz PATH)
# extra_paths = ["~/.hsh/scripts", "~/bin"]
auto_chmod = true
"#;

pub fn load_shell_config() -> HkConfig {
    let home = env::var("HOME").unwrap_or_default();
    let config_path = format!("{}/.hshrc", home);

    // Wygeneruj domyślny .hshrc jeśli nie istnieje
    if !Path::new(&config_path).exists() {
        generate_default_hshrc(&config_path);
    }

    let mut config = load_hk_file(&config_path).unwrap_or_else(|_| IndexMap::new());
    resolve_interpolations(&mut config).ok();
    config
}

/// Generuje domyślny plik .hshrc
fn generate_default_hshrc(path: &str) {
    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(path, DEFAULT_HSHRC) {
        Ok(_) => {
            eprintln!(
                "\x1b[38;5;110m[hsh]\x1b[0m Wygenerowano domyślny plik konfiguracji: \x1b[1m{}\x1b[0m",
                path
            );
        }
        Err(e) => {
            eprintln!("hsh: nie można utworzyć .hshrc: {}", e);
        }
    }
}

pub fn get_aliases(config: &HkConfig) -> HashMap<String, String> {
    config
        .get("aliases")
        .and_then(|v| v.as_map().ok())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
                .collect()
        })
        .unwrap_or_default()
}

pub fn get_prompt_config(config: &HkConfig) -> HashMap<String, String> {
    config
        .get("prompt")
        .and_then(|v| v.as_map().ok())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
                .collect()
        })
        .unwrap_or_default()
}

/// Pobierz zmienne środowiskowe z sekcji [env]
pub fn get_env_vars(config: &HkConfig) -> HashMap<String, String> {
    config
        .get("env")
        .and_then(|v| v.as_map().ok())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
                .collect()
        })
        .unwrap_or_default()
}

/// Pobierz opcje powłoki z sekcji [shell]
pub fn get_shell_options(config: &HkConfig) -> HashMap<String, String> {
    config
        .get("shell")
        .and_then(|v| v.as_map().ok())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().ok().map(|val| (k.clone(), val)))
                .collect()
        })
        .unwrap_or_default()
}

/// Pobierz kolejność segmentów promptu z konfiguracji
pub fn get_segment_order(config: &HashMap<String, String>) -> Vec<String> {
    if let Some(order) = config.get("segment_order") {
        order.split(',').map(|s| s.trim().to_string()).collect()
    } else {
        vec![
            "time".to_string(),
            "dir".to_string(),
            "git".to_string(),
            "mem_cpu".to_string(),
        ]
    }
}

/// Pobierz ścieżkę historii z konfiguracji
pub fn get_history_path(config: &HkConfig) -> String {
    let home = env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    config
        .get("shell")
        .and_then(|v| v.as_map().ok())
        .and_then(|m| m.get("history_file"))
        .and_then(|v| v.as_string().ok())
        .map(|p| {
            if p.starts_with('~') {
                format!("{}{}", home, &p[1..])
            } else {
                p
            }
        })
        .unwrap_or_else(|| format!("{}/.hsh-history", home))
}

/// Pobierz limit historii z konfiguracji
pub fn get_history_limit(config: &HkConfig) -> usize {
    config
        .get("shell")
        .and_then(|v| v.as_map().ok())
        .and_then(|m| m.get("history_limit"))
        .and_then(|v| v.as_string().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(10000)
}
