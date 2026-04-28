use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ─────────────────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Node {
    Command(String),
    If {
        condition:      Box<Node>,
        then_body:      Vec<Node>,
        elif_branches:  Vec<(Box<Node>, Vec<Node>)>,
        else_body:      Option<Vec<Node>>,
    },
    While {
        condition: Box<Node>,
        body:      Vec<Node>,
    },
    Until {
        condition: Box<Node>,
        body:      Vec<Node>,
    },
    For {
        var:   String,
        items: Vec<String>,
        body:  Vec<Node>,
    },
    ForArith {
        init:      String,
        condition: String,
        update:    String,
        body:      Vec<Node>,
    },
    Case {
        word: String,
        arms: Vec<(Vec<String>, Vec<Node>)>,
    },
    FunctionDef {
        name: String,
        body: Vec<Node>,
    },
    Sequence(Vec<Node>),
    /// break / continue w pętlach
    Break,
    Continue,
    /// return z funkcji
    Return(Option<i32>),
    /// Podstawianie zmiennej: VAR=wartość
    Assign {
        name:  String,
        value: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Shell functions registry
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
pub struct FunctionTable {
    functions: HashMap<String, Vec<Node>>,
}

impl FunctionTable {
    pub fn new() -> Self { Self::default() }

    pub fn define(&mut self, name: &str, body: Vec<Node>) {
        self.functions.insert(name.to_string(), body);
    }

    pub fn get(&self, name: &str) -> Option<&Vec<Node>> {
        self.functions.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<String>,
    pos:    usize,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        Parser {
            tokens: tokenize_script(input),
            pos:    0,
        }
    }

    pub fn parse(&mut self) -> Vec<Node> {
        self.parse_list()
    }

    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.pos).map(|s| s.as_str())
    }

    fn peek2(&self) -> Option<&str> {
        self.tokens.get(self.pos + 1).map(|s| s.as_str())
    }

    fn next(&mut self) -> Option<&str> {
        let t = self.tokens.get(self.pos).map(|s| s.as_str());
        self.pos += 1;
        t
    }

    fn expect(&mut self, word: &str) {
        match self.peek() {
            Some(w) if w == word => { self.pos += 1; }
            other => eprintln!("hsh: expected '{}', got {:?}", word, other),
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some("\n") | Some(";")) {
            self.pos += 1;
        }
    }

    fn parse_list(&mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        self.skip_newlines();

        while let Some(tok) = self.peek() {
            match tok {
                "fi" | "done" | "esac" | "else" | "elif" | "then" | "}" | ";;" => break,
                _ => {
                    if let Some(node) = self.parse_statement() {
                        nodes.push(node);
                    }
                    self.skip_newlines();
                }
            }
        }
        nodes
    }

    fn parse_statement(&mut self) -> Option<Node> {
        self.skip_newlines();
        let tok = self.peek()?;

        match tok {
            "if"       => Some(self.parse_if()),
            "while"    => Some(self.parse_while()),
            "until"    => Some(self.parse_until()),
            "for"      => Some(self.parse_for()),
            "case"     => Some(self.parse_case()),
            "break"    => { self.pos += 1; Some(Node::Break) }
            "continue" => { self.pos += 1; Some(Node::Continue) }
            "return"   => {
                self.pos += 1;
                let code = self.peek()
                    .and_then(|t| if t == "\n" || t == ";" { None } else { t.parse::<i32>().ok() });
                if code.is_some() { self.pos += 1; }
                Some(Node::Return(code))
            }
            _ => {
                // Sprawdź definicję funkcji: name() { ... }
                // Obsługa: name() { }, name () { }, function name { }
                let is_func_kw = tok == "function";
                let is_func_paren = {
                    let next = self.tokens.get(self.pos + 1).map(|s| s.as_str()).unwrap_or("");
                    next == "()" || (next == "(" && self.tokens.get(self.pos + 2).map(|s| s.as_str()) == Some(")"))
                };

                if is_func_kw {
                    Some(self.parse_function_keyword())
                } else if is_func_paren {
                    Some(self.parse_function())
                } else {
                    // Inline assign: VAR=value (bez komendy po)
                    // lub zwykła komenda
                    self.parse_command_or_assign()
                }
            }
        }
    }

    fn parse_command_or_assign(&mut self) -> Option<Node> {
        // Zbierz tokeny do końca linii
        let mut parts = Vec::new();
        while let Some(t) = self.peek() {
            if t == "\n" || t == ";" {
                self.pos += 1;
                break;
            }
            if matches!(t, "fi"|"done"|"esac"|"else"|"elif"|"then"|"}"|";;") {
                break;
            }
            parts.push(t.to_string());
            self.pos += 1;
        }
        if parts.is_empty() { return None; }

        // Sprawdź czy to samo-stojące przypisanie VAR=val
        if parts.len() == 1 {
            if let Some(eq_pos) = parts[0].find('=') {
                let name = parts[0][..eq_pos].to_string();
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    let value = parts[0][eq_pos + 1..].to_string();
                    return Some(Node::Assign { name, value });
                }
            }
        }

        Some(Node::Command(parts.join(" ")))
    }

    fn parse_if(&mut self) -> Node {
        self.expect("if");
        let condition = self.parse_condition();
        self.expect("then");
        self.skip_newlines();
        let then_body = self.parse_list();

        let mut elif_branches = Vec::new();
        let mut else_body     = None;

        loop {
            match self.peek() {
                Some("elif") => {
                    self.pos += 1;
                    let cond = self.parse_condition();
                    self.expect("then");
                    self.skip_newlines();
                    let body = self.parse_list();
                    elif_branches.push((Box::new(cond), body));
                }
                Some("else") => {
                    self.pos += 1;
                    self.skip_newlines();
                    else_body = Some(self.parse_list());
                    break;
                }
                _ => break,
            }
        }
        self.expect("fi");

        Node::If {
            condition: Box::new(condition),
            then_body,
            elif_branches,
            else_body,
        }
    }

    fn parse_condition(&mut self) -> Node {
        let mut parts = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(t, "then" | "do" | "\n") { break; }
            if t == ";" { self.pos += 1; break; }
            parts.push(t.to_string());
            self.pos += 1;
        }
        Node::Command(parts.join(" "))
    }

    fn parse_while(&mut self) -> Node {
        self.expect("while");
        let condition = self.parse_condition();
        self.skip_newlines();
        self.expect("do");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("done");
        Node::While {
            condition: Box::new(condition),
            body,
        }
    }

    fn parse_until(&mut self) -> Node {
        self.expect("until");
        let condition = self.parse_condition();
        self.skip_newlines();
        self.expect("do");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("done");
        Node::Until {
            condition: Box::new(condition),
            body,
        }
    }

    fn parse_for(&mut self) -> Node {
        self.expect("for");

        // Sprawdź arytmetyczny for: for (( init; cond; update ))
        if self.peek() == Some("((") || self.peek() == Some("(") {
            return self.parse_for_arith();
        }

        let var = self.next().unwrap_or("i").to_string();

        // 'in' jest opcjonalne (for var; do ... done = for var in "$@"; do)
        if self.peek() == Some("in") {
            self.pos += 1;
        } else {
            // for var; do — iteruj po argumentach pozycyjnych
            self.skip_newlines();
            self.expect("do");
            self.skip_newlines();
            let body = self.parse_list();
            self.expect("done");
            return Node::For {
                var,
                items: vec!["$@".to_string()],
                body,
            };
        }

        let mut items = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(t, "do" | "\n" | ";") { break; }
            items.push(t.to_string());
            self.pos += 1;
        }
        self.skip_newlines();
        self.expect("do");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("done");
        Node::For { var, items, body }
    }

    fn parse_for_arith(&mut self) -> Node {
        // Spożyj (( lub (
        let opening = self.next().unwrap_or("((");
        let double = opening == "((";

        // Zbierz do )) lub )
        let mut content = String::new();
        let mut depth = if double { 2i32 } else { 1i32 };
        while let Some(t) = self.peek() {
            if (t == "))" && double) || (t == ")" && !double) {
                self.pos += 1;
                break;
            }
            if !content.is_empty() { content.push(' '); }
            content.push_str(t);
            self.pos += 1;
        }

        // Podziel na ; init; cond; update
        let parts: Vec<&str> = content.splitn(3, ';').collect();
        let init      = parts.get(0).unwrap_or(&"").trim().to_string();
        let condition = parts.get(1).unwrap_or(&"").trim().to_string();
        let update    = parts.get(2).unwrap_or(&"").trim().to_string();

        self.skip_newlines();
        self.expect("do");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("done");

        Node::ForArith { init, condition, update, body }
    }

    fn parse_case(&mut self) -> Node {
        self.expect("case");
        let word = self.next().unwrap_or("").to_string();
        self.expect("in");
        self.skip_newlines();

        let mut arms = Vec::new();
        while !matches!(self.peek(), Some("esac") | None) {
            let mut patterns = Vec::new();
            if self.peek() == Some("(") { self.pos += 1; }
            while let Some(t) = self.peek() {
                if t == ")" { self.pos += 1; break; }
                let parts: Vec<&str> = t.split('|').collect();
                for p in parts { patterns.push(p.to_string()); }
                self.pos += 1;
            }
            self.skip_newlines();
            let mut body = Vec::new();
            while let Some(t) = self.peek() {
                if t == ";;" || t == "esac" { break; }
                if let Some(node) = self.parse_statement() {
                    body.push(node);
                }
                self.skip_newlines();
            }
            if self.peek() == Some(";;") { self.pos += 1; }
            self.skip_newlines();
            if !patterns.is_empty() {
                arms.push((patterns, body));
            }
        }
        self.expect("esac");
        Node::Case { word, arms }
    }

    fn parse_function(&mut self) -> Node {
        let name = self.next().unwrap_or("").to_string();
        match self.peek() {
            Some("()") => { self.pos += 1; }
            Some("(")  => {
                self.pos += 1;
                if self.peek() == Some(")") { self.pos += 1; }
            }
            _ => {}
        }
        self.skip_newlines();
        self.expect("{");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("}");
        Node::FunctionDef { name, body }
    }

    fn parse_function_keyword(&mut self) -> Node {
        self.expect("function");
        let name = self.next().unwrap_or("").to_string();
        // Opcjonalne ()
        if self.peek() == Some("()") { self.pos += 1; }
        else if self.peek() == Some("(") {
            self.pos += 1;
            if self.peek() == Some(")") { self.pos += 1; }
        }
        self.skip_newlines();
        self.expect("{");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("}");
        Node::FunctionDef { name, body }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tokenizer dla tekstu skryptu
// ─────────────────────────────────────────────────────────────────────────────

fn tokenize_script(input: &str) -> Vec<String> {
    let mut tokens    = Vec::new();
    let mut current   = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Komentarze
        if c == '#' && !in_single && !in_double {
            // Pomiń do końca linii
            while i < chars.len() && chars[i] != '\n' { i += 1; }
            continue;
        }

        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(c);
                i += 1;
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(c);
                i += 1;
            }

            '\n' | ';' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(c.to_string());
                i += 1;
            }

            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                i += 1;
            }

            // (( jako jeden token
            '(' if !in_single && !in_double && chars.get(i + 1) == Some(&'(') => {
                if !current.is_empty() { tokens.push(std::mem::take(&mut current)); }
                tokens.push("((".to_string());
                i += 2;
            }

            // )) jako jeden token
            ')' if !in_single && !in_double && chars.get(i + 1) == Some(&')') => {
                if !current.is_empty() { tokens.push(std::mem::take(&mut current)); }
                tokens.push("))".to_string());
                i += 2;
            }

            // () jako jeden token dla definicji funkcji
            '(' if !in_single && !in_double && chars.get(i + 1) == Some(&')') => {
                if !current.is_empty() { tokens.push(std::mem::take(&mut current)); }
                tokens.push("()".to_string());
                i += 2;
            }

            // Escape w podwójnych cudzysłowach
            '\\' if in_double => {
                current.push(c);
                i += 1;
                if i < chars.len() {
                    current.push(chars[i]);
                    i += 1;
                }
            }

            // Kontynuacja linii: \ przed \n
            '\\' if !in_single && !in_double && chars.get(i + 1) == Some(&'\n') => {
                i += 2; // Pomiń \ i \n
            }

            _ => { current.push(c); i += 1; }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

