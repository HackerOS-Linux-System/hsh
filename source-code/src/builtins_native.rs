use std::env;
use std::fs::{self, metadata, read_dir, File};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Local};

pub fn dispatch_native(cmd: &str, args: &[String]) -> Option<i32> {
    match cmd {
        "echo"  => Some(native_echo(args)),
        "pwd"   => Some(native_pwd()),
        "ls"    => Some(native_ls(args)),
        "cat"   => Some(native_cat(args)),
        "mkdir" => Some(native_mkdir(args)),
        "rm"    => Some(native_rm(args)),
        "cp"    => Some(native_cp(args)),
        "mv"    => Some(native_mv(args)),
        "touch" => Some(native_touch(args)),
        "env"   => Some(native_env(args)),
        "grep"  => Some(native_grep(args)),
        "head"  => Some(native_head(args)),
        "tail"  => Some(native_tail(args)),
        "wc"    => Some(native_wc(args)),
        "true"  => Some(0),
        "false" => Some(1),
        "uname" => Some(native_uname(args)),
        _       => None,
    }
}

// ── echo ────────────────────────────────────────────────────────────────────

fn native_echo(args: &[String]) -> i32 {
    let no_newline = args.first().map(|a| a == "-n").unwrap_or(false);
    let start = if no_newline { 1 } else { 0 };
    let out = args[start..].join(" ");
    // Expand \n \t \\ etc.
    let out = interpret_escapes(&out);
    if no_newline {
        print!("{}", out);
        io::stdout().flush().ok();
    } else {
        println!("{}", out);
    }
    0
}

fn interpret_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n')  => result.push('\n'),
                Some('t')  => result.push('\t'),
                Some('r')  => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some(c)    => { result.push('\\'); result.push(c); }
                None       => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ── pwd ─────────────────────────────────────────────────────────────────────

fn native_pwd() -> i32 {
    match env::current_dir() {
        Ok(p)  => { println!("{}", p.display()); 0 }
        Err(e) => { eprintln!("pwd: {}", e); 1 }
    }
}

// ── ls ──────────────────────────────────────────────────────────────────────

fn native_ls(args: &[String]) -> i32 {
    let mut show_all   = false;
    let mut long_fmt   = false;
    let mut human      = false;
    let mut paths: Vec<&str> = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-a" | "--all"          => show_all = true,
            "-l"                    => long_fmt = true,
            "-h" | "--human-readable" => human = true,
            "-la" | "-al"           => { long_fmt = true; show_all = true; }
            "-lah" | "-alh"         => { long_fmt = true; show_all = true; human = true; }
            _                       => paths.push(arg.as_str()),
        }
    }
    if paths.is_empty() { paths.push("."); }

    let mut code = 0;
    for path in &paths {
        if paths.len() > 1 { println!("{}:", path); }
        code |= ls_dir(path, show_all, long_fmt, human);
    }
    code
}

fn ls_dir(path: &str, show_all: bool, long_fmt: bool, human: bool) -> i32 {
    let dir = match read_dir(path) {
        Ok(d)  => d,
        Err(e) => { eprintln!("ls: {}: {}", path, e); return 1; }
    };

    let mut entries: Vec<_> = dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_all && name.starts_with('.') { continue; }

        if long_fmt {
            let meta = match entry.metadata() {
                Ok(m)  => m,
                Err(e) => { eprintln!("ls: {}: {}", name, e); continue; }
            };
            let perms = format_perms(&meta);
            let size  = if human { human_size(meta.len()) } else { format!("{:>8}", meta.len()) };
            let mtime: DateTime<Local> = meta.modified()
            .map(DateTime::from)
            .unwrap_or_else(|_| Local::now());
            let color = entry_color(&entry.path(), &meta);
            println!(
                "{}  {:>8}  {}  {}{}\x1b[0m",
                perms, size,
                mtime.format("%b %d %H:%M"),
                     color, name
            );
        } else {
            let meta = entry.metadata().ok();
            let color = meta
            .as_ref()
            .map(|m| entry_color(&entry.path(), m))
            .unwrap_or("");
            print!("{}{}\x1b[0m  ", color, name);
        }
    }
    if !long_fmt { println!(); }
    0
}

