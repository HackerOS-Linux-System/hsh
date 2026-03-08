use std::collections::HashMap;
use std::path::Path;
use std::fs;

// ─────────────────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Node {
    Command(String),
    If {
        condition: Box<Node>,
        then_body: Vec<Node>,
        elif_branches: Vec<(Box<Node>, Vec<Node>)>,
        else_body: Option<Vec<Node>>,
    },
    While {
        condition: Box<Node>,
        body: Vec<Node>,
    },
    For {
        var: String,
        items: Vec<String>,
        body: Vec<Node>,
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
                "fi" | "done" | "esac" | "else" | "elif" | "then" | "}" => break,
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
            "if"    => Some(self.parse_if()),
            "while" => Some(self.parse_while()),
            "for"   => Some(self.parse_for()),
            "case"  => Some(self.parse_case()),
            _ => {
                // Check for function definition: name()
                let is_func = {
                    let t = self.tokens.get(self.pos).map(|s| s.as_str()).unwrap_or("");
                    let next = self.tokens.get(self.pos + 1).map(|s| s.as_str()).unwrap_or("");
                    next == "()" || next == "(" && self.tokens.get(self.pos + 2).map(|s| s.as_str()) == Some(")")
                };

                if is_func {
                    Some(self.parse_function())
                } else {
                    // Collect until newline or semicolon
                    let mut cmd = String::new();
                    while let Some(t) = self.peek() {
                        if t == "\n" || t == ";" { self.pos += 1; break; }
                        if matches!(t, "fi"|"done"|"esac"|"else"|"elif"|"then"|"}") { break; }
                        if !cmd.is_empty() { cmd.push(' '); }
                        cmd.push_str(t);
                        self.pos += 1;
                    }
                    if cmd.is_empty() { None } else { Some(Node::Command(cmd)) }
                }
            }
        }
    }

    fn parse_if(&mut self) -> Node {
        self.expect("if");
        let condition = self.parse_condition();
        self.expect("then");
        self.skip_newlines();
        let then_body = self.parse_list();

        let mut elif_branches = Vec::new();
        let mut else_body = None;

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
            parts.push(t.to_string());
            self.pos += 1;
        }
        Node::Command(parts.join(" "))
    }

    fn parse_while(&mut self) -> Node {
        self.expect("while");
        let condition = self.parse_condition();
        self.expect("do");
        self.skip_newlines();
        let body = self.parse_list();
        self.expect("done");
        Node::While {
            condition: Box::new(condition),
            body,
        }
    }

    fn parse_for(&mut self) -> Node {
        self.expect("for");
        let var = self.next().unwrap_or("i").to_string();
        self.expect("in");
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

    fn parse_case(&mut self) -> Node {
        self.expect("case");
        let word = self.next().unwrap_or("").to_string();
        self.expect("in");
        self.skip_newlines();

        let mut arms = Vec::new();
        while !matches!(self.peek(), Some("esac") | None) {
            // patterns: pat1|pat2)
            let mut patterns = Vec::new();
            // skip optional leading (
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
            arms.push((patterns, body));
        }
        self.expect("esac");
        Node::Case { word, arms }
    }

    fn parse_function(&mut self) -> Node {
        let name = self.next().unwrap_or("").to_string();
        // consume () or ( )
        match self.peek() {
            Some("()") => { self.pos += 1; }
            Some("(")  => { self.pos += 1; if self.peek() == Some(")") { self.pos += 1; } }
            _ => {}
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
// Tokenizer for script text
// ─────────────────────────────────────────────────────────────────────────────

fn tokenize_script(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' if !in_double => { in_single = !in_single; current.push(c); i += 1; }
            '"'  if !in_single => { in_double = !in_double; current.push(c); i += 1; }

            '\n' | ';' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(current.drain(..).collect());
                }
                tokens.push(c.to_string());
                i += 1;
            }

            ' ' | '\t' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(current.drain(..).collect());
                }
                i += 1;
            }

            // () as single token for function defs
            '(' if !in_single && !in_double && chars.get(i + 1) == Some(&')') => {
                if !current.is_empty() { tokens.push(current.drain(..).collect()); }
                tokens.push("()".to_string());
                i += 2;
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
// test / [ ] builtin — native implementation
// ─────────────────────────────────────────────────────────────────────────────

pub fn builtin_test(args: &[String]) -> i32 {
    // Strip surrounding [ ] if present
    let args: Vec<&str> = args.iter()
    .map(|s| s.as_str())
    .filter(|&s| s != "[" && s != "]")
    .collect();

    eval_test(&args)
}

fn eval_test(args: &[&str]) -> i32 {
    match args {
        // Unary: -f -d -e -r -w -x -z -n
        [op, path] => match *op {
            "-f" => if Path::new(path).is_file()       { 0 } else { 1 },
            "-d" => if Path::new(path).is_dir()        { 0 } else { 1 },
            "-e" => if Path::new(path).exists()        { 0 } else { 1 },
            "-r" => if is_readable(path)               { 0 } else { 1 },
            "-w" => if is_writable(path)               { 0 } else { 1 },
            "-x" => if is_executable(path)             { 0 } else { 1 },
            "-z" => if path.is_empty()                 { 0 } else { 1 },
            "-n" => if !path.is_empty()                { 0 } else { 1 },
            "-L" | "-h" => if Path::new(path).is_symlink() { 0 } else { 1 },
            "-s" => {
                if fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false) { 0 } else { 1 }
            }
            _ => 1,
        },

        // Binary string/numeric comparisons
        [a, op, b] => match *op {
            "="  | "==" => if a == b { 0 } else { 1 },
            "!=" | "ne" => if a != b { 0 } else { 1 },
            "-eq" => num_cmp(a, b, |x, y| x == y),
            "-ne" => num_cmp(a, b, |x, y| x != y),
            "-lt" => num_cmp(a, b, |x, y| x <  y),
            "-le" => num_cmp(a, b, |x, y| x <= y),
            "-gt" => num_cmp(a, b, |x, y| x >  y),
            "-ge" => num_cmp(a, b, |x, y| x >= y),
            "-nt" => newer_than(a, b),
            "-ot" => newer_than(b, a),
            _ => 1,
        },

        // Logical: ! expr
        ["!", rest @ ..] => if eval_test(rest) != 0 { 0 } else { 1 },

        // Compound: expr -a expr  /  expr -o expr
        _ => {
            if let Some(pos) = args.iter().position(|&s| s == "-a") {
                let left  = eval_test(&args[..pos]);
                let right = eval_test(&args[pos + 1..]);
                if left == 0 && right == 0 { 0 } else { 1 }
            } else if let Some(pos) = args.iter().position(|&s| s == "-o") {
                let left  = eval_test(&args[..pos]);
                let right = eval_test(&args[pos + 1..]);
                if left == 0 || right == 0 { 0 } else { 1 }
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

/// Given a multi-line string, extract all heredoc bodies.
/// Returns map: delimiter -> body
pub fn collect_heredocs(
    lines: &[&str],
    line_idx: &mut usize,
    delimiter: &str,
    expand_vars: bool,
    vars: &HashMap<String, String>,
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
            // Simple $VAR expansion in heredoc
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
// Process substitution  <(cmd)  and  >(cmd)
// ─────────────────────────────────────────────────────────────────────────────

/// Expand <(cmd) by running cmd, writing output to a temp fd, returning /dev/fd/N path.
/// This is called before spawning the outer command.
pub fn expand_process_substitution(input: &str) -> (String, Vec<ProcessSubst>) {
    let mut result   = input.to_string();
    let mut substs   = Vec::new();
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
            if i < chars.len() { i += 1; } // skip )
            let placeholder = format!("<({})__SUBST_{}", cmd, substs.len());
            substs.push(ProcessSubst { cmd, direction: SubstDir::Read });
            let original: String = chars[start..i].iter().collect();
            result = result.replacen(&original, &format!("/dev/fd/SUBST_{}", substs.len() - 1), 1);
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
