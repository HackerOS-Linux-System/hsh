use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

/// Poziom krytyczności wykrytego wzorca.
///
/// `Critical`  — działanie potencjalnie nieodwracalne w skali całego systemu
///               (utrata danych na całym dysku, fork bomba). Wymaga przepisania
///               dokładnej komendy, samo "yes" nie wystarczy (ochrona przed
///               odruchowym Enter/paste).
/// `High`      — działanie ryzykowne, ale zwykle ograniczone w zasięgu
///               (force push, katalog domowy, zdalny kod bez weryfikacji).
///               Wymaga wpisania "yes".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Critical,
    High,
}

struct DangerRule {
    pattern:  &'static str,
    message:  &'static str,
    severity: Severity,
}

// Wzorce dopasowywane są przez regex na całej (znormalizowanej) linii komendy.
// Celowo dopuszczają dowolną kolejność/formę flag (np. `rm -fr`, `rm -Rf`),
// czego nie łapało poprzednie dopasowanie dosłownych podciągów.
static RULES: &[DangerRule] = &[
    DangerRule {
        pattern:  r"\brm\s+(-\S*\s+)*-[a-zA-Z]*r[a-zA-Z]*f[a-zA-Z]*\s+/(\s|$)",
        message:  "To usunie WSZYSTKIE pliki w systemie (rm -rf /)!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r"\brm\s+(-\S*\s+)*-[a-zA-Z]*f[a-zA-Z]*r[a-zA-Z]*\s+/(\s|$)",
        message:  "To usunie WSZYSTKIE pliki w systemie (rm -fr /)!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r"\brm\s+(-\S*\s+)*-[a-zA-Z]*r[a-zA-Z]*f[a-zA-Z]*\s+/\*",
        message:  "To usunie WSZYSTKIE pliki w katalogu głównym systemu!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r"\brm\s+(-\S*\s+)*-[a-zA-Z]*r[a-zA-Z]*f[a-zA-Z]*\s+~/?(\s|$)",
        message:  "To bezpowrotnie usunie cały katalog domowy!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r"\brm\s+(-\S*\s+)*-[a-zA-Z]*r[a-zA-Z]*f[a-zA-Z]*\s+\.\.?(/\S*)?(\s|$)",
        message:  "To rekurencyjnie usunie bieżący/nadrzędny katalog!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r"\bdd\s+.*\bof=/dev/(sd|nvme|hd|vd|xvd)",
        message:  "To nadpisze surowe dane na dysku (dd of=/dev/...)!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r"\bmkfs(\.\w+)?\s+.*?/dev/",
        message:  "To sformatuje partycję/dysk — utrata wszystkich danych!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r"\bchmod\s+(-R\s+)?0?777\s+/(\s|$)",
        message:  "To nada wszystkim prawa zapisu do całego systemu plików!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r":\(\)\s*\{\s*:\s*\|\s*:\s*&?\s*\}\s*;\s*:",
        message:  "FORK BOMBA — to zawiesi/zrestartuje system!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r">\s*/dev/(sd|nvme|hd|vd|xvd)[a-z0-9]*(\s|$)",
        message:  "To nadpisze surowe dane na urządzeniu blokowym!",
        severity: Severity::Critical,
    },
    DangerRule {
        pattern:  r"(curl|wget)\s+[^|;]*\|\s*(sudo\s+)?(sh|bash|zsh|hsh)\b",
        message:  "Uruchamiasz kod pobrany z sieci bez wcześniejszej weryfikacji!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r"\bgit\s+push\b[^|;&]*(--force(-with-lease)?\b|(^|\s)-f(\s|$))",
        message:  "Force push może bezpowrotnie nadpisać historię zdalnego repozytorium!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r"\bchown\s+(-R\s+)?\S+\s+/(\s|$)",
        message:  "To zmieni właściciela całego systemu plików!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r"\bshred\s+(-\S+\s+)*-[a-zA-Z]*u",
        message:  "To bezpowrotnie zniszczy dane na dysku (shred -u)!",
        severity: Severity::High,
    },
    DangerRule {
        pattern:  r"\bmv\s+/\*\s+/dev/null",
        message:  "To przeniesie (zniszczy) wszystkie pliki systemu!",
        severity: Severity::Critical,
    },
];