fn format_perms(meta: &std::fs::Metadata) -> String {
    let mode = meta.permissions().mode();
    let file_type = if meta.is_dir() { 'd' } else if meta.file_type().is_symlink() { 'l' } else { '-' };
    let bits = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    let perm_str: String = bits.iter().map(|(bit, ch)| if mode & bit != 0 { *ch } else { '-' }).collect();
    format!("{}{}", file_type, perm_str)
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "K", "M", "G", "T"];
    let mut size = bytes as f64;
    let mut i = 0;
    while size >= 1024.0 && i < UNITS.len() - 1 { size /= 1024.0; i += 1; }
    if i == 0 { format!("{:>4}{}", bytes, UNITS[0]) }
    else       { format!("{:>4.1}{}", size, UNITS[i]) }
}

fn entry_color(path: &Path, meta: &std::fs::Metadata) -> &'static str {
    if meta.is_dir() { "\x1b[1;34m" }
    else if meta.file_type().is_symlink() { "\x1b[1;36m" }
    else if meta.permissions().mode() & 0o111 != 0 { "\x1b[1;32m" }
    else if path.extension().map_or(false, |e| matches!(e.to_str(), Some("gz"|"tar"|"zip"|"bz2"|"xz"|"zst"))) { "\x1b[1;31m" }
    else { "" }
}

// ── cat ─────────────────────────────────────────────────────────────────────

fn native_cat(args: &[String]) -> i32 {
    if args.is_empty() {
        // Read from stdin
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => println!("{}", l),
                Err(e) => { eprintln!("cat: {}", e); return 1; }
            }
        }
        return 0;
    }
    let mut code = 0;
    let mut number = false;
    let mut files: Vec<&str> = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-n" => number = true,
            _    => files.push(arg.as_str()),
        }
    }
    for file in &files {
        match File::open(file) {
            Err(e) => { eprintln!("cat: {}: {}", file, e); code = 1; }
            Ok(f)  => {
                let reader = BufReader::new(f);
                for (i, line) in reader.lines().enumerate() {
                    match line {
                        Ok(l)  => if number { println!("{:>6}  {}", i + 1, l); } else { println!("{}", l); }
                        Err(e) => { eprintln!("cat: {}: {}", file, e); code = 1; break; }
                    }
                }
            }
        }
    }
    code
}

// ── mkdir ────────────────────────────────────────────────────────────────────

fn native_mkdir(args: &[String]) -> i32 {
    let parents = args.iter().any(|a| a == "-p" || a == "--parents");
    let mut code = 0;
    for arg in args.iter().filter(|a| !a.starts_with('-')) {
        let result = if parents {
            fs::create_dir_all(arg)
        } else {
            fs::create_dir(arg)
        };
        if let Err(e) = result {
            eprintln!("mkdir: {}: {}", arg, e);
            code = 1;
        }
    }
    code
}

// ── rm ───────────────────────────────────────────────────────────────────────

fn native_rm(args: &[String]) -> i32 {
    let recursive = args.iter().any(|a| a == "-r" || a == "-rf" || a == "-R");
    let force     = args.iter().any(|a| a == "-f" || a == "-rf");
    let mut code = 0;
    for arg in args.iter().filter(|a| !a.starts_with('-')) {
        let p = Path::new(arg);
        let result = if p.is_dir() && recursive {
            fs::remove_dir_all(p)
        } else if p.is_dir() {
            fs::remove_dir(p)
        } else {
            fs::remove_file(p)
        };
        if let Err(e) = result {
            if !force { eprintln!("rm: {}: {}", arg, e); code = 1; }
        }
    }
    code
}

