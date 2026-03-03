use std::collections::HashMap;
use std::env;

/// Shell-local variables (not exported to env)
pub struct ShellVars {
    local: HashMap<String, String>,
}

impl ShellVars {
    pub fn new() -> Self {
        ShellVars {
            local: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.local.insert(key.to_string(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.local
        .get(key)
        .cloned()
        .or_else(|| env::var(key).ok())
    }

    /// Expand $VAR and ${VAR} in a string
    pub fn expand(&self, input: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' {
                i += 1;
                if i >= chars.len() {
                    result.push('$');
                    continue;
                }
                if chars[i] == '{' {
                    // ${VAR}
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != '}' {
                        i += 1;
                    }
                    let name: String = chars[start..i].iter().collect();
                    result.push_str(&self.get(&name).unwrap_or_default());
                    if i < chars.len() {
                        i += 1; // skip '}'
                    }
                } else {
                    // $VAR
                    let start = i;
                    while i < chars.len()
                        && (chars[i].is_alphanumeric() || chars[i] == '_')
                        {
                            i += 1;
                        }
                        let name: String = chars[start..i].iter().collect();
                    if name.is_empty() {
                        result.push('$');
                    } else {
                        result.push_str(&self.get(&name).unwrap_or_default());
                    }
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }
}

/// Parse inline assignments like FOO=bar before a command.
/// Returns (env_pairs, rest_of_command)
pub fn parse_inline_env(input: &str) -> (Vec<(String, String)>, String) {
    let parts = shlex::split(input).unwrap_or_default();
    let mut pairs = Vec::new();
    let mut rest_start = 0;
    for (idx, part) in parts.iter().enumerate() {
        if let Some(eq) = part.find('=') {
            let key = &part[..eq];
            // key must be a valid identifier
            if key.chars().all(|c| c.is_alphanumeric() || c == '_') && !key.is_empty() {
                let val = part[eq + 1..].to_string();
                pairs.push((key.to_string(), val));
                rest_start = idx + 1;
                continue;
            }
        }
        break;
    }
    let rest = parts[rest_start..].join(" ");
    (pairs, rest)
}
