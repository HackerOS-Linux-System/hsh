pub fn run_docs(args: &[&str]) {
    let topic = args.first().copied().unwrap_or("");
    match topic {
        "redirections" | "redirect" => docs_redirections(),
        "pipes"        | "pipe"     => docs_pipes(),
        "vars"         | "variables"=> docs_variables(),
        "scripting"    | "script"   => docs_scripting(),
        "hints"                     => docs_hints(),
        "themes"       | "theme"    => docs_themes(),
        "builtins"     | "builtin"  => docs_builtins(),
        "shortcuts"    | "keys"     => docs_shortcuts(),
        _                           => docs_index(),
    }
}

fn header(title: &str) {
    let rst  = "\x1b[0m";
    let bold = "\x1b[1m";
    let line = "\x1b[38;5;240m";
    println!();
    println!("  {}{}{}", bold, title, rst);
    println!("  {}{}{}",  line, "─".repeat(50), rst);
    println!();
}

fn section(title: &str) {
    println!("  \x1b[1;38;5;110m{}\x1b[0m", title);
}

fn code(line: &str) {
    println!("    \x1b[38;5;242m│\x1b[0m \x1b[38;5;150m{}\x1b[0m", line);
}

fn text(line: &str) {
    println!("    {}", line);
}

fn tip(line: &str) {
    println!("  \x1b[38;5;179m💡 {}\x1b[0m", line);
}

// ─────────────────────────────────────────────────────────────────────────────

fn docs_index() {
    header("hsh docs — spis treści");
    println!("  Użycie: \x1b[38;5;110mhsh-docs\x1b[0m \x1b[38;5;179m<temat>\x1b[0m");
    println!();

    let topics = [
        ("redirections", "Przekierowania I/O: > >> < 2>&1 &> <<"),
        ("pipes",        "Natywne pipelines: cmd1 | cmd2 | cmd3"),
        ("vars",         "Zmienne: $VAR ${VAR:-def} $? $$ $# $@"),
        ("scripting",    "Skrypty: if/for/while/case/funkcje"),
        ("hints",        "Podpowiedzi: jak działają i jak je trenować"),
        ("themes",       "Motywy: lista i jak zmieniać"),
        ("builtins",     "Wbudowane komendy: cd exit history..."),
        ("shortcuts",    "Skróty klawiszowe Emacs mode"),
    ];

    for (cmd, desc) in &topics {
        println!(
            "  \x1b[38;5;110m{:<16}\x1b[0m \x1b[38;5;242m{}\x1b[0m",
            cmd, desc
        );
    }
    println!();
    tip("Wszystkie funkcje hsh działają bez potrzeby instalacji sh/bash!");
}

fn docs_redirections() {
    header("hsh docs — przekierowania I/O");
    text("hsh obsługuje przekierowania natywnie przez open() + dup2().");
    text("Nie wymaga sh jako fallbacku.");
    println!();

    section("Standardowe:");
    code("echo foo > plik.txt          # nadpisz plik");
    code("echo foo >> log.txt          # dopisz do pliku");
    code("cat < plik.txt               # stdin z pliku");
    code("cmd 2> err.log               # stderr do pliku");
    code("cmd 2>&1                     # stderr → stdout");
    code("cmd &> wszystko.log          # stdout+stderr do pliku");
    println!();

    section("Heredoc:");
    code("cat << EOF");
    code("  linia 1");
    code("  linia 2");
    code("EOF");
    println!();

    section("W pipeline:");
    code("grep error log.txt | sort | uniq > wyniki.txt");
    println!();

    tip("Heredoc automatycznie rozszerza zmienne wewnątrz (użyj << 'EOF' żeby wyłączyć)");
}

fn docs_pipes() {
    header("hsh docs — pipelines");
    text("Natywne pipelines bez sh — czysty Rust pipe(2) + fork.");
    println!();

    section("Podstawowe:");
    code("ls -la | grep .rs            # filtruj pliki");
    code("cat plik | wc -l             # liczba linii");
    code("ps aux | grep hsh | head -5  # wielostopniowy");
    println!();

    section("Z przekierowaniami:");
    code("cmd1 | cmd2 | cmd3 > wynik.txt");
    code("cmd1 2>&1 | grep error");
    println!();

    section("Natywne komendy w pipeline:");
    code("find . | grep .rs | wc -l    # wszystko natywne");
    println!();

    tip("hsh ma natywne: ls cat grep echo pwd mkdir rm cp mv touch head tail wc env uname");
}

