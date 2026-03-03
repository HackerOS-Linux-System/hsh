use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub command: String,
    pub timestamp: DateTime<Local>,
}

pub struct ShellHistory {
    pub entries: Vec<HistoryEntry>,
}

impl ShellHistory {
    pub fn load(path: &str) -> Self {
        if Path::new(path).exists() {
            if let Ok(data) = fs::read_to_string(path) {
                if let Ok(entries) = serde_json::from_str::<Vec<HistoryEntry>>(&data) {
                    return ShellHistory { entries };
                }
            }
        }
        ShellHistory { entries: Vec::new() }
    }

    pub fn save(&self, path: &str) {
        if let Ok(data) = serde_json::to_string_pretty(&self.entries) {
            let _ = fs::write(path, data);
        }
    }

    /// Add entry, deduplicating consecutive identical commands
    pub fn add(&mut self, cmd: &str) {
        if self.entries.last().map(|e| e.command.as_str()) == Some(cmd) {
            return;
        }
        self.entries.push(HistoryEntry {
            command: cmd.to_string(),
                          timestamp: Local::now(),
        });
    }

    /// Print history with timestamps
    pub fn print_all(&self) {
        for (i, entry) in self.entries.iter().enumerate().rev().take(500) {
            println!(
                "{:4}  {}  {}",
                i + 1,
                entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                     entry.command
            );
        }
    }

    /// Fuzzy search using fuzzy-matcher
    pub fn fuzzy_search(&self, query: &str) -> Vec<&HistoryEntry> {
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;
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
        // Deduplicate by command
        let mut seen = HashSet::new();
        scored
        .into_iter()
        .filter(|(_, e)| seen.insert(e.command.clone()))
        .map(|(_, e)| e)
        .collect()
    }
}
