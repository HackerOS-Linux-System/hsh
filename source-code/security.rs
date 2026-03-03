use std::io::{self, Write};

/// Extended list of dangerous patterns
static DANGEROUS_PATTERNS: &[(&str, &str)] = &[
    ("rm -rf /", "This will delete ALL files on your system!"),
    ("rm -rf /*", "This will delete ALL files on your system!"),
    ("dd if=/dev/zero of=/dev/sda", "This will wipe your disk!"),
    ("mkfs /dev/sda", "This will format your primary disk!"),
    ("chmod -R 777 /", "This will make all files world-writable!"),
    (":(){ :|:& };:", "FORK BOMB detected — this will crash the system!"),
    ("> /dev/sda", "This will overwrite your disk!"),
    ("mv /* /dev/null", "This will destroy all files!"),
    ("wget -O- | sh", "Executing remote code is dangerous!"),
    ("curl | sh", "Executing remote code is dangerous!"),
    ("curl | bash", "Executing remote code is dangerous!"),
    ("wget -O- | bash", "Executing remote code is dangerous!"),
];

/// Highlight pattern for terminal (red bold blinking)
pub fn highlight_dangerous(line: &str) -> Option<String> {
    for (pattern, _) in DANGEROUS_PATTERNS {
        if line.contains(pattern) {
            return Some(format!("\x1b[5;41m{}\x1b[0m", line));
        }
    }
    None
}

/// Returns warning message if dangerous, None otherwise
pub fn check_dangerous(input: &str) -> Option<&'static str> {
    for (pattern, warning) in DANGEROUS_PATTERNS {
        if input.contains(pattern) {
            return Some(warning);
        }
    }
    None
}

/// Ask user to confirm dangerous command. Returns true if confirmed.
pub fn confirm_dangerous(input: &str) -> bool {
    if let Some(warning) = check_dangerous(input) {
        eprintln!("\x1b[1;31m⚠  DANGER: {}\x1b[0m", warning);
        eprint!("\x1b[1;33mAre you sure? Type 'yes' to confirm: \x1b[0m");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).ok();
        answer.trim() == "yes"
    } else {
        true
    }
}