fn compiled_rules() -> &'static Vec<(Regex, &'static str, Severity)> {
    static CELL: OnceLock<Vec<(Regex, &'static str, Severity)>> = OnceLock::new();
    CELL.get_or_init(|| {
        RULES
            .iter()
            .filter_map(|r| {
                Regex::new(r.pattern)
                    .ok()
                    .map(|re| (re, r.message, r.severity))
            })
            .collect()
    })
}

/// Sprawdza czy komenda pasuje do znanego niebezpiecznego wzorca.
/// Zwraca poziom krytyczności i opis, jeśli tak.
pub fn check_dangerous(input: &str) -> Option<(Severity, &'static str)> {
    for (re, msg, sev) in compiled_rules() {
        if re.is_match(input) {
            return Some((*sev, msg));
        }
    }
    None
}

/// Podświetlenie niebezpiecznej linii w czasie pisania (przed wciśnięciem Enter).
/// Critical → migające czerwone tło; High → pogrubiony czerwony tekst.
pub fn highlight_dangerous(line: &str) -> Option<String> {
    if let Some((sev, _)) = check_dangerous(line) {
        let style = match sev {
            Severity::Critical => "\x1b[5;41;97m",
            Severity::High     => "\x1b[1;31m",
        };
        return Some(format!("{}{}\x1b[0m", style, line));
    }
    None
}

/// Pyta użytkownika o potwierdzenie niebezpiecznej komendy. Zwraca true jeśli można kontynuować.
///
/// - `Severity::High`     → trzeba wpisać dokładnie "yes".
/// - `Severity::Critical` → trzeba przepisać całą komendę tak, jak została wpisana
///   (chroni przed odruchowym potwierdzeniem/wklejeniem "yes" bez czytania).
pub fn confirm_dangerous(input: &str) -> bool {
    match check_dangerous(input) {
        Some((Severity::Critical, warning)) => {
            eprintln!("\x1b[1;31m⚠  KRYTYCZNE: {}\x1b[0m", warning);
            eprintln!("\x1b[1;33mAby kontynuować, przepisz dokładnie poniższą komendę:\x1b[0m");
            eprintln!("\x1b[38;5;244m  {}\x1b[0m", input.trim());
            eprint!("> ");
            io::stdout().flush().ok();
            let mut answer = String::new();
            io::stdin().read_line(&mut answer).ok();
            answer.trim() == input.trim()
        }
        Some((Severity::High, warning)) => {
            eprintln!("\x1b[1;31m⚠  UWAGA: {}\x1b[0m", warning);
            eprint!("\x1b[1;33mCzy na pewno chcesz kontynuować? Wpisz 'yes' aby potwierdzić: \x1b[0m");
            io::stdout().flush().ok();
            let mut answer = String::new();
            io::stdin().read_line(&mut answer).ok();
            answer.trim() == "yes"
        }
        None => true,
    }
}

/// Sprawdza naruszenie trybu `restricted` (odpowiednik `rbash`):
/// - nazwa komendy nie może zawierać '/' (musi być rozwiązywana przez PATH),
/// - przekierowania wyjścia (`>`, `>>`, `>|`, `<>`, `>&`, `&>`) są zablokowane.
/// Zwraca opis naruszenia, jeśli je wykryto.
pub fn restricted_violation(rest: &str) -> Option<&'static str> {
    if let Some(first) = rest.split_whitespace().next() {
        // Dopuszczamy przypisania zmiennych (FOO=bar) — te nie mają '/' zwykle,
        // a jeśli mają (np. w wartości), to i tak nie jest to nazwa komendy.
        if !first.contains('=') && first.contains('/') {
            return Some("nazwa polecenia nie może zawierać '/' w trybie restricted (użyj PATH)");
        }
    }
    if restricted_redirect_re().is_match(rest) {
        return Some("przekierowanie wyjścia jest zablokowane w trybie restricted");
    }
    None
}