// ── cp ───────────────────────────────────────────────────────────────────────

fn native_cp(args: &[String]) -> i32 {
    let recursive = args.iter().any(|a| a == "-r" || a == "-R" || a == "-rf");
    let files: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_str()).collect();
    if files.len() < 2 { eprintln!("cp: missing destination"); return 1; }
    let (srcs, dst) = files.split_at(files.len() - 1);
    let dst = Path::new(dst[0]);
    let mut code = 0;
    for src in srcs {
        let src_path = Path::new(src);
        let target = if dst.is_dir() {
            dst.join(src_path.file_name().unwrap_or_default())
        } else {
            dst.to_path_buf()
        };
        let result = if src_path.is_dir() && recursive {
            copy_dir_all(src_path, &target)
        } else {
            fs::copy(src_path, &target).map(|_| ())
        };
        if let Err(e) = result { eprintln!("cp: {}: {}", src, e); code = 1; }
    }
    code
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in read_dir(src)?.flatten() {
        let t = dst.join(entry.file_name());
        if entry.path().is_dir() { copy_dir_all(&entry.path(), &t)?; }
        else { fs::copy(entry.path(), t)?; }
    }
    Ok(())
}

// ── mv ───────────────────────────────────────────────────────────────────────

fn native_mv(args: &[String]) -> i32 {
    let files: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_str()).collect();
    if files.len() < 2 { eprintln!("mv: missing destination"); return 1; }
    let (srcs, dst) = files.split_at(files.len() - 1);
    let dst = Path::new(dst[0]);
    let mut code = 0;
    for src in srcs {
        let src_path = Path::new(src);
        let target = if dst.is_dir() {
            dst.join(src_path.file_name().unwrap_or_default())
        } else {
            dst.to_path_buf()
        };
        if let Err(e) = fs::rename(src_path, &target) {
            // cross-device: copy + remove
            if e.kind() == io::ErrorKind::CrossesDevices || e.raw_os_error() == Some(18) {
                if let Err(e2) = fs::copy(src_path, &target).and_then(|_| fs::remove_file(src_path)) {
                    eprintln!("mv: {}: {}", src, e2); code = 1;
                }
            } else {
                eprintln!("mv: {}: {}", src, e); code = 1;
            }
        }
    }
    code
}

// ── touch ────────────────────────────────────────────────────────────────────

fn native_touch(args: &[String]) -> i32 {
    let mut code = 0;
    for arg in args.iter().filter(|a| !a.starts_with('-')) {
        let result = if Path::new(arg).exists() {
            // Update mtime via File::open + set_modified (or just re-write empty content)
            File::options().write(true).open(arg).map(|_| ())
        } else {
            File::create(arg).map(|_| ())
        };
        if let Err(e) = result { eprintln!("touch: {}: {}", arg, e); code = 1; }
    }
    code
}

// ── env ──────────────────────────────────────────────────────────────────────

fn native_env(args: &[String]) -> i32 {
    if args.is_empty() {
        let mut pairs: Vec<(String, String)> = env::vars().collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in pairs { println!("{}={}", k, v); }
    } else {
        // env VAR=val command
        let eq_end = args.iter().position(|a| !a.contains('=')).unwrap_or(args.len());
        let env_pairs = &args[..eq_end];
        let cmd_args  = &args[eq_end..];
        if cmd_args.is_empty() {
            for pair in env_pairs { println!("{}", pair); }
            return 0;
        }
        use std::process::Command;
        let mut cmd = Command::new(&cmd_args[0]);
        cmd.args(&cmd_args[1..]);
        for pair in env_pairs {
            if let Some(eq) = pair.find('=') {
                cmd.env(&pair[..eq], &pair[eq+1..]);
            }
        }
        return cmd.status().map(|s| s.code().unwrap_or(1)).unwrap_or(1);
    }
    0
}

