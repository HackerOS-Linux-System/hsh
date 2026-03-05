// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2025 Michał. All rights reserved.
//
// Natywna arytmetyka powłoki: $(( wyrażenie ))
// Obsługuje: + - * / % ** ( ) zmienne porównania logiczne

use std::collections::HashMap;

#[derive(Debug)]
enum Token {
    Num(i64),
    Plus, Minus, Star, Slash, Percent, StarStar,
    LParen, RParen,
    Eq, Neq, Lt, Le, Gt, Ge,
    And, Or, Not,
    Var(String),
}

pub fn evaluate(expr: &str, vars: &HashMap<String, String>) -> Result<i64, String> {
    let tokens = tokenize(expr, vars)?;
    let mut pos = 0;
    let result = parse_or(&tokens, &mut pos)?;
    Ok(result)
}

/// Expand all $(( )) in a string, return the result string.
pub fn expand_arithmetic(input: &str, vars: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut i = 0;
    let chars: Vec<char> = input.chars().collect();

    while i < chars.len() {
        // Look for $((
        if chars[i] == '$'
            && chars.get(i + 1) == Some(&'(')
            && chars.get(i + 2) == Some(&'(')
            {
                i += 3;
                let start = i;
                let mut depth = 2i32;
                while i < chars.len() {
                    if chars[i] == '(' { depth += 1; }
                    if chars[i] == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
                let expr: String = chars[start..i].iter().collect();
                // skip closing ))
                if chars.get(i) == Some(&')') { i += 1; }
                if chars.get(i) == Some(&')') { i += 1; }

                match evaluate(&expr, vars) {
                    Ok(val)  => result.push_str(&val.to_string()),
                    Err(e)   => {
                        eprintln!("hsh: arithmetic: {}", e);
                        result.push_str("0");
                    }
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
    }

    result
}

// ─── Tokenizer ────────────────────────────────────────────────────────────────

fn tokenize(expr: &str, vars: &HashMap<String, String>) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.trim().chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' => { i += 1; }

            '0'..='9' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() { i += 1; }
                let num: i64 = chars[start..i].iter().collect::<String>().parse()
                .map_err(|_| "invalid number".to_string())?;
                tokens.push(Token::Num(num));
            }

            // variable or keyword
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
                let name: String = chars[start..i].iter().collect();
                // resolve variable value
                let val = vars.get(&name)
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
                tokens.push(Token::Num(val));
            }

            '$' => {
                i += 1;
                if chars.get(i) == Some(&'{') {
                    i += 1;
                    let start = i;
                    while i < chars.len() && chars[i] != '}' { i += 1; }
                    let name: String = chars[start..i].iter().collect();
                    if i < chars.len() { i += 1; }
                    let val = vars.get(&name)
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                    tokens.push(Token::Num(val));
                } else {
                    let start = i;
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') { i += 1; }
                    let name: String = chars[start..i].iter().collect();
                    let val = vars.get(&name)
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                    tokens.push(Token::Num(val));
                }
            }

            '+' => { tokens.push(Token::Plus);    i += 1; }
            '-' => { tokens.push(Token::Minus);   i += 1; }
            '%' => { tokens.push(Token::Percent); i += 1; }
            '(' => { tokens.push(Token::LParen);  i += 1; }
            ')' => { tokens.push(Token::RParen);  i += 1; }

            '*' => {
                if chars.get(i + 1) == Some(&'*') {
                    tokens.push(Token::StarStar); i += 2;
                } else {
                    tokens.push(Token::Star); i += 1;
                }
            }
            '/' => { tokens.push(Token::Slash); i += 1; }

            '=' if chars.get(i + 1) == Some(&'=') => { tokens.push(Token::Eq);  i += 2; }
            '!' if chars.get(i + 1) == Some(&'=') => { tokens.push(Token::Neq); i += 2; }
            '<' if chars.get(i + 1) == Some(&'=') => { tokens.push(Token::Le);  i += 2; }
            '>' if chars.get(i + 1) == Some(&'=') => { tokens.push(Token::Ge);  i += 2; }
            '<' => { tokens.push(Token::Lt); i += 1; }
            '>' => { tokens.push(Token::Gt); i += 1; }
            '!' => { tokens.push(Token::Not); i += 1; }

            '&' if chars.get(i + 1) == Some(&'&') => { tokens.push(Token::And); i += 2; }
            '|' if chars.get(i + 1) == Some(&'|') => { tokens.push(Token::Or);  i += 2; }

            c => return Err(format!("unexpected character: '{}'", c)),
        }
    }
    Ok(tokens)
}