fn docs_variables() {
    header("hsh docs — zmienne");

    section("Ustawianie:");
    code("FOO=bar                      # lokalna zmienna");
    code("export FOO=bar               # eksportuj do środowiska");
    code("FOO=bar komenda              # inline dla jednej komendy");
    println!();

    section("Odczyt:");
    code("echo $FOO");
    code("echo ${FOO}");
    code("echo ${FOO:-domyślna}        # jeśli puste, użyj domyślnej");
    code("echo ${FOO:+alt}             # jeśli ustawione, użyj alt");
    code("echo ${FOO:?błąd}            # jeśli puste, wypisz błąd");
    println!();

    section("Specjalne:");
    code("echo $?   # exit code ostatniej komendy");
    code("echo $$   # PID bieżącego procesu");
    code("echo $0   # nazwa powłoki (hsh)");
    code("echo $#   # liczba argumentów pozycyjnych");
    code("echo $@   # wszystkie argumenty");
    println!();

    section("Arytmetyka:");
    code("echo $((2 + 2))");
    code("echo $((x * y + 1))");
    code("echo $((2 ** 10))            # potęgowanie");
    code("echo $((a == b))             # porównanie: 0 lub 1");
    println!();

    section("Podstawianie komend:");
    code("FILES=$(ls *.rs)");
    code("DATE=`date +%Y-%m-%d`");
    code("echo \"Dzisiaj: $(date)\"");
    println!();

    tip("${VAR:-default} nie modyfikuje zmiennej — tylko zwraca wartość domyślną");
}

fn docs_scripting() {
    header("hsh docs — skryptowanie");
    text("hsh obsługuje wszystkie podstawowe konstrukty bez fallbacku do sh.");
    println!();

    section("if / elif / else / fi:");
    code("if [ -f plik.txt ]; then");
    code("    echo \"plik istnieje\"");
    code("elif [ -d katalog ]; then");
    code("    echo \"to katalog\"");
    code("else");
    code("    echo \"nie istnieje\"");
    code("fi");
    println!();

    section("for:");
    code("for f in *.rs; do");
    code("    echo \"plik: $f\"");
    code("done");
    println!();
    code("for i in 1 2 3 4 5; do");
    code("    echo $i");
    code("done");
    println!();

    section("while:");
    code("while [ $i -lt 10 ]; do");
    code("    echo $i");
    code("    i=$((i + 1))");
    code("done");
    println!();

    section("case:");
    code("case $1 in");
    code("    start) systemctl start usługa ;;");
    code("    stop)  systemctl stop  usługa ;;");
    code("    *)     echo \"nieznana opcja\" ;;");
    code("esac");
    println!();

    section("Funkcje:");
    code("greet() {");
    code("    echo \"Cześć, $1!\"");
    code("}");
    code("greet \"Michał\"");
    println!();

    section("test / [ ]:");
    code("[ -f plik ]   # plik istnieje i jest plikiem");
    code("[ -d katalog ]  # katalog istnieje");
    code("[ -x skrypt ]   # ma prawa wykonania");
    code("[ \"$a\" = \"$b\" ] # równość stringów");
    code("[ $x -gt 5 ]    # porównanie liczb");
    println!();

    section("Łączenie komend:");
    code("cmd1 && cmd2   # cmd2 tylko jeśli cmd1 się powiodło");
    code("cmd1 || cmd2   # cmd2 tylko jeśli cmd1 się nie powiodło");
    code("cmd1 ; cmd2    # zawsze oba");
    println!();

    section("Multiline w REPL:");
    text("  Wpisz 'if' i naciśnij Enter — hsh czeka na 'fi'.");
    text("  Wcięcia są opcjonalne. Ctrl-C anuluje wpisywanie.");
    println!();

    tip("Zapisz skrypt do pliku .sh — hsh automatycznie nada mu prawa wykonania");
}

fn docs_hints() {
    header("hsh docs — system podpowiedzi");
    text("hsh łączy trzy typy podpowiedzi podobne do fish + zsh:");
    println!();

    section("1. Historia inline (jak fish):");
    text("  Wpisz początek komendy → hsh pokazuje dim hint z historii.");
    text("  Naciśnij → (strzałka prawo) lub End żeby zaakceptować.");
    code("git co    →  git co\x1b[38;5;236mmit -m \"fix\"\x1b[0m  ← dim hint");
    println!();

    section("2. Sekwencje (następna komenda):");
    text("  Na pustej linii hsh podpowiada co zazwyczaj robisz po tej komendzie.");
    code("             →  \x1b[38;5;236mgit push\x1b[0m  ← po 'git commit'");
    println!();

    section("3. Tab completion:");
    text("  Pierwsze słowo → lista komend.");
    text("  Po git/cargo/systemctl/apt/docker/npm → subkomendy.");
    text("  Ścieżki → uzupełnianie plików i katalogów.");
    println!();

    section("Trenowanie podpowiedzi:");
    text("  Podpowiedzi uczą się z Twojej historii automatycznie.");
    text("  Im więcej używasz hsh, tym trafniejsze podpowiedzi.");
    text("  Dane przechowywane w ~/.hsh-hints.json");
    println!();

    section("Spellcheck:");
    text("  Jeśli komenda nie istnieje (exit 127), hsh zaproponuje poprawkę:");
    code("  gti status");
    text("  ❓ Czy chodziło Ci o: git status?");
    println!();

    tip("→ (strzałka prawo) akceptuje cały hint. Alt+F akceptuje jedno słowo.");
}

