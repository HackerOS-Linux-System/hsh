use std::collections::HashMap;
use std::env;
use std::process::Command;

pub struct ShellVars {
    local:         HashMap<String, String>,
    pub last_exit: i32,
    pub positional: Vec<String>,
}

impl ShellVars {
    pub fn new() -> Self {
        ShellVars {
            local:      HashMap::new(),
            last_exit:  0,
            positional: Vec::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.local.insert(key.to_string(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<String> {
        // Special variables
        match key {
            "?"  => return Some(self.last_exit.to_string()),
            "$"  => return Some(std::process::id().to_string()),
            "0"  => return Some("hsh".to_string()),
            "#"  => return Some(self.positional.len().to_string()),
            "@"  => return Some(self.positional.join(" ")),
            "*"  => return Some(self.positional.join(" ")),
            _    => {}
        }

        // Positional $1 $2 ...
        if let Ok(n) = key.parse::<usize>() {
            if n >= 1 {
                return self.positional.get(n - 1).cloned();
            }
        }

        self.local.get(key).cloned().or_else(|| env::var(key).ok())
    }

    /// Returns merged map: env vars + local vars (local wins on conflict)
    pub fn all(&self) -> HashMap<String, String> {
        let mut map: HashMap<String, String> = env::vars().collect();
        for (k, v) in &self.local {
            map.insert(k.clone(), v.clone());
        }
        // Add special vars
        map.insert("?".to_string(),  self.last_exit.to_string());
        map.insert("$".to_string(),  std::process::id().to_string());
        map.insert("0".to_string(),  "hsh".to_string());
        map.insert("#".to_string(),  self.positional.len().to_string());
        map.insert("@".to_string(),  self.positional.join(" "));
        map
    }

    /// Expand $VAR, ${VAR}, $?, $$, $0, $#, $@, and $(...) in a string.
    pub fn expand(&self, input: &str) -> String {
        let expanded = self.expand_command_substitution(input);
        self.expand_vars(&expanded)
    }

    /// Expand $(command) and `command` substitutions.
    fn expand_command_substitution(&self, input: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // $( ... )
            if chars[i] == '$' && chars.get(i + 1) == Some(&'(')
                && chars.get(i + 2) != Some(&'(')  // not arithmetic
                {
                    i += 2;
                    let mut depth = 1i32;
                    let start = i;
                    while i < chars.len() {
                        if chars[i] == '(' { depth += 1; }
                        if chars[i] == ')' {
                            depth -= 1;
                            if depth == 0 { break; }
                        }
                        i += 1;
                    }
                    let cmd: String = chars[start..i].iter().collect();
                    if i < chars.len() { i += 1; } // skip )
                    result.push_str(&run_substitution(&cmd));
                }
                // backtick `command`
                else if chars[i] == '`' {
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != '`' { i += 1; }
                    let cmd: String = chars[start..i].iter().collect();
                    if i < chars.len() { i += 1; } // skip closing `
                    result.push_str(&run_substitution(&cmd));
                }
                else {
                    result.push(chars[i]);
                    i += 1;
                }
        }
        result
    }

    /// Expand $VAR and ${VAR} (after command substitution is done).
    fn expand_vars(&self, input: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] != '$' {
                result.push(chars[i]);
                i += 1;
                continue;
            }

            i += 1; // skip $
            if i >= chars.len() {
                result.push('$');
                continue;
            }

            match chars[i] {
                // ${VAR} or ${VAR:-default} or ${VAR:+alt} or ${VAR:?err}
                '{' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '}' && chars[i] != ':' { i += 1; }
                let name: String = chars[start..i].iter().collect();

                // Check for modifiers :- :+ :?
                let modifier = if i < chars.len() && chars[i] == ':' {
                    i += 1;
                    let mod_char = chars.get(i).copied();
                    i += 1;
                    let mod_start = i;
                    while i < chars.len() && chars[i] != '}' { i += 1; }
                    let mod_val: String = chars[mod_start..i].iter().collect();
                    Some((mod_char, mod_val))
                } else {
                    None
                };

                if i < chars.len() { i += 1; } // skip }

                let val = self.get(&name);
                let expanded = match modifier {
                    Some((Some('-'), default)) => {
                        val.filter(|v| !v.is_empty()).unwrap_or(default)
                    }
                    Some((Some('+'), alt)) => {
                        if val.as_deref().map(|v| !v.is_empty()).unwrap_or(false) {
                            alt
                        } else {
                            String::new()
                        }
                    }
                    Some((Some('?'), msg)) => {
                        match val {
                            Some(v) if !v.is_empty() => v,
                            _ => {
                                eprintln!("hsh: {}: {}", name, msg);
                                String::new()
                            }
                        }
                    }
                    _ => val.unwrap_or_default(),
                };
                result.push_str(&expanded);
                }

                // Special single-char vars: $? $$ $0 $# $@ $*
                '?' | '$' | '0' | '#' | '@' | '*' => {
                    let key = chars[i].to_string();
                    result.push_str(&self.get(&key).unwrap_or_default());
                    i += 1;
                }

                // $VAR
                c if c.is_alphanumeric() || c == '_' => {
                    let start = i;
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    let name: String = chars[start..i].iter().collect();
                    result.push_str(&self.get(&name).unwrap_or_default());
                }

                // Bare $ not followed by a valid char
                _ => {
                    result.push('$');
                }
            }
        }
        result
    }
}

/// Run a command substitution, return trimmed stdout.
fn run_substitution(cmd: &str) -> String {
    match Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            // POSIX: strip trailing newlines
            s.trim_end_matches('\n').to_string()
        }
        Err(_) => String::new(),
    }
}

/// Parse inline assignments like FOO=bar before a command.
/// Returns (env_pairs, rest_of_command)
pub fn parse_inline_env(input: &str) -> (Vec<(String, String)>, String) {
    let parts = shlex::split(input).unwrap_or_default();
    let mut pairs     = Vec::new();
    let mut rest_start = 0;

    for (idx, part) in parts.iter().enumerate() {
        if let Some(eq) = part.find('=') {
            let key = &part[..eq];
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                pairs.push((key.to_string(), part[eq + 1..].to_string()));
                rest_start = idx + 1;
                continue;
            }
        }
        break;
    }

    let rest = parts[rest_start..].join(" ");
    (pairs, rest)
}
