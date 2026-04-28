use std::io::{self, Write};
use crate::theme::Theme;

// ─────────────────────────────────────────────────────────────────────────────
// TUI Settings — interaktywny wybór motywu w stylu menu
// ─────────────────────────────────────────────────────────────────────────────

/// Główna funkcja ustawień — wyświetla TUI i obsługuje wybór.
pub fn run_settings() {
    // Próba TUI; jeśli terminal nie obsługuje raw mode — fallback do prostego menu
    match run_tui_settings() {
        Ok(_)  => {}
        Err(_) => run_simple_settings(),
    }
}

fn run_tui_settings() -> io::Result<()> {
    use std::io::Read;

    let themes = Theme::all_themes();
    let current = Theme::load();
    let mut selected = themes
        .iter()
        .position(|t| t.name == current.name)
        .unwrap_or(0);

    // Wejdź w raw mode przez bezpośrednie wywołanie termios
    enable_raw_mode()?;

    let result = tui_loop(&themes, &mut selected);

    // Zawsze wróć do normalnego trybu
    disable_raw_mode()?;

    // Przesuń kursor za menu
    let _ = write!(io::stdout(), "\r\n");
    io::stdout().flush()?;

    match result {
        TuiResult::Selected(idx) => {
            if let Some(theme) = themes.into_iter().nth(idx) {
                let name = theme.name.clone();
                theme.save();
                println!(
                    "\x1b[38;5;114m✓\x1b[0m Motyw \x1b[1m'{}'\x1b[0m zapisany.",
                    name
                );
            }
        }
        TuiResult::Cancelled => {
            println!("\x1b[38;5;242mAnulowano.\x1b[0m");
        }
    }

    Ok(())
}

enum TuiResult {
    Selected(usize),
    Cancelled,
}

fn tui_loop(themes: &[Theme], selected: &mut usize) -> TuiResult {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    let page_size = 10usize;
    let mut page_start = (*selected / page_size) * page_size;

    loop {
        render_tui(&mut out, themes, *selected, page_start, page_size);
        out.flush().ok();

        match read_key() {
            Key::Up => {
                if *selected > 0 {
                    *selected -= 1;
                    if *selected < page_start {
                        page_start = page_start.saturating_sub(page_size);
                    }
                }
            }
            Key::Down => {
                if *selected + 1 < themes.len() {
                    *selected += 1;
                    if *selected >= page_start + page_size {
                        page_start += page_size;
                    }
                }
            }
            Key::PageUp => {
                *selected = selected.saturating_sub(page_size);
                page_start = (*selected / page_size) * page_size;
            }
            Key::PageDown => {
                *selected = (*selected + page_size).min(themes.len().saturating_sub(1));
                page_start = (*selected / page_size) * page_size;
            }
            Key::Home => {
                *selected = 0;
                page_start = 0;
            }
            Key::End => {
                *selected = themes.len().saturating_sub(1);
                page_start = (*selected / page_size) * page_size;
            }
            Key::Enter => {
                // Wyczyść TUI przed powrotem
                clear_tui(&mut out, page_size.min(themes.len() - page_start) + 6);
                out.flush().ok();
                return TuiResult::Selected(*selected);
            }
            Key::Escape | Key::Char('q') | Key::Ctrl('c') => {
                clear_tui(&mut out, page_size.min(themes.len() - page_start) + 6);
                out.flush().ok();
                return TuiResult::Cancelled;
            }
            Key::Char(c) if c.is_ascii_digit() => {
                let digit = c as usize - '0' as usize;
                if digit >= 1 && digit <= themes.len().min(9) {
                    *selected = digit - 1;
                    clear_tui(&mut out, page_size.min(themes.len() - page_start) + 6);
                    out.flush().ok();
                    return TuiResult::Selected(*selected);
                }
            }
            _ => {}
        }
    }
}

