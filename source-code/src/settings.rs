use std::io::{self, Write};
use crate::theme::Theme;

pub fn run_settings() {
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
    println!("  {}Wpisz numer [1–{}] lub Enter żeby anulować:{}", dim, themes.len(), rst);
    print!("  {}❯{} ", "\x1b[38;5;110m", rst);
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let choice: usize = match input.trim().parse() {
        Ok(n) => n,
        Err(_) => { println!("  {}Anulowano.{}", dim, rst); return; }
    };

    if let Some(theme) = themes.into_iter().nth(choice.saturating_sub(1)) {
        if choice >= 1 {
            let name = theme.name.clone();
            theme.save();
            println!();
            println!(
                "  {}✓{} Motyw {}'{}'{} zapisany.",
                "\x1b[38;5;114m", rst,
                bold, name, rst,
            );
            println!("  {}Zmiany są widoczne od razu w kolejnym prompcie.{}", dim, rst);
        } else {
            println!("  {}Anulowano.{}", dim, rst);
        }
    } else {
        println!("  {}Nieprawidłowy numer.{}", "\x1b[38;5;203m", rst);
    }
    println!();
}

fn print_theme_preview(num: usize, t: &Theme) {
    let rst  = "\x1b[0m";
    let dim  = "\x1b[38;5;240m";
    let bold = "\x1b[1m";

    // Symulacja fragmentu promptu
    let time   = format!("{}17:05{}", t.time_color, rst);
    let dir    = format!("{}~/hsh{}", t.dir_color, rst);
    let branch = format!("{} main{}", t.git_color, rst);
    let pc     = format!("{}{}{}", t.prompt_ok, t.prompt_char, rst);
    let cmd_ok = format!("{}git{}", t.cmd_ok_color, rst);
    let flag   = format!("{}-v{}", t.flag_color, rst);
    let hint   = format!("{}  commit -m \"fix\"{}  ← hint", t.hint_color, rst);

    println!(
        "  {}[{}]{} {}{:<10}{}  {}  {}  {}  {} {} {}{}",
        bold, num, rst,
        bold, t.name, rst,
        time, dir, branch,
        pc, cmd_ok, flag, hint
    );
    println!("  {}    {}{}", dim, "─".repeat(60), rst);
}
