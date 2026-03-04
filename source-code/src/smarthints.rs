use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

/// Tracks command sequences to suggest next command
#[derive(Serialize, Deserialize, Default)]
pub struct SmartHints {
    /// Map: previous_command -> {next_command -> count}
    sequences: HashMap<String, HashMap<String, u64>>,
    /// Map: command -> total usage count
    usage: HashMap<String, u64>,
}

impl SmartHints {
    pub fn load(path: &str) -> Self {
        if Path::new(path).exists() {
            if let Ok(data) = fs::read_to_string(path) {
                if let Ok(s) = serde_json::from_str(&data) {
                    return s;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self, path: &str) {
        if let Ok(data) = serde_json::to_string(&self) {
            let _ = fs::write(path, data);
        }
    }

    pub fn record(&mut self, prev: &str, current: &str) {
        *self.usage.entry(current.to_string()).or_insert(0) += 1;
        if !prev.is_empty() {
            let next_map = self.sequences.entry(prev.to_string()).or_default();
            *next_map.entry(current.to_string()).or_insert(0) += 1;
        }
    }

    /// Suggest the most likely next command after `prev`
    pub fn suggest_next(&self, prev: &str) -> Option<&str> {
        let next_map = self.sequences.get(prev)?;
        next_map
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(cmd, _)| cmd.as_str())
    }

    /// Spellcheck: find closest command using Levenshtein distance
    pub fn spellcheck<'a>(&self, input: &str, known: &'a [String]) -> Option<&'a str> {
        let input_lower = input.to_lowercase();
        // Only check first word
        let word = input_lower.split_whitespace().next()?;
        if word.len() < 2 { return None; }

        let (best, dist) = known
        .iter()
        .map(|cmd| (cmd.as_str(), levenshtein(word, cmd)))
        .min_by_key(|(_, d)| *d)?;

        // Only suggest if distance is small relative to word length
        let threshold = (word.len() / 3).max(1).min(3);
        if dist > 0 && dist <= threshold {
            Some(best)
        } else {
            None
        }
    }
}

/// Classic Levenshtein distance
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 { return n; }
    if n == 0 { return m; }

    // Use two rows instead of full matrix — O(n) memory
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
            .min(curr[j - 1] + 1)
            .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
