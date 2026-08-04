use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use rustyline::Editor;

use crate::helper::ShellHelper;
use crate::history::ShellHistory;
use crate::jobs::JobTable;
use crate::vars::ShellVars;

fn expand_tilde(s: &str) -> String {
    if s.starts_with('~') {
        if let Ok(home) = env::var("HOME") {
            format!("{}{}", home, &s[1..])
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    }
}

/// Sprawdź, czy dana nazwa binarki istnieje w którymś z katalogów PATH.
/// Zwraca pełną ścieżkę jeśli tak.
fn which(name: &str) -> Option<PathBuf> {
    let path_var = env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        if dir.is_empty() { continue; }
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn parse_job_id(s: &str) -> Option<usize> {
    if s.starts_with('%') {
        s[1..].parse().ok()
    } else if s == "%" {
        Some(1)
    } else {
        s.parse().ok()
    }
}

pub fn handle_builtin(
    cmd:            &str,
    _rl:            &mut Editor<ShellHelper, rustyline::history::FileHistory>,
    prev_dir:       &mut Option<PathBuf>,
    jobs:           &mut JobTable,
    shell_history:  &ShellHistory,
    aliases:        &HashMap<String, String>,
    dry_run:        bool,
    vars:           &mut ShellVars,
    _heredoc_bodies: &HashMap<String, String>,
) -> Option<i32> {
    let trimmed = cmd.trim();

    // ── cd ───────────────────────────────────────────────────────────────────
    if trimmed == "cd" || trimmed.starts_with("cd ") {
        if vars.restricted {
            eprintln!("hsh: cd: nie dozwolone w trybie restricted");
            return Some(1);
        }
        let dir_str = trimmed.strip_prefix("cd").unwrap_or("").trim();
        let target_dir = if dir_str.is_empty() {
            env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else if dir_str == "-" {
            if let Some(pd) = prev_dir.take() {
                let p = pd.to_string_lossy().to_string();
                println!("{}", p);
                p
            } else {
                println!("cd: no previous directory");
                return Some(1);
            }
        } else {
            expand_tilde(dir_str)
        };
        if dry_run {
            println!("[dry-run] cd {}", target_dir);
            return Some(0);
        }
        let current = env::current_dir().unwrap_or(PathBuf::from("/"));
        if env::set_current_dir(&target_dir).is_ok() {
            *prev_dir = Some(current);
            vars.set_pwd();
            Some(0)
        } else {
            eprintln!("cd: no such file or directory: {}", target_dir);
            Some(1)
        }
    }

    // ── exit ─────────────────────────────────────────────────────────────────
    else if trimmed == "exit" || trimmed.starts_with("exit ") {
        let code: i32 = trimmed
        .strip_prefix("exit")
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(vars.last_exit);
        if let Some(cmd) = vars.traps.get("EXIT").cloned() {
            let _ = std::process::Command::new("sh").arg("-c").arg(&cmd).status();
        }
        std::process::exit(code);
    }

    // ── history ──────────────────────────────────────────────────────────────
    else if trimmed == "history" || trimmed.starts_with("history ") {
        let arg = trimmed.strip_prefix("history").unwrap_or("").trim();
        if arg.is_empty() {
            shell_history.print_all();
        } else if arg == "-c" {
            // Wyczyść historię (nie modyfikujemy shell_history bezpośrednio)
            eprintln!("hsh: history -c: wyczyść historię przez usunięcie pliku ~/.hsh-history");
        } else {
            let results = shell_history.fuzzy_search(arg);
            for entry in results.iter().take(20) {
                println!(
                    "  \x1b[38;5;242m{}\x1b[0m  {}",
                    entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                         entry.command
                );
            }
        }
        Some(0)
    }

    // ── which / type ─────────────────────────────────────────────────────────
    else if trimmed.starts_with("which ") || trimmed.starts_with("type ") {
        let name = if trimmed.starts_with("which ") {
            trimmed.strip_prefix("which ").unwrap_or("").trim()
        } else {
            trimmed.strip_prefix("type ").unwrap_or("").trim()
        };
        resolve_type(name, aliases, vars);
        Some(0)
    }

    // ── jobs ─────────────────────────────────────────────────────────────────
    else if trimmed == "jobs" {
        jobs.list();
        Some(0)
    }

    // ── fg ───────────────────────────────────────────────────────────────────
    else if trimmed == "fg" || trimmed.starts_with("fg ") {
        let id_str = trimmed.strip_prefix("fg").unwrap_or("").trim();
        let id = if id_str.is_empty() { 1 } else { parse_job_id(id_str).unwrap_or(1) };
        if let Some(pid) = jobs.fg(id) {
            use nix::sys::wait::waitpid;
            use nix::unistd::Pid;
            let _ = waitpid(Pid::from_raw(pid as i32), None);
            jobs.mark_done(id);
            Some(0)
        } else {
            eprintln!("fg: job {} not found", id);
            Some(1)
        }
    }

    // ── bg ───────────────────────────────────────────────────────────────────
    else if trimmed == "bg" || trimmed.starts_with("bg ") {
        let id_str = trimmed.strip_prefix("bg").unwrap_or("").trim();
        let id = if id_str.is_empty() { 1 } else { parse_job_id(id_str).unwrap_or(1) };
        if jobs.bg(id) {
            println!("[{}] continued", id);
            Some(0)
        } else {
            eprintln!("bg: job {} not found", id);
            Some(1)
        }
    }

    // ── stop ─────────────────────────────────────────────────────────────────
    else if trimmed == "stop" || trimmed.starts_with("stop ") {
        let id_str = trimmed.strip_prefix("stop").unwrap_or("").trim();
        let id = if id_str.is_empty() { 1 } else { parse_job_id(id_str).unwrap_or(1) };
        if jobs.stop(id) {
            println!("[{}] stopped", id);
            Some(0)
        } else {
            eprintln!("stop: job {} not found", id);
            Some(1)
        }
    }

    // ── kill ─────────────────────────────────────────────────────────────────
    else if trimmed.starts_with("kill ") {
        let args: Vec<&str> = trimmed
        .strip_prefix("kill ")
        .unwrap_or("")
        .split_whitespace()
        .collect();
        if args.is_empty() {
            eprintln!("kill: usage: kill [-SIG] %id|pid");
            return Some(1);
        }
        let mut signal = nix::sys::signal::Signal::SIGTERM;
        let mut id_str = args[0];
        if args[0].starts_with('-') {
            let sig_name = &args[0][1..];
            signal = match sig_name {
                "TERM" | "15" => nix::sys::signal::Signal::SIGTERM,
                "KILL" | "9"  => nix::sys::signal::Signal::SIGKILL,
                "INT"  | "2"  => nix::sys::signal::Signal::SIGINT,
                "STOP" | "19" => nix::sys::signal::Signal::SIGSTOP,
                "CONT" | "18" => nix::sys::signal::Signal::SIGCONT,
                "HUP"  | "1"  => nix::sys::signal::Signal::SIGHUP,
                "USR1" | "10" => nix::sys::signal::Signal::SIGUSR1,
                "USR2" | "12" => nix::sys::signal::Signal::SIGUSR2,
                _ => {
                    eprintln!("kill: unknown signal {}", sig_name);
                    return Some(1);
                }
            };
            id_str = args.get(1).unwrap_or(&"");
        }

        // Obsługa kill PID (bez %) — bezpośrednie wysłanie sygnału
        if !id_str.starts_with('%') {
            if let Ok(pid) = id_str.parse::<i32>() {
                use nix::sys::signal::kill;
                use nix::unistd::Pid;
                match kill(Pid::from_raw(pid), signal) {
                    Ok(_)  => return Some(0),
                    Err(e) => {
                        eprintln!("kill: {}: {}", pid, e);
                        return Some(1);
                    }
                }
            }
        }

        let id = parse_job_id(id_str).unwrap_or(1);
        if jobs.send_signal(id, signal) {
            Some(0)
        } else {
            eprintln!("kill: job {} not found", id);
            Some(1)
        }
    }

    // ── wait ─────────────────────────────────────────────────────────────────
    else if trimmed == "wait" || trimmed.starts_with("wait ") {
        let id_str = trimmed.strip_prefix("wait").unwrap_or("").trim();
        if id_str.is_empty() {
            // wait bez argumentu — czekaj na wszystkie dzieci
            use nix::sys::wait::{waitpid, WaitPidFlag};
            use nix::unistd::Pid;
            loop {
                match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                    Ok(nix::sys::wait::WaitStatus::StillAlive) | Err(_) => break,
                    _ => {}
                }
            }
            return Some(0);
        }
        let id = parse_job_id(id_str).unwrap_or(1);
        if let Some(pid) = jobs.fg(id) {
            use nix::sys::wait::waitpid;
            use nix::unistd::Pid;
            let status = waitpid(Pid::from_raw(pid as i32), None);
            jobs.mark_done(id);
            match status {
                Ok(nix::sys::wait::WaitStatus::Exited(_, code)) => Some(code),
                Ok(nix::sys::wait::WaitStatus::Signaled(_, sig, _)) => {
                    eprintln!("wait: job terminated by signal {}", sig);
                    Some(128 + (sig as i32))
                }
                _ => Some(1),
            }
        } else {
            eprintln!("wait: job {} not found", id);
            Some(127)
        }
    }

    // ── export ───────────────────────────────────────────────────────────────
    else if trimmed == "export" || trimmed.starts_with("export ") {
        let export_str = trimmed.strip_prefix("export").unwrap_or("").trim();
        if export_str.is_empty() {
            // export bez argumentów = lista
            let mut env_vars: Vec<(String, String)> = env::vars().collect();
            env_vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in env_vars {
                println!("export {}=\"{}\"", k, v);
            }
            return Some(0);
        }
        if export_str == "-p" {
            let mut env_vars: Vec<(String, String)> = env::vars().collect();
            env_vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in env_vars {
                println!("export {}=\"{}\"", k, v);
            }
            return Some(0);
        }
        // Obsługa wielu zmiennych: export A=1 B=2
        for part in export_str.split_whitespace() {
            if let Some(eq_pos) = part.find('=') {
                let name  = part[..eq_pos].trim();
                let value = part[eq_pos + 1..].trim().trim_matches('"').trim_matches('\'');
                if vars.restricted_var_blocked(name) {
                    eprintln!("hsh: export: {}: nie można zmienić w trybie restricted", name);
                } else if dry_run {
                    println!("[dry-run] export {}={}", name, value);
                } else {
                    env::set_var(name, value);
                    vars.set(name, value);
                }
            } else {
                // export VARNAME — eksportuj istniejącą zmienną
                if let Some(val) = vars.get(part) {
                    env::set_var(part, &val);
                }
            }
        }
        Some(0)
    }

    // ── local ─────────────────────────────────────────────────────────────────
    // Prawdziwy zakres: wartość widoczna tylko wewnątrz bieżącej funkcji
    // (patrz vars.push_scope/pop_scope wywoływane przy wejściu/wyjściu z funkcji).
    // Poza funkcją zachowuje się jak zwykłe przypisanie.
    else if trimmed.starts_with("local ") {
        let rest = trimmed.strip_prefix("local ").unwrap_or("").trim();
        for part in rest.split_whitespace() {
            if let Some(eq_pos) = part.find('=') {
                let name  = &part[..eq_pos];
                let value = &part[eq_pos + 1..];
                vars.set_local(name, value);
            } else {
                // local bez wartości — ustaw pusty string jeśli nie istnieje
                if vars.get(part).is_none() {
                    vars.set_local(part, "");
                }
            }
        }
        Some(0)
    }

    // ── readonly ──────────────────────────────────────────────────────────────
    else if trimmed.starts_with("readonly ") {
        let rest = trimmed.strip_prefix("readonly ").unwrap_or("").trim();
        for part in rest.split_whitespace() {
            if let Some(eq_pos) = part.find('=') {
                let name  = &part[..eq_pos];
                let value = &part[eq_pos + 1..];
                if vars.restricted_var_blocked(name) {
                    eprintln!("hsh: readonly: {}: nie można zmienić w trybie restricted", name);
                    continue;
                }
                vars.set(name, value);
                env::set_var(name, value);
                vars.mark_readonly(name);
            } else {
                // readonly istniejącej zmiennej — od teraz nie da się jej zmienić/usunąć
                if let Some(val) = vars.get(part) {
                    env::set_var(part, &val);
                }
                vars.mark_readonly(part);
            }
        }
        Some(0)
    }

    // ── declare / typeset ────────────────────────────────────────────────────
    else if trimmed.starts_with("declare ") || trimmed.starts_with("typeset ") {
        let rest = if trimmed.starts_with("declare ") {
            trimmed.strip_prefix("declare ").unwrap_or("")
        } else {
            trimmed.strip_prefix("typeset ").unwrap_or("")
        };
        let rest = rest.trim();

        // Flagi: -x (export), -r (readonly), -i (integer), -a (array), -p (print)
        let mut flag_export  = false;
        let mut flag_print   = false;
        let mut flag_readonly = false;
        let mut parts = rest.split_whitespace().peekable();

        while let Some(part) = parts.peek() {
            if part.starts_with('-') {
                let flags = parts.next().unwrap();
                for c in flags[1..].chars() {
                    match c {
                        'x' => flag_export = true,
                        'p' => flag_print  = true,
                        'r' => flag_readonly = true,
                        'i' | 'a' | 'A' => {} // obsługiwane symbolicznie
                        _ => eprintln!("declare: nieznana flaga -{}", c),
                    }
                }
            } else {
                break;
            }
        }

        if flag_print {
            let mut env_vars: Vec<(String, String)> = env::vars().collect();
            env_vars.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, v) in env_vars {
                println!("declare -x {}=\"{}\"", k, v);
            }
            return Some(0);
        }

        for part in parts {
            if let Some(eq_pos) = part.find('=') {
                let name  = &part[..eq_pos];
                let value = &part[eq_pos + 1..].trim_matches('"').trim_matches('\'');
                if vars.restricted_var_blocked(name) {
                    eprintln!("hsh: declare: {}: nie można zmienić w trybie restricted", name);
                    continue;
                }
                vars.set(name, value);
                if flag_export {
                    env::set_var(name, value);
                }
                if flag_readonly {
                    vars.mark_readonly(name);
                }
            } else {
                if flag_export {
                    if vars.restricted_var_blocked(part) {
                        eprintln!("hsh: declare: {}: nie można zmienić w trybie restricted", part);
                    } else if let Some(val) = vars.get(part) {
                        env::set_var(part, &val);
                    }
                }
                if flag_readonly {
                    vars.mark_readonly(part);
                }
            }
        }
        Some(0)
    }

    // ── unset ─────────────────────────────────────────────────────────────────
    else if trimmed.starts_with("unset ") {
        let names = trimmed.strip_prefix("unset ").unwrap_or("").trim();
        let mut had_error = false;
        for name in names.split_whitespace() {
            if vars.unset(name).is_err() {
                eprintln!("unset: {}: readonly variable", name);
                had_error = true;
            }
        }
        Some(if had_error { 1 } else { 0 })
    }

    // ── trap ──────────────────────────────────────────────────────────────────
    // Obsługiwane sygnały: EXIT (przy zamknięciu powłoki) i INT (Ctrl+C).
    // `trap 'komenda' SYGNAŁ`  — ustaw pułapkę
    // `trap - SYGNAŁ`          — usuń pułapkę
    // `trap`                   — wypisz aktualnie ustawione pułapki
    else if trimmed == "trap" {
        if vars.traps.is_empty() {
            println!("brak ustawionych pułapek (trap)");
        } else {
            let mut sorted: Vec<(&String, &String)> = vars.traps.iter().collect();
            sorted.sort_by_key(|(k, _)| k.as_str());
            for (sig, cmd) in sorted {
                println!("trap -- '{}' {}", cmd, sig);
            }
        }
        Some(0)
    }
    else if trimmed.starts_with("trap ") {
        let rest = trimmed.strip_prefix("trap ").unwrap_or("").trim();
        // Rozdziel na komendę (w cudzysłowie/apostrofie lub bez) i nazwę sygnału (ostatnie słowo)
        let parts = shlex::split(rest).unwrap_or_default();
        if parts.len() < 2 {
            eprintln!("trap: usage: trap 'komenda' SYGNAŁ   |   trap - SYGNAŁ");
            return Some(1);
        }
        let sig = parts.last().unwrap().to_uppercase();
        let sig = sig.trim_start_matches("SIG").to_string();
        if !["EXIT", "INT"].contains(&sig.as_str()) {
            eprintln!("trap: nieobsługiwany sygnał '{}' (hsh obsługuje: EXIT, INT)", sig);
            return Some(1);
        }
        let action = &parts[0];
        if action == "-" || action.is_empty() {
            vars.traps.remove(&sig);
        } else {
            vars.traps.insert(sig, action.clone());
        }
        Some(0)
    }

    // ── alias ─────────────────────────────────────────────────────────────────
    else if trimmed == "alias" {
        let mut merged: HashMap<&str, &str> = HashMap::new();
        for (k, v) in aliases.iter() {
            if !vars.disabled_aliases.contains(k) {
                merged.insert(k.as_str(), v.as_str());
            }
        }
        for (k, v) in vars.runtime_aliases.iter() {
            merged.insert(k.as_str(), v.as_str());
        }
        let mut sorted: Vec<(&&str, &&str)> = merged.iter().collect();
        sorted.sort_by_key(|(k, _)| **k);
        for (k, v) in sorted {
            println!("alias {}='{}'", k, v);
        }
        Some(0)
    }
    else if trimmed.starts_with("alias ") {
        let rest = trimmed.strip_prefix("alias ").unwrap_or("").trim();
        if let Some(eq_pos) = rest.find('=') {
            let name  = rest[..eq_pos].trim();
            let value = rest[eq_pos + 1..].trim().trim_matches('"').trim_matches('\'');
            vars.runtime_aliases.insert(name.to_string(), value.to_string());
            vars.disabled_aliases.remove(name);
            eprintln!(
                "hsh: alias {} ustawiony na czas tej sesji. Aby zachować na stałe, dodaj go do ~/.hshrc [aliases] (np. przez hsh-settings).",
                name
            );
        } else {
            // alias name — wypisz wartość (uwzględniając nadpisania z tej sesji)
            if vars.disabled_aliases.contains(rest) {
                eprintln!("alias: {} nie zdefiniowany", rest);
                return Some(1);
            }
            if let Some(val) = vars.runtime_aliases.get(rest) {
                println!("alias {}='{}'", rest, val);
            } else if let Some(val) = aliases.get(rest) {
                println!("alias {}='{}'", rest, val);
            } else {
                eprintln!("alias: {} nie zdefiniowany", rest);
                return Some(1);
            }
        }
        Some(0)
    }

    // ── unalias ───────────────────────────────────────────────────────────────
    else if trimmed == "unalias -a" {
        vars.runtime_aliases.clear();
        for k in aliases.keys() {
            vars.disabled_aliases.insert(k.clone());
        }
        Some(0)
    }
    else if trimmed.starts_with("unalias ") {
        let name = trimmed.strip_prefix("unalias ").unwrap_or("").trim();
        if name.is_empty() {
            eprintln!("unalias: usage: unalias name");
            return Some(1);
        }
        let existed_runtime = vars.runtime_aliases.remove(name).is_some();
        let existed_config  = aliases.contains_key(name) && !vars.disabled_aliases.contains(name);
        if existed_config {
            vars.disabled_aliases.insert(name.to_string());
        }
        if existed_runtime || existed_config {
            Some(0)
        } else {
            eprintln!("unalias: {}: nie znaleziono", name);
            Some(1)
        }
    }

    // ── set ───────────────────────────────────────────────────────────────────
    else if trimmed == "set" {
        // set bez argumentów — wypisz wszystkie zmienne
        let mut all: Vec<(String, String)> = vars.all().into_iter().collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in all {
            println!("{}={}", k, v);
        }
        Some(0)
    }
    else if trimmed.starts_with("set ") {
        let args: Vec<&str> = trimmed
        .strip_prefix("set ")
        .unwrap_or("")
        .split_whitespace()
        .collect();
        let mut i = 0;
        while i < args.len() {
            match args[i] {
                "-e" | "--errexit"  => vars.set_option("e", true),
                "+e"                => vars.set_option("e", false),
                "-x" | "--xtrace"  => vars.set_option("x", true),
                "+x"                => vars.set_option("x", false),
                "-u" | "--nounset" => vars.set_option("u", true),
                "+u"                => vars.set_option("u", false),
                "-r" | "--restricted" => {
                    vars.set_option("restricted", true);
                    eprintln!("hsh: tryb restricted włączony (nie da się go wyłączyć w tej sesji)");
                }
                "-o" => {
                    if let Some(&opt) = args.get(i + 1) {
                        match opt {
                            "restricted" => {
                                vars.set_option("restricted", true);
                                eprintln!("hsh: tryb restricted włączony (nie da się go wyłączyć w tej sesji)");
                            }
                            "errexit"  => vars.set_option("e", true),
                            "xtrace"   => vars.set_option("x", true),
                            "nounset"  => vars.set_option("u", true),
                            _ => eprintln!("set: nieznana opcja -o {}", opt),
                        }
                        i += 1;
                    }
                }
                "+o" => {
                    if let Some(&opt) = args.get(i + 1) {
                        if opt == "restricted" {
                            eprintln!("set: +o restricted: nie można wyłączyć trybu restricted w tej sesji");
                        } else {
                            match opt {
                                "errexit" => vars.set_option("e", false),
                                "xtrace"  => vars.set_option("x", false),
                                "nounset" => vars.set_option("u", false),
                                _ => eprintln!("set: nieznana opcja +o {}", opt),
                            }
                        }
                        i += 1;
                    }
                }
                other => eprintln!("set: unknown option {}", other),
            }
            i += 1;
        }
        Some(0)
    }

    // ── pushd ─────────────────────────────────────────────────────────────────
    else if trimmed == "pushd" || trimmed.starts_with("pushd ") {
        if vars.restricted {
            eprintln!("hsh: pushd: nie dozwolone w trybie restricted");
            return Some(1);
        }
        let dir_str = trimmed.strip_prefix("pushd").unwrap_or("").trim();
        let target = if dir_str.is_empty() {
            env::var("HOME").unwrap_or_else(|_| "/".to_string())
        } else {
            expand_tilde(dir_str)
        };
        let current = env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
        vars.dir_stack.push(current.clone());
        if env::set_current_dir(&target).is_ok() {
            vars.set_pwd();
            // Wypisz stack
            print!("{}", target);
            for d in vars.dir_stack.iter().rev() {
                print!(" {}", d);
            }
            println!();
            Some(0)
        } else {
            vars.dir_stack.pop();
            eprintln!("pushd: {}: no such directory", target);
            Some(1)
        }
    }

    // ── popd ──────────────────────────────────────────────────────────────────
    else if trimmed == "popd" {
        if vars.restricted {
            eprintln!("hsh: popd: nie dozwolone w trybie restricted");
            return Some(1);
        }
        if let Some(prev) = vars.dir_stack.pop() {
            if env::set_current_dir(&prev).is_ok() {
                vars.set_pwd();
                print!("{}", prev);
                for d in vars.dir_stack.iter().rev() {
                    print!(" {}", d);
                }
                println!();
                Some(0)
            } else {
                eprintln!("popd: cannot change to {}", prev);
                Some(1)
            }
        } else {
            eprintln!("popd: directory stack empty");
            Some(1)
        }
    }

    // ── dirs ──────────────────────────────────────────────────────────────────
    else if trimmed == "dirs" || trimmed.starts_with("dirs ") {
        let current = env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
        print!("{}", current);
        for d in vars.dir_stack.iter().rev() {
            print!(" {}", d);
        }
        println!();
        Some(0)
    }

    // ── source / . ────────────────────────────────────────────────────────────
    else if trimmed.starts_with("source ") || trimmed.starts_with(". ") {
        // obsługiwane w execute.rs przez strip_source_prefix
        None
    }

    // ── true / false ──────────────────────────────────────────────────────────
    else if trimmed == "true" {
        Some(0)
    }
    else if trimmed == "false" {
        Some(1)
    }

    // ── : (no-op) ─────────────────────────────────────────────────────────────
    else if trimmed == ":" || trimmed.starts_with(": ") {
        Some(0)
    }

    // ── echo (builtin fallback) ───────────────────────────────────────────────
    // echo jest obsługiwane przez builtins_native, ale jeśli ktoś wywoła
    // bezpośrednio — przepuść do natywnego

    // ── printf (builtin fallback) ─────────────────────────────────────────────

    // ── read ──────────────────────────────────────────────────────────────────
    else if trimmed == "read" || trimmed.starts_with("read ") {
        let rest = trimmed.strip_prefix("read").unwrap_or("").trim();
        let mut prompt_str = None::<String>;
        let mut varnames: Vec<&str> = Vec::new();
        let mut args = rest.split_whitespace().peekable();

        while let Some(arg) = args.peek() {
            if *arg == "-p" {
                args.next();
                prompt_str = args.next().map(|s| s.to_string());
            } else if *arg == "-r" {
                args.next(); // pomiń -r (raw mode — domyślnie)
            } else if arg.starts_with('-') {
                args.next(); // pomiń nieznane flagi
            } else {
                break;
            }
        }
        varnames.extend(args);

        // Wypisz prompt jeśli podano -p
        if let Some(ref p) = prompt_str {
            use std::io::Write;
            print!("{}", p);
            std::io::stdout().flush().ok();
        }

        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => return Some(1), // EOF
            Ok(_) => {
                let line = line.trim_end_matches('\n').trim_end_matches('\r');
                if varnames.is_empty() {
                    // Czytaj do REPLY
                    vars.set("REPLY", line);
                } else if varnames.len() == 1 {
                    vars.set(varnames[0], line);
                    env::set_var(varnames[0], line);
                } else {
                    // Podziel wg IFS (domyślnie whitespace)
                    let ifs = vars.get("IFS").unwrap_or_else(|| " \t\n".to_string());
                    let sep: Vec<char> = ifs.chars().collect();
                    let mut parts: Vec<&str> = line
                    .splitn(varnames.len(), |c| sep.contains(&c))
                    .collect();
                    // Ostatnia zmienna dostaje resztę
                    while parts.len() < varnames.len() {
                        parts.push("");
                    }
                    for (name, val) in varnames.iter().zip(parts.iter()) {
                        vars.set(name, val.trim());
                        env::set_var(name, val.trim());
                    }
                }
                Some(0)
            }
            Err(_) => Some(1),
        }
    }

    // ── eval ──────────────────────────────────────────────────────────────────
    else if trimmed.starts_with("eval ") {
        // eval jest obsługiwany przez execute.rs przez ponowne wywołanie run_line
        // Tutaj zwracamy None żeby przepuścić dalej
        None
    }

    // ── exec ──────────────────────────────────────────────────────────────────
    else if trimmed.starts_with("exec ") {
        if vars.restricted {
            eprintln!("hsh: exec: nie dozwolone w trybie restricted");
            return Some(1);
        }
        let rest = trimmed.strip_prefix("exec ").unwrap_or("").trim();
        if rest.is_empty() { return Some(0); }
        let parts: Vec<String> = shlex::split(rest).unwrap_or_default();
        if parts.is_empty() { return Some(0); }

        if dry_run {
            println!("[dry-run] exec {}", rest);
            return Some(0);
        }

        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&parts[0])
        .args(&parts[1..])
        .exec();
        eprintln!("hsh: exec: {}: {}", parts[0], err);
        Some(1)
    }

    // ── hsh-help / help ───────────────────────────────────────────────────────
    else if trimmed == "hsh-help" || trimmed == "help" {
        print_help();
        Some(0)
    }

    // ── net-block ─────────────────────────────────────────────────────────────
    // Uruchamia podaną komendę bez dostępu do sieci, wykorzystując systemowe
    // narzędzie `unshare` (pakiet util-linux) do utworzenia nowej przestrzeni
    // nazw sieciowej bez interfejsów. Przydatne np. do uruchamiania nieznanych
    // skryptów/instalatorów bez ryzyka eksfiltracji danych przez sieć.
    // Wymaga obecności `unshare` w PATH — jeśli go brak, odmawia (nie
    // wykonuje komendy "na wszelki wypadek" bez izolacji).
    else if trimmed.starts_with("net-block ") {
        let rest = trimmed.strip_prefix("net-block ").unwrap_or("").trim();
        if rest.is_empty() {
            eprintln!("net-block: usage: net-block <komenda> [argumenty...]");
            return Some(1);
        }
        if which("unshare").is_none() {
            eprintln!(
                "net-block: narzędzie 'unshare' (pakiet util-linux) nie jest dostępne w PATH — nie mogę zagwarantować izolacji sieciowej, więc odmawiam uruchomienia."
            );
            return Some(1);
        }
        let parts: Vec<String> = shlex::split(rest).unwrap_or_default();
        if parts.is_empty() { return Some(1); }
        if dry_run {
            println!("[dry-run] net-block {}", rest);
            return Some(0);
        }
        // --map-root-user pozwala na unshare bez CAP_SYS_ADMIN (user namespaces)
        let status = std::process::Command::new("unshare")
            .arg("--net")
            .arg("--map-root-user")
            .arg("--")
            .args(&parts)
            .status();
        match status {
            Ok(s) => Some(s.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("net-block: nie udało się uruchomić: {}", e);
                Some(1)
            }
        }
    }

    // ── hsh-audit ─────────────────────────────────────────────────────────────
    // Weryfikuje integralność łańcuchowego logu audytowego (~/.hsh-audit.log).
    else if trimmed == "hsh-audit" {
        match crate::security::verify_audit_log() {
            Ok(0) => println!("hsh-audit: brak wpisów w logu audytowym."),
            Ok(n) => println!("hsh-audit: ✓ log nienaruszony — {} wpisów, łańcuch spójny.", n),
            Err(line) => {
                eprintln!(
                    "hsh-audit: ⚠ ŁAŃCUCH PRZERWANY przy wpisie {} — log mógł zostać zmodyfikowany poza hsh!",
                    line
                );
                return Some(1);
            }
        }
        Some(0)
    }

    // ── hsh-version ───────────────────────────────────────────────────────────
    else if trimmed == "hsh-version" {
        println!("hsh 0.4.0 — HackerOS Shell");
        Some(0)
    }

    // ── hsh-reload ────────────────────────────────────────────────────────────
    else if trimmed == "hsh-reload" {
        let home = env::var("HOME").unwrap_or_default();
        let hshrc = format!("{}/.hshrc", home);
        if Path::new(&hshrc).exists() {
            println!("hsh: przeładowuję {} ...", hshrc);
            println!("hsh: użyj 'source ~/.hshrc' żeby załadować aliasy do bieżącej sesji");
        } else {
            eprintln!("hsh: plik {} nie istnieje", hshrc);
            return Some(1);
        }
        Some(0)
    }

    else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────