// ─────────────────────────────────────────────────────────────────────────────
// test / [ ] builtin — natywna implementacja
// ─────────────────────────────────────────────────────────────────────────────

pub fn builtin_test(args: &[String]) -> i32 {
    let args: Vec<&str> = args.iter()
        .map(|s| s.as_str())
        .filter(|&s| s != "[" && s != "]" && s != "test")
        .collect();

    eval_test(&args)
}

fn eval_test(args: &[&str]) -> i32 {
    match args {
        [] => 1,

        // Unarne
        [op, path] => match *op {
            "-f" => if Path::new(path).is_file()                         { 0 } else { 1 },
            "-d" => if Path::new(path).is_dir()                          { 0 } else { 1 },
            "-e" => if Path::new(path).exists()                          { 0 } else { 1 },
            "-r" => if is_readable(path)                                  { 0 } else { 1 },
            "-w" => if is_writable(path)                                  { 0 } else { 1 },
            "-x" => if is_executable(path)                                { 0 } else { 1 },
            "-z" => if path.is_empty()                                    { 0 } else { 1 },
            "-n" => if !path.is_empty()                                   { 0 } else { 1 },
            "-L" | "-h" => if Path::new(path).is_symlink()               { 0 } else { 1 },
            "-s" => {
                if fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) { 0 } else { 1 }
            }
            "-p" => if Path::new(path).exists()                           { 0 } else { 1 },
            _    => 1,
        },

        // Binarne string/numeric
        [a, op, b] => match *op {
            "="  | "==" => if a == b { 0 } else { 1 },
            "!=" | "<>" => if a != b { 0 } else { 1 },
            "<"          => if *a < *b { 0 } else { 1 },
            ">"          => if *a > *b { 0 } else { 1 },
            "-eq" => num_cmp(a, b, |x, y| x == y),
            "-ne" => num_cmp(a, b, |x, y| x != y),
            "-lt" => num_cmp(a, b, |x, y| x <  y),
            "-le" => num_cmp(a, b, |x, y| x <= y),
            "-gt" => num_cmp(a, b, |x, y| x >  y),
            "-ge" => num_cmp(a, b, |x, y| x >= y),
            "-nt" => newer_than(a, b),
            "-ot" => newer_than(b, a),
            "-ef" => same_file(a, b),
            _     => 1,
        },

        // Negacja: ! expr
        ["!", rest @ ..] => if eval_test(rest) != 0 { 0 } else { 1 },

        // Złożone: -a / -o
        _ => {
            if let Some(pos) = args.iter().position(|&s| s == "-a") {
                let l = eval_test(&args[..pos]);
                let r = eval_test(&args[pos + 1..]);
                if l == 0 && r == 0 { 0 } else { 1 }
            } else if let Some(pos) = args.iter().position(|&s| s == "-o") {
                let l = eval_test(&args[..pos]);
                let r = eval_test(&args[pos + 1..]);
                if l == 0 || r == 0 { 0 } else { 1 }
            } else if args.len() == 1 {
                if !args[0].is_empty() { 0 } else { 1 }
            } else {
                1
            }
        }
    }
}