fn render_tui<W: Write>(
    out:        &mut W,
    themes:     &[Theme],
    selected:   usize,
    page_start: usize,
    page_size:  usize,
) {
    let rst  = "\x1b[0m";
    let bold = "\x1b[1m";
    let dim  = "\x1b[38;5;240m";
    let acc  = "\x1b[38;5;110m";
    let hi   = "\x1b[48;5;235m\x1b[38;5;255m";  // podświetlenie wybranego
    let page_end = (page_start + page_size).min(themes.len());
    let total_lines = (page_end - page_start) + 6;

    // Przesuń kursor na górę (wyczyść poprzednie renderowanie)
    write!(out, "\x1b[{}A\r", total_lines).ok();

    // Nagłówek
    write!(
        out,
        "\r  {}{}hsh › Ustawienia — Wybór motywu{}\r\n",
        bold, acc, rst
    ).ok();
    write!(
        out,
        "  {}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{}\r\n",
        dim, rst
    ).ok();
    write!(
        out,
        "  {}↑↓ nawigacja  Enter wybierz  q anuluj  PgUp/PgDn strona{}\r\n",
        "\x1b[38;5;244m", rst
    ).ok();
    write!(out, "\r\n").ok();

    // Lista motywów
    for i in page_start..page_end {
        let t = &themes[i];
        let is_selected = i == selected;

        // Symulacja promptu z kolorami motywu
        let time_seg   = format!("{}17:05{}", t.time_color, rst);
        let dir_seg    = format!("{}~/projekt{}", t.dir_color, rst);
        let git_seg    = format!("{} main{}", t.git_color, rst);
        let pc_seg     = format!("{}{}{}", t.prompt_ok, t.prompt_char, rst);
        let cmd_seg    = format!("{}cargo{}", t.cmd_ok_color, rst);
        let flag_seg   = format!("{}build{}", t.flag_color, rst);
        let hint_seg   = format!("{} --release{}", t.hint_color, rst);

        let preview = format!(
            "{}  {}  {}  {} {} {}{}",
            time_seg, dir_seg, git_seg, pc_seg, cmd_seg, flag_seg, hint_seg
        );

        if is_selected {
            write!(
                out,
                "  {}{} ❯ {:>2}. {:<18} │ {}{}\r\n",
                hi, bold,
                i + 1,
                t.name,
                preview,
                rst
            ).ok();
        } else {
            write!(
                out,
                "  {}  {:>2}. {:<18} {} │ {}{}\r\n",
                dim,
                i + 1,
                t.name,
                rst,
                preview,
                rst
            ).ok();
        }
    }

    // Stopka z informacją o stronie
    write!(out, "\r\n").ok();
    write!(
        out,
        "  {}Strona {}/{} · Motyw {}/{}{}\r\n",
        dim,
        page_start / page_size + 1,
        (themes.len() + page_size - 1) / page_size,
        selected + 1,
        themes.len(),
        rst
    ).ok();
}

fn clear_tui<W: Write>(out: &mut W, lines: usize) {
    // Przesuń do góry i wyczyść
    write!(out, "\x1b[{}A\r", lines).ok();
    for _ in 0..lines {
        write!(out, "\x1b[2K\r\n").ok();
    }
    write!(out, "\x1b[{}A\r", lines).ok();
}

// ─────────────────────────────────────────────────────────────────────────────
// Fallback — proste menu bez raw mode
// ─────────────────────────────────────────────────────────────────────────────

fn run_simple_settings() {
    let rst  = "\x1b[0m";
    let bold = "\x1b[1m";
    let dim  = "\x1b[38;5;240m";

    println!();
    println!("  {}{}hsh › ustawienia{}", bold, "\x1b[38;5;110m", rst);
    println!("  {}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━{}", dim, rst);
    println!();
    println!("  {}Dostępne motywy:{}", bold, rst);
    println!();

    let themes = Theme::all_themes();
    for (i, t) in themes.iter().enumerate() {
        print_theme_preview(i + 1, t);
    }

    println!();
    println!(
        "  {}Wpisz numer [1–{}] lub Enter żeby anulować:{}",
        dim,
        themes.len(),
        rst
    );
    print!("  {}❯{} ", "\x1b[38;5;110m", rst);
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let choice: usize = match input.trim().parse() {
        Ok(n) if n >= 1 && n <= themes.len() => n,
        _ => {
            println!("  {}Anulowano.{}", dim, rst);
            return;
        }
    };

    if let Some(theme) = themes.into_iter().nth(choice - 1) {
        let name = theme.name.clone();
        theme.save();
        println!();
        println!(
            "  {}✓{} Motyw {}'{}'{} zapisany.",
            "\x1b[38;5;114m", rst, bold, name, rst,
        );
        println!(
            "  {}Zmiany są widoczne od razu w kolejnym prompcie.{}",
            dim, rst
        );
    }
    println!();
}

