use std::collections::HashMap;
use std::env;
use std::process::Command;
use std::time::Instant;
use rand::Rng;

pub struct ShellVars {
    pub local:      HashMap<String, String>,
    pub last_exit:  i32,
    pub positional: Vec<String>,
    pub errexit:    bool,
    pub xtrace:     bool,
    pub nounset:    bool,
    pub start_time: Instant,
    pub line_no:    usize,
    pub dir_stack:  Vec<String>,
}

impl ShellVars {
    pub fn new() -> Self {
        let mut s = ShellVars {
            local:      HashMap::new(),
            last_exit:  0,
            positional: Vec::new(),
            errexit:    false,
            xtrace:     false,
            nounset:    false,
            start_time: Instant::now(),
            line_no:    0,
            dir_stack:  Vec::new(),
        };
        // Domyślne IFS
        s.local.insert("IFS".to_string(), " \t\n".to_string());
        s
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.local.insert(key.to_string(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "?"      => return Some(self.last_exit.to_string()),
            "$"      => return Some(std::process::id().to_string()),
            "0"      => return Some("hsh".to_string()),
            "#"      => return Some(self.positional.len().to_string()),
            "@" | "*" => return Some(self.positional.join(" ")),
            "RANDOM"  => return Some(rand::thread_rng().gen_range(0u32..=32767).to_string()),
            "SECONDS" => return Some(self.start_time.elapsed().as_secs().to_string()),
            "PWD"     => return Some(
                env::current_dir().unwrap_or_default().to_string_lossy().to_string()
            ),
            "OLDPWD"  => return self.local.get("OLDPWD").cloned(),
            "LINENO"  => return Some(self.line_no.to_string()),
            "PPID"    => return Some(unsafe { libc::getppid() }.to_string()),
            "UID"     => return Some(unsafe { libc::getuid() }.to_string()),
            "EUID"    => return Some(unsafe { libc::geteuid() }.to_string()),
            "HOSTNAME" => {
                return std::fs::read_to_string("/etc/hostname")
                    .ok()
                    .map(|h| h.trim().to_string())
                    .or_else(|| env::var("HOSTNAME").ok());
            }
            _ => {}
        }

        // Argumenty pozycyjne $1 $2 ...
        if let Ok(n) = key.parse::<usize>() {
            if n >= 1 {
                return self.positional.get(n - 1).cloned();
            }
        }

        self.local.get(key).cloned().or_else(|| env::var(key).ok())
    }

    /// Zwraca scaloną mapę: env vars + lokalne (lokalne mają priorytet)
    pub fn all(&self) -> HashMap<String, String> {
        let mut map: HashMap<String, String> = env::vars().collect();
        for (k, v) in &self.local {
            map.insert(k.clone(), v.clone());
        }
        map.insert("?".to_string(),       self.last_exit.to_string());
        map.insert("$".to_string(),       std::process::id().to_string());
        map.insert("0".to_string(),       "hsh".to_string());
        map.insert("#".to_string(),       self.positional.len().to_string());
        map.insert("@".to_string(),       self.positional.join(" "));
        map.insert("*".to_string(),       self.positional.join(" "));
        map.insert("RANDOM".to_string(),  rand::thread_rng().gen_range(0u32..=32767).to_string());
        map.insert("SECONDS".to_string(), self.start_time.elapsed().as_secs().to_string());
        map.insert("PWD".to_string(),
            env::current_dir().unwrap_or_default().to_string_lossy().to_string());
        if let Some(old) = self.local.get("OLDPWD") {
            map.insert("OLDPWD".to_string(), old.clone());
        }
        map.insert("LINENO".to_string(), self.line_no.to_string());
        map
    }

    /// Ekspanduj zmienne i podstawianie komend w stringu.
    pub fn expand(&self, input: &str) -> String {
        let after_cmd = self.expand_command_substitution(input);
        self.expand_vars(&after_cmd)
    }

    /// Ekspanduj $(command) i `command`.
    fn expand_command_substitution(&self, input: &str) -> String {
        let mut result = String::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // $( ... ) — nie mylić z $(( ... ))
            if chars[i] == '$'
                && chars.get(i + 1) == Some(&'(')
                && chars.get(i + 2) != Some(&'(')
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
                if i < chars.len() { i += 1; }
                result.push_str(&run_substitution(&cmd));
            }
            // `command`
            else if chars[i] == '`' {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '`' { i += 1; }
                let cmd: String = chars[start..i].iter().collect();
                if i < chars.len() { i += 1; }
                result.push_str(&run_substitution(&cmd));
            }
            else {
                result.push(chars[i]);
                i += 1;
            }
        }
        result
    }

    /// Ekspanduj $VAR i ${VAR} (po ekspansji komend).
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

            i += 1;
            if i >= chars.len() {
                result.push('$');
                continue;
            }

            match chars[i] {
                // ${VAR} ze wszystkimi modyfikatorami
                '{' => {
                    i += 1;
                    let start = i;
                    // Zbierz nazwę zmiennej (do } lub modyfikatora)
                    while i < chars.len() && chars[i] != '}' && chars[i] != ':' && chars[i] != '#' {
                        // Obsługa ${#VAR} — długość
                        break;
                    }

                    // Sprawdź ${#VAR}
                    if start < chars.len() && chars[start] == '#' && chars.get(start + 1) != Some(&'}') {
                        let name_start = start + 1;
                        let mut j = name_start;
                        while j < chars.len() && chars[j] != '}' { j += 1; }
                        let name: String = chars[name_start..j].iter().collect();
                        if j < chars.len() { i = j + 1; } else { i = j; }
                        let len = self.get(&name).unwrap_or_default().len();
                        result.push_str(&len.to_string());
                        continue;
                    }

                    let mut j = start;
                    while j < chars.len() && chars[j] != '}' && chars[j] != ':' { j += 1; }
                    let name: String = chars[start..j].iter().collect();

                    // Modyfikatory :-, :+, :?, :=, #, ##, %, %%
                    let modifier = if j < chars.len() && chars[j] == ':' {
                        j += 1;
                        let mod_char = chars.get(j).copied();
                        j += 1;
                        let mod_start = j;
                        let mut depth = 0i32;
                        while j < chars.len() {
                            if chars[j] == '{' { depth += 1; }
                            if chars[j] == '}' {
                                if depth == 0 { break; }
                                depth -= 1;
                            }
                            j += 1;
                        }
                        let mod_val: String = chars[mod_start..j].iter().collect();
                        Some((mod_char, mod_val))
                    } else {
                        None
                    };

                    if j < chars.len() { i = j + 1; } else { i = j; }

                    let val = self.get(&name);
                    let expanded = match modifier {
                        Some((Some('-'), default)) => {
                            val.filter(|v| !v.is_empty()).unwrap_or_else(|| self.expand(&default))
                        }
                        Some((Some('+'), alt)) => {
                            if val.as_deref().map(|v| !v.is_empty()).unwrap_or(false) {
                                self.expand(&alt)
                            } else {
                                String::new()
                            }
                        }
                        Some((Some('?'), msg)) => {
                            match val {
                                Some(v) if !v.is_empty() => v,
                                _ => {
                                    let m = self.expand(&msg);
                                    eprintln!("hsh: {}: {}", name, if m.is_empty() { "parameter not set".to_string() } else { m });
                                    if self.nounset { std::process::exit(1); }
                                    String::new()
                                }
                            }
                        }
                        Some((Some('='), default)) => {
                            // ${VAR:=default} — ustaw zmienną jeśli pusta
                            match val {
                                Some(v) if !v.is_empty() => v,
                                _ => {
                                    let d = self.expand(&default);
                                    // Nie możemy mutować self tutaj, ale zapisujemy przez env
                                    env::set_var(&name, &d);
                                    d
                                }
                            }
                        }
                        _ => {
                            if self.nounset && val.is_none() {
                                eprintln!("hsh: {}: unbound variable", name);
                                std::process::exit(1);
                            }
                            val.unwrap_or_default()
                        }
                    };
                    result.push_str(&expanded);
                }

                // Specjalne zmienne jednoargumentowe: $? $$ $0 $# $@ $* $! $-
                '?' | '$' | '0' | '#' | '@' | '*' | '!' | '-' => {
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
                    let val = self.get(&name);
                    if self.nounset && val.is_none() {
                        eprintln!("hsh: {}: unbound variable", name);
                        std::process::exit(1);
                    }
                    result.push_str(&val.unwrap_or_default());
                }

                _ => {
                    result.push('$');
                }
            }
        }
        result
    }

    pub fn set_option(&mut self, name: &str, value: bool) {
        match name {
            "errexit" | "e" => self.errexit = value,
            "xtrace"  | "x" => self.xtrace  = value,
            "nounset" | "u" => self.nounset  = value,
            _ => {}
        }
    }

    pub fn set_pwd(&mut self) {
        let pwd = env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if let Some(old) = self.local.get("PWD").cloned() {
            self.local.insert("OLDPWD".to_string(), old);
        }
        self.local.insert("PWD".to_string(), pwd.clone());
        env::set_var("PWD", &pwd);
    }

    /// Prosta ekspansja zmiennych w heredoc (bez podstawiania komend).
    pub fn expand_in_heredoc(&self, s: &str) -> String {
        let mut result = s.to_string();
        // Ekspanduj ${VAR} i $VAR dla lokalnych zmiennych
        for (k, v) in &self.local {
            result = result.replace(&format!("${{{}}}", k), v);
            result = result.replace(&format!("${}", k), v);
        }
        // Specjalne zmienne
        result = result.replace("$?", &self.last_exit.to_string());
        result = result.replace("$$", &std::process::id().to_string());
        result
    }
}

/// Uruchom podstawianie komendy, zwróć przycięty stdout.
fn run_substitution(cmd: &str) -> String {
    // Użyj hsh -c jeśli dostępny, fallback do sh
    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let out = Command::new(&shell)
        .arg("-c")
        .arg(cmd)
        .output();

    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            // POSIX: usuń końcowe newlines
            s.trim_end_matches('\n').to_string()
        }
        Err(_) => String::new(),
    }
}

/// Parsuj inline przypisania zmiennych przed komendą.
/// Zwraca (lista_par, reszta_komendy).
pub fn parse_inline_env(input: &str) -> (Vec<(String, String)>, String) {
    let parts = shlex::split(input).unwrap_or_default();
    let mut pairs      = Vec::new();
    let mut rest_start = 0;

    for (idx, part) in parts.iter().enumerate() {
        if let Some(eq) = part.find('=') {
            let key = &part[..eq];
            // Klucz musi być poprawną nazwą zmiennej
            if !key.is_empty()
                && key.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
                && key.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
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