fn resolve_type(name: &str, aliases: &HashMap<String, String>, vars: &ShellVars) {
    let builtins = [
        "cd", "exit", "history", "which", "type", "jobs", "fg", "bg", "stop",
        "kill", "wait", "export", "alias", "unalias", "set", "pushd", "popd",
        "dirs", "source", "hsh-help", "help", "true", "false", ":", "read",
        "local", "readonly", "declare", "typeset", "unset", "exec", "eval", "trap",
        "hsh-version", "hsh-reload", "hsh-audit", "net-block",
    ];

    if builtins.contains(&name) {
        println!("{} is a shell builtin", name);
        return;
    }

    if vars.disabled_aliases.contains(name) {
        // usunięty w tej sesji — nie pokazuj jako alias, sprawdź dalej (PATH itd.)
    } else if let Some(val) = vars.runtime_aliases.get(name) {
        println!("{} is aliased to '{}'", name, val);
        return;
    } else if let Some(val) = aliases.get(name) {
        println!("{} is aliased to '{}'", name, val);
        return;
    }

    // Sprawdź funkcje (vars nie przechowuje funkcji — sprawdź przez FunctionTable)
    // Tutaj nie mamy dostępu do FunctionTable, więc pomijamy

    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            let full = Path::new(dir).join(name);
            if full.exists() {
                println!("{} is {}", name, full.display());
                return;
            }
        }
    }

    // Sprawdź natywne komendy hsh
    let native_cmds = [
        "echo", "pwd", "ls", "cat", "mkdir", "rm", "cp", "mv", "touch",
        "env", "grep", "head", "tail", "wc", "uname", "find", "xargs", "printf",
    ];
    if native_cmds.contains(&name) {
        println!("{} is a native hsh command", name);
        return;
    }

    println!("{}: not found", name);
}