fn print_theme_preview(num: usize, t: &Theme) {
    let rst  = "\x1b[0m";
    let dim  = "\x1b[38;5;240m";
    let bold = "\x1b[1m";

    let time   = format!("{}17:05{}", t.time_color, rst);
    let dir    = format!("{}~/hsh{}", t.dir_color, rst);
    let branch = format!("{} main{}", t.git_color, rst);
    let pc     = format!("{}{}{}", t.prompt_ok, t.prompt_char, rst);
    let cmd_ok = format!("{}git{}", t.cmd_ok_color, rst);
    let flag   = format!("{}-v{}", t.flag_color, rst);
    let hint   = format!("{}  commit -m \"fix\"{}  ← hint", t.hint_color, rst);

    println!(
        "  {}[{:>2}]{} {}{:<18}{}  {}  {}  {}  {} {} {}{}",
        bold, num, rst,
        bold, t.name, rst,
        time, dir, branch,
        pc, cmd_ok, flag, hint
    );
    println!("  {}    {}{}", dim, "─".repeat(60), rst);
}

// ─────────────────────────────────────────────────────────────────────────────
// Raw mode (bez crossterm — bezpośrednio przez libc/termios)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn enable_raw_mode() -> io::Result<()> {
    use libc::{tcgetattr, tcsetattr, TCSANOW, ICANON, ECHO, STDIN_FILENO};
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if tcgetattr(STDIN_FILENO, &mut t) != 0 {
            return Err(io::Error::last_os_error());
        }
        t.c_lflag &= !(ICANON | ECHO);
        t.c_cc[libc::VMIN]  = 1;
        t.c_cc[libc::VTIME] = 0;
        if tcsetattr(STDIN_FILENO, TCSANOW, &t) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // Ukryj kursor
    write!(io::stdout(), "\x1b[?25l")?;
    // Zainicjuj pustą przestrzeń dla renderowania
    let page_size = 16; // max tematów na stronie + nagłówek
    for _ in 0..page_size {
        writeln!(io::stdout())?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn enable_raw_mode() -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "raw mode not supported"))
}

#[cfg(unix)]
fn disable_raw_mode() -> io::Result<()> {
    use libc::{tcgetattr, tcsetattr, TCSANOW, ICANON, ECHO, STDIN_FILENO};
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if tcgetattr(STDIN_FILENO, &mut t) != 0 {
            return Err(io::Error::last_os_error());
        }
        t.c_lflag |= ICANON | ECHO;
        if tcsetattr(STDIN_FILENO, TCSANOW, &t) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    // Pokaż kursor
    write!(io::stdout(), "\x1b[?25h")?;
    io::stdout().flush()?;
    Ok(())
}

#[cfg(not(unix))]
fn disable_raw_mode() -> io::Result<()> {
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Odczyt klawiszy
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Escape,
    Char(char),
    Ctrl(char),
    Unknown,
}

fn read_key() -> Key {
    let mut buf = [0u8; 8];
    let n = unsafe {
        libc::read(libc::STDIN_FILENO, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
    };
    if n <= 0 { return Key::Unknown; }

    match buf[0] {
        b'\r' | b'\n' => Key::Enter,
        b'\x1b' => {
            if n == 1 { return Key::Escape; }
            match buf[1] {
                b'[' => match buf[2] {
                    b'A' => Key::Up,
                    b'B' => Key::Down,
                    b'H' => Key::Home,
                    b'F' => Key::End,
                    b'5' if n > 3 && buf[3] == b'~' => Key::PageUp,
                    b'6' if n > 3 && buf[3] == b'~' => Key::PageDown,
                    b'1' if n > 3 && buf[3] == b'~' => Key::Home,
                    b'4' if n > 3 && buf[3] == b'~' => Key::End,
                    _ => Key::Unknown,
                },
                _ => Key::Escape,
            }
        }
        b'\x03' => Key::Ctrl('c'),
        b'\x04' => Key::Ctrl('d'),
        c if c >= 0x20 && c < 0x7f => Key::Char(c as char),
        b'q' => Key::Char('q'),
        _ => Key::Unknown,
    }
}