// ── grep ─────────────────────────────────────────────────────────────────────

fn native_grep(args: &[String]) -> i32 {
    let mut ignore_case    = false;
    let mut invert         = false;
    let mut count_only     = false;
    let mut line_numbers   = false;
    let mut pattern        = None::<String>;
    let mut files: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-i" | "--ignore-case" => ignore_case = true,
            "-v" | "--invert-match"=> invert = true,
            "-c" | "--count"       => count_only = true,
            "-n" | "--line-number" => line_numbers = true,
            _ => {
                if pattern.is_none() { pattern = Some(args[i].clone()); }
                else { files.push(args[i].clone()); }
            }
        }
        i += 1;
    }

    let pattern = match pattern {
        Some(p) => p,
        None    => { eprintln!("grep: no pattern"); return 2; }
    };

    let pat = if ignore_case { pattern.to_lowercase() } else { pattern.clone() };

    let mut matched_any = false;

    let process_lines = |reader: &mut dyn BufRead, prefix: &str, code: &mut i32| {
        let mut match_count = 0u64;
        for (lineno, line) in reader.lines().enumerate() {
            let line = match line { Ok(l) => l, Err(_) => break };
            let haystack = if ignore_case { line.to_lowercase() } else { line.clone() };
            let matches = haystack.contains(&pat);
            let show = if invert { !matches } else { matches };
            if show {
                if count_only { match_count += 1; }
                else {
                    let out = if line_numbers {
                        format!("{}{}:{}", prefix, lineno + 1, line)
                    } else {
                        format!("{}{}", prefix, line)
                    };
                    // Highlight match in red
                    println!("{}", highlight_match(&out, &pattern, ignore_case));
                }
            }
        }
        if count_only { println!("{}{}", prefix, match_count); }
        if match_count > 0 || (!count_only) { *code = 0; }
    };

    let mut code = 1;

    if files.is_empty() {
        let stdin = io::stdin();
        process_lines(&mut stdin.lock(), "", &mut code);
    } else {
        let show_prefix = files.len() > 1;
        for file in &files {
            match File::open(file) {
                Err(e) => { eprintln!("grep: {}: {}", file, e); code = 2; }
                Ok(f)  => {
                    let prefix = if show_prefix { format!("{}:", file) } else { String::new() };
                    process_lines(&mut BufReader::new(f), &prefix, &mut code);
                }
            }
        }
    }
    code
}

fn highlight_match(line: &str, pattern: &str, ignore_case: bool) -> String {
    let lower_line = if ignore_case { line.to_lowercase() } else { line.to_string() };
    let lower_pat  = if ignore_case { pattern.to_lowercase() } else { pattern.to_string() };
    if let Some(pos) = lower_line.find(&lower_pat) {
        format!(
            "{}\x1b[1;31m{}\x1b[0m{}",
            &line[..pos],
            &line[pos..pos + pattern.len()],
                &line[pos + pattern.len()..]
        )
    } else {
        line.to_string()
    }
}

// ── head / tail ───────────────────────────────────────────────────────────────

fn native_head(args: &[String]) -> i32 {
    let n = parse_n_arg(args, 10);
    let files: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_str()).collect();
    read_lines_limited(&files, n, false)
}

fn native_tail(args: &[String]) -> i32 {
    let n = parse_n_arg(args, 10);
    let files: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_str()).collect();
    read_lines_limited(&files, n, true)
}

fn parse_n_arg(args: &[String], default: usize) -> usize {
    for i in 0..args.len() {
        if args[i] == "-n" {
            return args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(default);
        }
        if let Some(stripped) = args[i].strip_prefix("-n") {
            if let Ok(n) = stripped.parse() { return n; }
        }
        // -5
        if args[i].starts_with('-') {
            if let Ok(n) = args[i][1..].parse::<usize>() { return n; }
        }
    }
    default
}