fn restricted_redirect_re() -> &'static Regex {
    // Uwaga: crate `regex` (w przeciwieństwie do PCRE/`fancy-regex`) nie wspiera
    // lookahead/lookbehind, więc wzorzec musi być prosty. Każde wystąpienie '>'
    // (zwykłe przekierowanie, >>, >|, >&, &>) albo '<>' jest traktowane jako
    // przekierowanie wyjścia i blokowane w trybie restricted — łącznie z
    // duplikacją deskryptorów typu `2>&1`, co jest zgodne z zachowaniem rbash.
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| Regex::new(r">|<>").unwrap())
}

// ─────────────────────────────────────────────────────────────────────────────
// Redakcja sekretów przed zapisem do historii/logu audytowego
// ─────────────────────────────────────────────────────────────────────────────
//
// Chroni przed przypadkowym zapisaniem haseł/tokenów/kluczy API w plikach
// ~/.hsh-history i ~/.hsh-audit.log w postaci jawnej — jeśli ktoś uzyska
// dostęp do tych plików (backup, inny user na tej samej maszynie, wyciek),
// nie dostanie sekretów "za darmo". To ochrona at-rest, nie blokuje wykonania.

struct SecretRule {
    pattern: &'static str,
}

static SECRET_RULES: &[SecretRule] = &[
    // --password=xxx / --token=xxx / -p xxx itp.
    SecretRule { pattern: r"(?i)(--?(?:password|passwd|pass|token|api[_-]?key|secret|access[_-]?key)[= ])(\S+)" },
    // AWS access key id
    SecretRule { pattern: r"\b(AKIA[0-9A-Z]{16})\b" },
    // Bearer / Basic auth headers
    SecretRule { pattern: r"(?i)(Authorization:\s*(?:Bearer|Basic)\s+)(\S+)" },
    // URL z osadzonym hasłem: proto://user:pass@host
    SecretRule { pattern: r"([a-zA-Z]+://[^/\s:]+:)([^@\s]+)(@)" },
    // Prywatne klucze SSH/PEM w linii (nagłówek)
    SecretRule { pattern: r"-----BEGIN [A-Z ]*PRIVATE KEY-----" },
];

fn secret_rules() -> &'static Vec<Regex> {
    static CELL: OnceLock<Vec<Regex>> = OnceLock::new();
    CELL.get_or_init(|| {
        SECRET_RULES.iter().filter_map(|r| Regex::new(r.pattern).ok()).collect()
    })
}