fn num_cmp(a: &str, b: &str, f: impl Fn(i64, i64) -> bool) -> i32 {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(x), Ok(y)) => if f(x, y) { 0 } else { 1 },
        _ => 1,
    }
}

fn newer_than(a: &str, b: &str) -> i32 {
    let ta = fs::metadata(a).and_then(|m| m.modified()).ok();
    let tb = fs::metadata(b).and_then(|m| m.modified()).ok();
    match (ta, tb) {
        (Some(ta), Some(tb)) => if ta > tb { 0 } else { 1 },
        _ => 1,
    }
}

fn same_file(a: &str, b: &str) -> i32 {
    use std::os::unix::fs::MetadataExt;
    let ma = fs::metadata(a).ok();
    let mb = fs::metadata(b).ok();
    match (ma, mb) {
        (Some(ma), Some(mb)) => {
            if ma.ino() == mb.ino() && ma.dev() == mb.dev() { 0 } else { 1 }
        }
        _ => 1,
    }
}

fn is_readable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).map(|m| m.permissions().mode() & 0o444 != 0).unwrap_or(false)
}

fn is_writable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).map(|m| m.permissions().mode() & 0o222 != 0).unwrap_or(false)
}

fn is_executable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

// ─────────────────────────────────────────────────────────────────────────────
// Heredoc collector
// ─────────────────────────────────────────────────────────────────────────────

