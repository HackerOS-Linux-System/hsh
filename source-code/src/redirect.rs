use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::{IntoRawFd, RawFd};

use libc::{dup2, close};

#[derive(Debug, Clone)]
pub enum RedirectTarget {
    File(String),
    Fd(RawFd),
    HereDoc(String),
}

#[derive(Debug, Clone)]
pub enum RedirectKind {
    Out,
    Append,
    In,
    HereDoc,
    OutErr,
    DupFd,
}

#[derive(Debug, Clone)]
pub struct Redirect {
    pub kind:   RedirectKind,
    pub fd:     RawFd,
    pub target: RedirectTarget,
}

/// Parse redirections out of a command string.
/// Returns (clean_command_without_redirections, Vec<Redirect>)
pub fn parse_redirections(input: &str) -> (String, Vec<Redirect>) {
    let mut redirects = Vec::new();
    let mut clean     = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i         = 0;
    let mut in_single = false;
    let mut in_double = false;

    while i < chars.len() {
        let c = chars[i];

        match c {
            '\'' if !in_double => { in_single = !in_single; clean.push(c); i += 1; }
            '"'  if !in_single => { in_double = !in_double; clean.push(c); i += 1; }

            // heredoc <<  (but NOT process substitution <() )
            '<' if !in_single && !in_double
            && chars.get(i + 1) == Some(&'<')
            && chars.get(i + 2) != Some(&'(') =>
            {
                i += 2;
                // optional hyphen for strip-tabs
                let mut strip_tabs = false;
                if chars.get(i) == Some(&'-') {
                    strip_tabs = true;
                    i += 1;
                }
                while i < chars.len() && chars[i] == ' ' { i += 1; }
                let mut delim = String::new();
                // maybe quoted delimiter
                let quote = chars.get(i).copied();
                if quote == Some('"') || quote == Some('\'') {
                    i += 1;
                    while i < chars.len() && chars[i] != quote.unwrap() {
                        delim.push(chars[i]);
                        i += 1;
                    }
                    if i < chars.len() { i += 1; } // skip closing quote
                } else {
                    while i < chars.len() && chars[i] != '\n' && chars[i] != ';' && !chars[i].is_whitespace() {
                        delim.push(chars[i]);
                        i += 1;
                    }
                }
                // store delimiter, strip-tabs flag will be handled later in heredoc body extraction
                // For now we just store the delimiter as is; the actual body is collected in execute.rs
                redirects.push(Redirect {
                    kind:   RedirectKind::HereDoc,
                    fd:     0,
                    target: RedirectTarget::HereDoc(delim),
                });
            }

            // process substitution <(cmd) — pass through unchanged
            '<' if !in_single && !in_double && chars.get(i + 1) == Some(&'(') => {
                let start = i;
                let mut depth = 0i32;
                while i < chars.len() {
                    if chars[i] == '(' { depth += 1; }
                    if chars[i] == ')' {
                        depth -= 1;
                        if depth == 0 { i += 1; break; }
                    }
                    i += 1;
                }
                let subst: String = chars[start..i].iter().collect();
                clean.push_str(&subst);
            }

            // N>  N>>  N>&M  N<
            c if c.is_ascii_digit()
            && !in_single && !in_double
            && (chars.get(i + 1) == Some(&'>') || chars.get(i + 1) == Some(&'<')) =>
            {
                let src_fd = (c as u8 - b'0') as RawFd;
                i += 1;
                let op = chars[i];
                i += 1;

                if op == '>' && chars.get(i) == Some(&'&') {
                    i += 1;
                    let mut num = String::new();
                    while i < chars.len() && chars[i].is_ascii_digit() { num.push(chars[i]); i += 1; }
                    let dst: RawFd = num.parse().unwrap_or(1);
                    redirects.push(Redirect { kind: RedirectKind::DupFd, fd: src_fd, target: RedirectTarget::Fd(dst) });
                } else if op == '>' && chars.get(i) == Some(&'>') {
                    i += 1;
                    let path = read_word(&chars, &mut i);
                    redirects.push(Redirect { kind: RedirectKind::Append, fd: src_fd, target: RedirectTarget::File(path) });
                } else if op == '>' {
                    let path = read_word(&chars, &mut i);
                    redirects.push(Redirect { kind: RedirectKind::Out, fd: src_fd, target: RedirectTarget::File(path) });
                } else {
                    // N<
                    let path = read_word(&chars, &mut i);
                    redirects.push(Redirect { kind: RedirectKind::In, fd: src_fd, target: RedirectTarget::File(path) });
                }
            }

            // &>  stdout+stderr to file
            '&' if !in_single && !in_double && chars.get(i + 1) == Some(&'>') => {
                i += 2;
                let path = read_word(&chars, &mut i);
                redirects.push(Redirect { kind: RedirectKind::OutErr, fd: 1, target: RedirectTarget::File(path.clone()) });
                redirects.push(Redirect { kind: RedirectKind::OutErr, fd: 2, target: RedirectTarget::File(path) });
            }

            // >>
            '>' if !in_single && !in_double && chars.get(i + 1) == Some(&'>') => {
                i += 2;
                let path = read_word(&chars, &mut i);
                redirects.push(Redirect { kind: RedirectKind::Append, fd: 1, target: RedirectTarget::File(path) });
            }

            // >  or  >&N
            '>' if !in_single && !in_double => {
                i += 1;
                if chars.get(i) == Some(&'&') {
                    i += 1;
                    let mut num = String::new();
                    while i < chars.len() && chars[i].is_ascii_digit() { num.push(chars[i]); i += 1; }
                    let dst: RawFd = num.parse().unwrap_or(2);
                    redirects.push(Redirect { kind: RedirectKind::DupFd, fd: 1, target: RedirectTarget::Fd(dst) });
                } else {
                    let path = read_word(&chars, &mut i);
                    redirects.push(Redirect { kind: RedirectKind::Out, fd: 1, target: RedirectTarget::File(path) });
                }
            }

            // <
            '<' if !in_single && !in_double => {
                i += 1;
                let path = read_word(&chars, &mut i);
                redirects.push(Redirect { kind: RedirectKind::In, fd: 0, target: RedirectTarget::File(path) });
            }

            _ => { clean.push(c); i += 1; }
        }
    }

    (clean.trim().to_string(), redirects)
}