/// Zwraca wersję komendy z zamaskowanymi sekretami — do zapisu w historii/logu.
/// Nie wpływa na faktyczne wykonanie komendy, tylko na to, co zostaje zapisane na dysku.
pub fn redact_secrets(command: &str) -> String {
    let mut out = command.to_string();
    for re in secret_rules() {
        out = re
            .replace_all(&out, |caps: &regex::Captures| {
                if caps.len() >= 3 {
                    format!("{}[REDACTED]{}", &caps[1], caps.get(3).map(|m| m.as_str()).unwrap_or(""))
                } else {
                    "[REDACTED]".to_string()
                }
            })
            .to_string();
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Wykrywanie potencjalnie niebezpiecznego PATH (klasyczny wektor PATH hijacking)
// ─────────────────────────────────────────────────────────────────────────────

/// Sprawdza zmienną PATH pod kątem katalogów, które mogłyby posłużyć do
/// podstawienia złośliwego binarnego pliku pod nazwą popularnego polecenia:
/// `.` (bieżący katalog) lub katalogi zapisywalne przez innych (world-writable).
/// Zwraca listę ostrzeżeń (pusta jeśli PATH wygląda bezpiecznie).
pub fn check_path_hijack_risks(path_var: &str) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;

    let mut warnings = Vec::new();
    for dir in path_var.split(':') {
        if dir.is_empty() { continue; }
        if dir == "." || dir == "" {
            warnings.push("PATH zawiera bieżący katalog ('.') — ryzyko podstawienia złośliwego pliku wykonywalnego".to_string());
            continue;
        }
        if let Ok(meta) = std::fs::metadata(dir) {
            let mode = meta.permissions().mode();
            // world-writable (inni mogą pisać) bez sticky bit
            let world_writable = mode & 0o002 != 0;
            let sticky = mode & 0o1000 != 0;
            if world_writable && !sticky {
                warnings.push(format!(
                    "katalog w PATH zapisywalny przez wszystkich (bez sticky bit): {}",
                    dir
                ));
            }
        }
    }
    warnings
}

// ─────────────────────────────────────────────────────────────────────────────
// Twarda lista blokowanych komend (nie proszą o potwierdzenie — po prostu odmawiają)
// ─────────────────────────────────────────────────────────────────────────────

/// Sprawdza, czy pierwsze słowo komendy znajduje się na liście `deny_commands`
/// z sekcji [safety] w ~/.hshrc. W przeciwieństwie do `check_dangerous`,
/// to twarda blokada — nie da się jej obejść potwierdzeniem.
pub fn is_denied_command(rest: &str, denied: &[String]) -> Option<String> {
    let first = rest.split_whitespace().next()?;
    // dopasuj też po ostatnim segmencie ścieżki (np. "/usr/bin/nc" blokuje "nc")
    let basename = first.rsplit('/').next().unwrap_or(first);
    if denied.iter().any(|d| d == first || d == basename) {
        Some(basename.to_string())
    } else {
        None
    }
}
//
// Każdy wykonany wiersz jest dopisywany razem ze skrótem zależnym od skrótu
// poprzedniego wpisu. Ciche ręczne wykasowanie/podmienienie linii w środku
// pliku przerywa łańcuch przy następnym uruchomieniu — `verify_audit_log`
// wykrywa to i ostrzega. To NIE jest ochrona kryptograficzna (DefaultHasher
// nie jest odporny na celowe kolizje) — to prosty mechanizm wykrywania
// przypadkowej/nieautoryzowanej modyfikacji logu, analogiczny do CRC.

fn audit_log_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    format!("{}/.hsh-audit.log", home)
}

fn hash_entry(prev_hash: &str, ts: u64, command: &str) -> String {
    let mut hasher = DefaultHasher::new();
    prev_hash.hash(&mut hasher);
    ts.hash(&mut hasher);
    command.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn last_hash(path: &str) -> String {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| content.lines().last().map(|l| l.to_string()))
        .and_then(|last_line| {
            last_line
                .split('\t')
                .nth(2)
                .map(|h| h.to_string())
        })
        .unwrap_or_else(|| "genesis".to_string())
}

/// Dopisz wykonaną komendę do łańcuchowego logu audytowego (`~/.hsh-audit.log`).
/// Sekrety (hasła, tokeny, klucze) są redagowane PRZED zapisem i haszowaniem —
/// nigdy nie trafiają na dysk w postaci jawnej.
pub fn audit_log(command: &str) {
    let path = audit_log_path();
    let prev = last_hash(&path);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let redacted = redact_secrets(command);
    let flat = redacted.replace('\n', " ⏎ ").replace('\t', " ");
    let hash = hash_entry(&prev, ts, &flat);

    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}\t{}\t{}\t{}", ts, prev, hash, flat);
    }
}

/// Zweryfikuj integralność łańcucha logu audytowego. Zwraca `Ok(liczba_wpisów)`
/// albo `Err(numer_pierwszego_zerwanego_ogniwa)`.
pub fn verify_audit_log() -> Result<usize, usize> {
    let path = audit_log_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(0),
    };
    let mut prev = "genesis".to_string();
    for (i, line) in content.lines().enumerate() {
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        if parts.len() != 4 {
            return Err(i + 1);
        }
        let ts: u64 = parts[0].parse().unwrap_or(0);
        let stored_prev = parts[1];
        let stored_hash = parts[2];
        let cmd = parts[3];
        if stored_prev != prev {
            return Err(i + 1);
        }
        let expected = hash_entry(stored_prev, ts, cmd);
        if expected != stored_hash {
            return Err(i + 1);
        }
        prev = stored_hash.to_string();
    }
    Ok(content.lines().count())
}