pub fn collect_heredocs(
    lines:      &[&str],
    line_idx:   &mut usize,
    delimiter:  &str,
    expand_vars: bool,
    vars:       &HashMap<String, String>,
) -> String {
    let mut body = String::new();
    *line_idx += 1;
    while *line_idx < lines.len() {
        let line = lines[*line_idx];
        if line.trim() == delimiter {
            *line_idx += 1;
            break;
        }
        if expand_vars {
            let mut expanded = line.to_string();
            for (k, v) in vars {
                expanded = expanded.replace(&format!("${}", k), v);
                expanded = expanded.replace(&format!("${{{}}}", k), v);
            }
            body.push_str(&expanded);
        } else {
            body.push_str(line);
        }
        body.push('\n');
        *line_idx += 1;
    }
    body
}

// ─────────────────────────────────────────────────────────────────────────────
// Process substitution
// ─────────────────────────────────────────────────────────────────────────────

pub fn expand_process_substitution(input: &str) -> (String, Vec<ProcessSubst>) {
    let mut result = input.to_string();
    let mut substs = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' && chars.get(i + 1) == Some(&'(') {
            let start = i;
            i += 2;
            let cmd_start = i;
            let mut depth = 1i32;
            while i < chars.len() {
                if chars[i] == '(' { depth += 1; }
                if chars[i] == ')' { depth -= 1; if depth == 0 { break; } }
                i += 1;
            }
            let cmd: String = chars[cmd_start..i].iter().collect();
            if i < chars.len() { i += 1; }
            let original: String = chars[start..i].iter().collect();
            let idx = substs.len();
            substs.push(ProcessSubst { cmd, direction: SubstDir::Read });
            result = result.replacen(&original, &format!("/dev/fd/SUBST_{}", idx), 1);
        } else {
            i += 1;
        }
    }
    (result, substs)
}