fn read_lines_limited(files: &[&str], n: usize, tail: bool) -> i32 {
    let mut code = 0;
    let read_file = |path: &str| -> io::Result<Vec<String>> {
        let f = File::open(path)?;
        BufReader::new(f).lines().collect::<io::Result<Vec<_>>>()
    };
    let process = |lines: Vec<String>| {
        let to_show = if tail {
            let start = lines.len().saturating_sub(n);
            &lines[start..]
        } else {
            &lines[..n.min(lines.len())]
        };
        for l in to_show { println!("{}", l); }
    };
    if files.is_empty() {
        let stdin = io::stdin();
        let lines: Vec<String> = stdin.lock().lines().filter_map(|l| l.ok()).collect();
        process(lines);
    } else {
        for f in files {
            match read_file(f) {
                Ok(lines) => { if files.len() > 1 { println!("==> {} <==", f); } process(lines); }
                Err(e)    => { eprintln!("{}: {}", f, e); code = 1; }
            }
        }
    }
    code
}

// ── wc ───────────────────────────────────────────────────────────────────────

fn native_wc(args: &[String]) -> i32 {
    let do_lines = args.iter().any(|a| a == "-l");
    let do_words = args.iter().any(|a| a == "-w");
    let do_chars = args.iter().any(|a| a == "-c");
    let all = !do_lines && !do_words && !do_chars;

    let files: Vec<&str> = args.iter().filter(|a| !a.starts_with('-')).map(|s| s.as_str()).collect();
    let mut code = 0;

    let count = |content: &str| -> (u64, u64, u64) {
        let lines = content.lines().count() as u64;
        let words = content.split_whitespace().count() as u64;
        let chars = content.len() as u64;
        (lines, words, chars)
    };
    let print_counts = |lines: u64, words: u64, chars: u64, name: &str| {
        let mut parts = Vec::new();
        if all || do_lines { parts.push(format!("{:>8}", lines)); }
        if all || do_words { parts.push(format!("{:>8}", words)); }
        if all || do_chars { parts.push(format!("{:>8}", chars)); }
        parts.push(format!(" {}", name));
        println!("{}", parts.join(""));
    };

    if files.is_empty() {
        let mut content = String::new();
        io::stdin().lock().lines().for_each(|l| { if let Ok(l) = l { content.push_str(&l); content.push('\n'); }});
        let (l, w, c) = count(&content);
        print_counts(l, w, c, "");
    } else {
        for f in &files {
            match fs::read_to_string(f) {
                Ok(s)  => { let (l, w, c) = count(&s); print_counts(l, w, c, f); }
                Err(e) => { eprintln!("wc: {}: {}", f, e); code = 1; }
            }
        }
    }
    code
}

// ── uname ────────────────────────────────────────────────────────────────────

fn native_uname(args: &[String]) -> i32 {
    let all = args.iter().any(|a| a == "-a");
    let kernel = args.iter().any(|a| a == "-s") || (!all && args.is_empty());
    let nodename = args.iter().any(|a| a == "-n");
    let release = args.iter().any(|a| a == "-r");
    let machine = args.iter().any(|a| a == "-m");

    let mut parts = Vec::new();
    if all || kernel  { parts.push("Linux"); }
    if all || nodename {
        let hostname = fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".to_string());
        parts.push(Box::leak(hostname.trim().to_string().into_boxed_str()));
    }
    if all || release {
        let release = fs::read_to_string("/proc/version")
        .unwrap_or_default()
        .split_whitespace()
        .nth(2)
        .unwrap_or("unknown")
        .to_string();
        parts.push(Box::leak(release.into_boxed_str()));
    }
    if all || machine {
        // Read from /proc/cpuinfo or use nix
        let arch = if cfg!(target_arch = "x86_64")  { "x86_64" }
        else if cfg!(target_arch = "aarch64") { "aarch64" }
        else { "unknown" };
        parts.push(arch);
    }
    println!("{}", parts.join(" "));
    0
}