// ─── Recursive descent parser ─────────────────────────────────────────────────
// Precedence (low→high): || && == != < <= > >= + - * / % ** unary

fn parse_or(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_and(t, pos)?;
    while matches!(t.get(*pos), Some(Token::Or)) {
        *pos += 1;
        let right = parse_and(t, pos)?;
        left = if left != 0 || right != 0 { 1 } else { 0 };
    }
    Ok(left)
}

fn parse_and(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_cmp(t, pos)?;
    while matches!(t.get(*pos), Some(Token::And)) {
        *pos += 1;
        let right = parse_cmp(t, pos)?;
        left = if left != 0 && right != 0 { 1 } else { 0 };
    }
    Ok(left)
}

fn parse_cmp(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_add(t, pos)?;
    loop {
        match t.get(*pos) {
            Some(Token::Eq)  => { *pos += 1; left = (left == parse_add(t, pos)?) as i64; }
            Some(Token::Neq) => { *pos += 1; left = (left != parse_add(t, pos)?) as i64; }
            Some(Token::Lt)  => { *pos += 1; left = (left <  parse_add(t, pos)?) as i64; }
            Some(Token::Le)  => { *pos += 1; left = (left <= parse_add(t, pos)?) as i64; }
            Some(Token::Gt)  => { *pos += 1; left = (left >  parse_add(t, pos)?) as i64; }
            Some(Token::Ge)  => { *pos += 1; left = (left >= parse_add(t, pos)?) as i64; }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_add(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_mul(t, pos)?;
    loop {
        match t.get(*pos) {
            Some(Token::Plus)  => { *pos += 1; left += parse_mul(t, pos)?; }
            Some(Token::Minus) => { *pos += 1; left -= parse_mul(t, pos)?; }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_mul(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_pow(t, pos)?;
    loop {
        match t.get(*pos) {
            Some(Token::Star)    => { *pos += 1; left *= parse_pow(t, pos)?; }
            Some(Token::Slash)   => {
                *pos += 1;
                let r = parse_pow(t, pos)?;
                if r == 0 { return Err("division by zero".into()); }
                left /= r;
            }
            Some(Token::Percent) => {
                *pos += 1;
                let r = parse_pow(t, pos)?;
                if r == 0 { return Err("modulo by zero".into()); }
                left %= r;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_pow(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    let base = parse_unary(t, pos)?;
    if matches!(t.get(*pos), Some(Token::StarStar)) {
        *pos += 1;
        let exp = parse_unary(t, pos)?;
        if exp < 0 { return Err("negative exponent".into()); }
        Ok(base.pow(exp as u32))
    } else {
        Ok(base)
    }
}

fn parse_unary(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    match t.get(*pos) {
        Some(Token::Minus) => { *pos += 1; Ok(-parse_primary(t, pos)?) }
        Some(Token::Not)   => { *pos += 1; Ok(if parse_primary(t, pos)? == 0 { 1 } else { 0 }) }
        _ => parse_primary(t, pos),
    }
}

fn parse_primary(t: &[Token], pos: &mut usize) -> Result<i64, String> {
    match t.get(*pos) {
        Some(Token::Num(n)) => { let v = *n; *pos += 1; Ok(v) }
        Some(Token::LParen) => {
            *pos += 1;
            let v = parse_or(t, pos)?;
            match t.get(*pos) {
                Some(Token::RParen) => { *pos += 1; Ok(v) }
                _ => Err("expected closing ')'".into()),
            }
        }
        other => Err(format!("unexpected token: {:?}", other)),
    }
}