/// Apply redirections in the child process (after fork, before exec).
pub fn apply_redirections(
    redirects: &[Redirect],
    heredoc_bodies: &HashMap<String, String>,
) -> io::Result<()> {
    for r in redirects {
        match (&r.kind, &r.target) {
            (RedirectKind::Out, RedirectTarget::File(path)) => {
                let f = File::create(path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path, e)))?;
                safe_dup2(f.into_raw_fd(), r.fd)?;
            }
            (RedirectKind::Append, RedirectTarget::File(path)) => {
                let f = OpenOptions::new().append(true).create(true).open(path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path, e)))?;
                safe_dup2(f.into_raw_fd(), r.fd)?;
            }
            (RedirectKind::In, RedirectTarget::File(path)) => {
                let f = File::open(path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path, e)))?;
                safe_dup2(f.into_raw_fd(), r.fd)?;
            }
            (RedirectKind::DupFd, RedirectTarget::Fd(dst)) => {
                safe_dup2(*dst, r.fd)?;
            }
            (RedirectKind::OutErr, RedirectTarget::File(path)) => {
                let f = File::create(path)
                .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path, e)))?;
                safe_dup2(f.into_raw_fd(), r.fd)?;
            }
            (RedirectKind::HereDoc, RedirectTarget::HereDoc(delim)) => {
                if let Some(body) = heredoc_bodies.get(delim) {
                    // Create a pipe, write body into write end, dup read end to fd
                    let mut pipe_fds: [libc::c_int; 2] = [-1, -1];
                    let ret = unsafe { libc::pipe(pipe_fds.as_mut_ptr()) };
                    if ret != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    let (read_fd, write_fd) = (pipe_fds[0], pipe_fds[1]);
                    let body_bytes = body.as_bytes();
                    unsafe {
                        libc::write(
                            write_fd,
                            body_bytes.as_ptr() as *const libc::c_void,
                                    body_bytes.len(),
                        );
                        close(write_fd);
                    }
                    safe_dup2(read_fd, r.fd)?;
                    unsafe { close(read_fd); }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Safe wrapper around libc::dup2
fn safe_dup2(old: RawFd, new: RawFd) -> io::Result<()> {
    let ret = unsafe { dup2(old, new) };
    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        // Close the original fd if it differs from the new one
        if old != new {
            unsafe { close(old); }
        }
        Ok(())
    }
}

/// Read a word token, skipping leading whitespace, stopping at shell metacharacters.
fn read_word(chars: &[char], i: &mut usize) -> String {
    while *i < chars.len() && chars[*i] == ' ' { *i += 1; }
    let mut word  = String::new();
    let mut in_s  = false;
    let mut in_d  = false;
    while *i < chars.len() {
        let c = chars[*i];
        match c {
            '\'' if !in_d => { in_s = !in_s; *i += 1; }
            '"'  if !in_s => { in_d = !in_d; *i += 1; }
            ' ' | '\t' | '\n' | ';' | '&' | '|' if !in_s && !in_d => break,
            _ => { word.push(c); *i += 1; }
        }
    }
    word
}