fn print_help() {
    println!("\x1b[1;32mhsh\x1b[0m — HackerOS Shell v0.4.0");
    println!();
    println!("\x1b[1mBuilt-in commands:\x1b[0m");
    println!("  cd [dir|-]           Change directory (- goes back)");
    println!("  exit [code]          Exit shell");
    println!("  history [query]      Show history; with query: fuzzy search");
    println!("  which/type NAME      Show if alias, builtin, or binary");
    println!("  jobs                 List background jobs");
    println!("  fg [id]              Bring job to foreground");
    println!("  bg [id]              Resume job in background");
    println!("  stop [id]            Suspend job");
    println!("  kill [-SIG] %id|pid  Send signal to job or PID");
    println!("  wait [id]            Wait for job to finish");
    println!("  export [KEY=VAL]     Set/list environment variables");
    println!("  export -p            Print all exported variables");
    println!("  local KEY=VAL        Set local variable");
    println!("  readonly KEY=VAL     Set readonly variable");
    println!("  declare [-xrip]      Declare variables with attributes");
    println!("  unset NAME           Remove variable");
    println!("  read [-p prompt] VAR Read line from stdin");
    println!("  exec CMD             Replace shell with command");
    println!("  alias                List aliases");
    println!("  set [-e] [-x] [-u]   Set shell options (or list all vars)");
    println!("  pushd [dir]          Push directory onto stack");
    println!("  popd                 Pop directory from stack");
    println!("  dirs                 Show directory stack");
    println!("  source FILE          Execute file in current shell");
    println!("  true / false / :     Boolean/no-op builtins");
    println!("  help / hsh-help      Show this help");
    println!("  hsh-version          Show version");
    println!("  hsh-reload           Reload config info");
    println!();
    println!("\x1b[1mNative commands (built into hsh):\x1b[0m");
    println!("  echo  pwd  ls  cat  mkdir  rm  cp  mv  touch  env");
    println!("  grep  head  tail  wc  uname  find  xargs  printf");
    println!();
    println!("\x1b[1mScript features:\x1b[0m");
    println!("  if/elif/else/fi      Conditionals");
    println!("  for/while/until/done Loops (incl. for (( arith )))");
    println!("  case/esac            Pattern matching");
    println!("  function / name()    Function definitions");
    println!("  break / continue     Loop control");
    println!("  return [code]        Return from function");
    println!("  hsh --check FILE     Validate script syntax");
    println!("  hsh FILE.sh [args]   Run script directly");
    println!();
    println!("\x1b[1mFeatures:\x1b[0m");
    println!("  Syntax highlighting with dangerous-command detection");
    println!("  Timestamped history with fuzzy search (~/.hsh-history)");
    println!("  Inline env vars:  FOO=bar command");
    println!("  Glob expansion:   ls *.rs");
    println!("  Background jobs:  command &");
    println!("  Auto ~/.hshrc generation on first run");
    println!("  TUI theme selector: hsh-settings");
    println!("  Dry-run mode:     hsh --dry-run");
    println!("  -c flag:          hsh -c 'command'");
}