fn docs_themes() {
    header("hsh docs — motywy");
    text("Motywy zapisywane w: ~/.config/hackeros/hsh/theme.json");
    println!();

    section("Zmiana motywu:");
    code("hsh-settings");
    println!();

    section("Dostępne motywy:");
    let themes = [
        ("default",  "Stonowany, miętowo-złoty. Czytelny na każdym tle."),
        ("cosmic",   "Cyberpunk fiolet + cyan. Dla terminali z ciemnym tłem."),
        ("nord",     "Skandynawski błękit. Zimna paleta, bardzo czytelna."),
        ("gruvbox",  "Ciepły retro amber. Inspirowany edytorem Vim."),
        ("dracula",  "Ciemny fiolet + różowy. Klasyczny motyw hacker."),
        ("hackeros", "Matrix zielony. Idealny dla HackerOS."),
    ];
    for (name, desc) in &themes {
        println!(
            "  \x1b[38;5;110m{:<12}\x1b[0m \x1b[38;5;242m{}\x1b[0m",
            name, desc
        );
    }
    println!();

    tip("Motyw działa od razu po wybraniu — nie trzeba restartu.");
}

fn docs_builtins() {
    header("hsh docs — wbudowane komendy");

    let builtins: &[(&str, &str)] = &[
        ("cd [dir|-]",          "Zmień katalog. '-' wraca do poprzedniego."),
        ("exit [code]",         "Wyjdź z hsh z podanym kodem."),
        ("history [query]",     "Historia komend. Query = fuzzy search."),
        ("which NAME",          "Pokaż ścieżkę lub typ komendy."),
        ("type NAME",           "Alias dla which."),
        ("jobs",                "Lista zadań w tle."),
        ("fg [id]",             "Przenieś zadanie na pierwszy plan."),
        ("export KEY=VAL",      "Ustaw zmienną środowiskową."),
        ("source FILE",         "Wykonaj plik w bieżącej powłoce."),
        (". FILE",              "Alias dla source."),
        ("test EXPR",           "Oceń wyrażenie. Zwraca 0 (prawda) lub 1."),
        ("[ EXPR ]",            "Alias dla test."),
        ("hsh-settings",        "Interaktywna zmiana motywu."),
        ("hsh-docs [temat]",    "Ta dokumentacja."),
        ("hsh-help",            "Krótka pomoc."),
    ];

    for (cmd, desc) in builtins {
        println!(
            "  \x1b[38;5;110m{:<24}\x1b[0m \x1b[38;5;242m{}\x1b[0m",
            cmd, desc
        );
    }
    println!();

    section("Natywne komendy (bez /bin):");
    let native = [
        "ls", "cat", "grep", "echo", "pwd", "mkdir", "rm", "cp",
        "mv", "touch", "head", "tail", "wc", "env", "uname",
    ];
    println!("  \x1b[38;5;110m{}\x1b[0m", native.join("  "));
    println!();
    tip("Natywne komendy działają nawet bez zainstalowanych coreutilsów.");
}

fn docs_shortcuts() {
    header("hsh docs — skróty klawiszowe (Emacs mode)");

    let shortcuts: &[(&str, &str)] = &[
        ("→ / End",      "Akceptuj cały hint / uzupełnienie"),
        ("Alt+F",        "Akceptuj jedno słowo hintu"),
        ("Tab",          "Uzupełnij komendę / plik"),
        ("Ctrl+R",       "Szukaj w historii (reverse search)"),
        ("Ctrl+L",       "Wyczyść ekran"),
        ("Ctrl+C",       "Anuluj bieżącą linię"),
        ("Ctrl+D",       "Wyjdź z hsh (EOF)"),
        ("Ctrl+A",       "Skocz na początek linii"),
        ("Ctrl+E",       "Skocz na koniec linii"),
        ("Ctrl+W",       "Usuń słowo wstecz"),
        ("Ctrl+U",       "Usuń od kursora do początku"),
        ("Ctrl+K",       "Usuń od kursora do końca"),
        ("Alt+B",        "Cofnij o jedno słowo"),
        ("Alt+F",        "Przejdź o jedno słowo naprzód"),
        ("↑ / ↓",        "Poprzednia / następna komenda w historii"),
    ];

    for (key, desc) in shortcuts {
        println!(
            "  \x1b[38;5;179m{:<18}\x1b[0m \x1b[38;5;242m{}\x1b[0m",
            key, desc
        );
    }
    println!();
    tip("Tryb Vi można włączyć edytując .hshrc: edit_mode = vi");
}
