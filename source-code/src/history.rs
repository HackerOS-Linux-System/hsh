use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub command:   String,
    pub timestamp: DateTime<Local>,
}

pub struct ShellHistory {
    pub entries: Vec<HistoryEntry>,
    path:        String,
    dirty:       bool,
}

impl ShellHistory {
    /// Wczytaj historię z pliku JSON.
    /// Ścieżka domyślna: ~/.hsh-history (bez podkatalogów).
    pub fn load(path: &str) -> Self {
        let canonical = resolve_history_path(path);
        let entries = if Path::new(&canonical).exists() {
            fs::read_to_string(&canonical)
                .ok()
                .and_then(|data| serde_json::from_str::<Vec<HistoryEntry>>(&data).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        ShellHistory {
            entries,
            path: canonical,
            dirty: false,
        }
    }

    /// Zapisz historię do pliku JSON.
    /// Tworzy katalog nadrzędny jeśli nie istnieje.
    pub fn save(&self, _path: &str) {
        // Używamy self.path (canonical), ignorujemy przekazany argument
        // żeby nie zapisywać do błędnej lokalizacji
        self.save_to(&self.path.clone());
    }

    pub fn save_to(&self, path: &str) {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        match serde_json::to_string_pretty(&self.entries) {
            Ok(data) => {
                // Atomowy zapis: pisz do .tmp potem rename
                let tmp = format!("{}.tmp", path);
                if let Ok(mut f) = fs::File::create(&tmp) {
                    if f.write_all(data.as_bytes()).is_ok() {
                        let _ = fs::rename(&tmp, path);
                    } else {
                        let _ = fs::remove_file(&tmp);
                    }
                }
            }
            Err(e) => eprintln!("hsh: history save error: {}", e),
        }
    }

    /// Dodaj wpis — pomijaj kolejne duplikaty i komendy zaczynające się od spacji.
    pub fn add(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() { return; }
        // Pomiń jeśli taka sama jak ostatnia
        if self.entries.last().map(|e| e.command.as_str()) == Some(cmd) {
            return;
        }
        self.entries.push(HistoryEntry {
            command:   cmd.to_string(),
            timestamp: Local::now(),
        });
        self.dirty = true;
        // Auto-zapis co 10 wpisów
        if self.entries.len() % 10 == 0 {
            self.save_to(&self.path.clone());
        }
    }

    /// Zwróć ostatnią komendę.
    pub fn last_command(&self) -> Option<String> {
        self.entries.last().map(|e| e.command.clone())
    }

    /// Wypisz wszystkie wpisy (nowsze na górze, maks 1000).
    pub fn print_all(&self) {
        let start = self.entries.len().saturating_sub(1000);
        for (i, entry) in self.entries[start..].iter().enumerate().rev() {
            println!(
                "\x1b[38;5;242m{:5}\x1b[0m  \x1b[38;5;238m{}\x1b[0m  {}",
                start + i + 1,
                entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                entry.command
            );
        }
    }

    /// Fuzzy search — zwraca deduplikowane wyniki posortowane wg score.
    pub fn fuzzy_search(&self, query: &str) -> Vec<&HistoryEntry> {
        use fuzzy_matcher::skim::SkimMatcherV2;
        use fuzzy_matcher::FuzzyMatcher;
        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(i64, &HistoryEntry)> = self
            .entries
            .iter()
            .filter_map(|e| {
                matcher
                    .fuzzy_match(&e.command, query)
                    .map(|score| (score, e))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        let mut seen = HashSet::new();
        scored
            .into_iter()
            .filter(|(_, e)| seen.insert(e.command.clone()))
            .map(|(_, e)| e)
            .collect()
    }

    /// Zwróć n ostatnich unikalnych komend (dla podpowiedzi).
    pub fn recent_unique(&self, n: usize) -> Vec<&str> {
        let mut seen = HashSet::new();
        self.entries
            .iter()
            .rev()
            .filter(|e| seen.insert(e.command.as_str()))
            .take(n)
            .map(|e| e.command.as_str())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// Rozwiąż ścieżkę historii — zawsze do katalogu HOME użytkownika.
fn resolve_history_path(path: &str) -> String {
    // Jeśli ścieżka zawiera tylko nazwę pliku (bez /), umieść w HOME
    if !path.contains('/') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        return format!("{}/{}", home, path);
    }

    // Jeśli ścieżka zaczyna się od ~, rozwiń
    if path.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        return format!("{}{}", home, &path[1..]);
    }

    path.to_string()
}

impl Drop for ShellHistory {
    fn drop(&mut self) {
        if self.dirty {
            self.save_to(&self.path.clone());
        }
    }
}
