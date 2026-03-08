use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
pub struct SmartHints {
    sequences: HashMap<String, HashMap<String, u64>>,
    usage:     HashMap<String, u64>,
    prefixes:  HashMap<String, HashMap<String, u64>>,
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
        let current = current.trim();
        if current.is_empty() { return; }
        *self.usage.entry(current.to_string()).or_insert(0) += 1;
        if !prev.is_empty() {
            let map = self.sequences.entry(prev.to_string()).or_default();
            *map.entry(current.to_string()).or_insert(0) += 1;
        }
        let first_word = current.split_whitespace().next().unwrap_or(current);
        let map = self.prefixes.entry(first_word.to_string()).or_default();
        *map.entry(current.to_string()).or_insert(0) += 1;
    }

    pub fn suggest_next(&self, prev: &str) -> Option<&str> {
        let map = self.sequences.get(prev)?;
        map.iter()
        .max_by_key(|(_, &c)| c)
        .map(|(cmd, _)| cmd.as_str())
    }

    pub fn suggest_completion(&self, partial: &str) -> Option<&str> {
        let partial = partial.trim();
        if partial.is_empty() { return None; }
        let first_word = partial.split_whitespace().next()?;
        let map = self.prefixes.get(first_word)?;
        map.iter()
        .filter(|(cmd, _)| cmd.starts_with(partial) && cmd.len() > partial.len())
        .max_by_key(|(_, &c)| c)
        .map(|(cmd, _)| cmd.as_str())
    }

    pub fn top_commands(&self, n: usize) -> Vec<&str> {
        let mut v: Vec<(&str, u64)> = self.usage
        .iter()
        .map(|(k, &v)| (k.as_str(), v))
        .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.into_iter().take(n).map(|(k, _)| k).collect()
    }

    pub fn spellcheck<'a>(&self, input: &str, known: &'a [String]) -> Option<&'a str> {
        let word = input.split_whitespace().next()?.to_lowercase();
        if word.len() < 2 { return None; }
        let (best, dist) = known
        .iter()
        .map(|cmd| (cmd.as_str(), levenshtein(&word, cmd)))
        .min_by_key(|(_, d)| *d)?;
        let threshold = (word.len() / 3).max(1).min(3);
        if dist > 0 && dist <= threshold { Some(best) } else { None }
    }

    // ── Gettery dla helper.rs ─────────────────────────────────────────────────

    pub fn prefixes_ref(&self) -> &HashMap<String, HashMap<String, u64>> {
        &self.prefixes
    }

    pub fn sequences_ref(&self) -> &HashMap<String, HashMap<String, u64>> {
        &self.sequences
    }
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    if m == 0 { return n; }
    if n == 0 { return m; }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