#[derive(Debug)]
pub enum SubstDir { Read, Write }

#[derive(Debug)]
pub struct ProcessSubst {
    pub cmd:       String,
    pub direction: SubstDir,
}

// ─────────────────────────────────────────────────────────────────────────────
// Walidacja składni skryptu
// ─────────────────────────────────────────────────────────────────────────────

/// Wynik walidacji składni skryptu .sh
#[derive(Debug)]
pub enum SyntaxCheck {
    Ok,
    Error { line: usize, message: String },
    Warning { line: usize, message: String },
}

/// Prosta walidacja składni skryptu przed uruchomieniem.
pub fn validate_script(content: &str) -> Vec<SyntaxCheck> {
    let mut results = Vec::new();
    let mut if_depth   = 0i32;
    let mut for_depth  = 0i32;
    let mut while_depth = 0i32;
    let mut case_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut in_single  = false;
    let mut in_double  = false;

    for (lineno, line) in content.lines().enumerate() {
        let line_no = lineno + 1;
        let trimmed = line.trim();

        // Pomiń komentarze
        if trimmed.starts_with('#') { continue; }
        if trimmed.is_empty()       { continue; }

        // Śledź cudzysłowy
        for ch in trimmed.chars() {
            match ch {
                '\'' if !in_double => in_single = !in_single,
                '"'  if !in_single => in_double = !in_double,
                _ => {}
            }
        }

        if in_single || in_double { continue; } // Wewnątrz stringu

        // Sprawdź słowa kluczowe
        let first = trimmed.split_whitespace().next().unwrap_or("");
        match first {
            "if"    => if_depth    += 1,
            "fi"    => {
                if_depth -= 1;
                if if_depth < 0 {
                    results.push(SyntaxCheck::Error {
                        line: line_no,
                        message: "nieoczekiwane 'fi' bez pasującego 'if'".to_string(),
                    });
                    if_depth = 0;
                }
            }
            "for"   => for_depth   += 1,
            "while" => while_depth += 1,
            "until" => while_depth += 1,
            "done"  => {
                if for_depth > 0 { for_depth -= 1; }
                else if while_depth > 0 { while_depth -= 1; }
                else {
                    results.push(SyntaxCheck::Error {
                        line: line_no,
                        message: "nieoczekiwane 'done' bez pasującej pętli".to_string(),
                    });
                }
            }
            "case"  => case_depth  += 1,
            "esac"  => {
                case_depth -= 1;
                if case_depth < 0 {
                    results.push(SyntaxCheck::Error {
                        line: line_no,
                        message: "nieoczekiwane 'esac' bez pasującego 'case'".to_string(),
                    });
                    case_depth = 0;
                }
            }
            _ => {}
        }

        // Nawiasy klamrowe
        for ch in trimmed.chars() {
            if ch == '{' { brace_depth += 1; }
            if ch == '}' {
                brace_depth -= 1;
                if brace_depth < 0 {
                    results.push(SyntaxCheck::Warning {
                        line: line_no,
                        message: "nieoczekiwany '}'".to_string(),
                    });
                    brace_depth = 0;
                }
            }
        }

        // Ostrzeżenie: rm -rf bez ograniczenia
        if trimmed.contains("rm -rf /") || trimmed.contains("rm -rf /*") {
            results.push(SyntaxCheck::Warning {
                line: line_no,
                message: "potencjalnie niebezpieczna komenda rm -rf!".to_string(),
            });
        }
    }

    // Brakujące zamknięcia
    if if_depth > 0 {
        results.push(SyntaxCheck::Error {
            line: 0,
            message: format!("brak {} 'fi' dla 'if'", if_depth),
        });
    }
    if for_depth > 0 || while_depth > 0 {
        results.push(SyntaxCheck::Error {
            line: 0,
            message: format!("brak 'done' dla pętli"),
        });
    }
    if case_depth > 0 {
        results.push(SyntaxCheck::Error {
            line: 0,
            message: format!("brak 'esac' dla 'case'"),
        });
    }
    if brace_depth > 0 {
        results.push(SyntaxCheck::Error {
            line: 0,
            message: format!("brak {} '}}' dla funkcji/bloku", brace_depth),
        });
    }

    results
}

/// Drukuj wyniki walidacji
pub fn print_syntax_errors(path: &str, checks: &[SyntaxCheck]) -> bool {
    let mut has_errors = false;
    for check in checks {
        match check {
            SyntaxCheck::Error { line, message } => {
                has_errors = true;
                if *line == 0 {
                    eprintln!("\x1b[1;31m[błąd składni]\x1b[0m {}: {}", path, message);
                } else {
                    eprintln!("\x1b[1;31m[błąd składni]\x1b[0m {}:{}: {}", path, line, message);
                }
            }
            SyntaxCheck::Warning { line, message } => {
                if *line == 0 {
                    eprintln!("\x1b[1;33m[ostrzeżenie]\x1b[0m {}: {}", path, message);
                } else {
                    eprintln!("\x1b[1;33m[ostrzeżenie]\x1b[0m {}:{}: {}", path, line, message);
                }
            }
            SyntaxCheck::Ok => {}
        }
    }
    has_errors
}
