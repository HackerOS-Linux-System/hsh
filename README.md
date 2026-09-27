# hsh

A from-scratch, multi-file port of `hsh` (a shell for HackerOS) from
Rust to H#, matching the original's own file layout where a matching
concept exists. Uses [`hprompt`](../hprompt) for line editing/history.

## Files (mapped to the original)

| This project | Original | What it covers |
|---|---|---|
| `src/util.h#` | (scattered helpers) | shared string/scan helpers, `getenv_safe`, `write_raw` |
| `src/arithmetic.h#` | `arithmetic.rs` | `$(( ))` integer arithmetic — full port |
| `src/vars.h#` | `vars.rs` | variables, aliases, `$VAR`/`${VAR}`/`$(( ))`/`$(cmd)` expansion |
| `src/theme.h#` | `theme.rs` | **all 20** original color themes, generated from the exact Rust data |
| `src/git_info.h#` | `git_info.rs` | branch, dirty flag, ahead/behind, detached-HEAD — full port (sync, not the original's background watcher — see below) |
| `src/prompt.h#` | `prompt.rs` | every prompt segment: time, dir, git, sysinfo, depth, root, exit code, duration, prompt char |
| `src/config.h#` | `config.rs` | `~/.hshrc` loading, all 7 original sections (`shell`/`prompt`/`aliases`/`env`/`completion`/`safety`/`scripts`) |
| `src/security.h#` | `security.rs` | full port: 15 danger rules (Critical/High), 5 secret-redaction rules, restricted mode, PATH-hijack check, denied commands, hash-chained audit log |
| `src/path_cache.h#` | `path_cache.rs` | PATH command cache with TTL + PATH-hash invalidation |
| `src/history.h#` | `history.rs` | timestamped history, fuzzy search (own subsequence scorer — no fuzzy-match crate in H#) |
| `src/jobs.h#` | `jobs.rs` | job table; best-effort (see "Left out / limited" below) |
| `src/smarthints.h#` | `smarthints.rs` | Levenshtein typo correction, next-command suggestion (static, not live — see below) |
| `src/docs.h#` | `docs.rs` | `help <topic>` pages |
| `src/settings.h#` | `settings.rs` | line-based theme picker (the original's own fallback for no-raw-mode terminals — here it's the *only* picker, since H# has no raw mode at all) |
| `src/helper.h#` | `helper.rs` | wires `hprompt`'s tab-completion word list (builtins + aliases + cached PATH commands) |
| `src/execute.h#` | `execute.rs` **+** `script.rs` **+** `builtins.rs` | command dispatch, **full `if/elif/else/fi`, `while/do/done`, `until/do/done`, `for/do/done`, `function`/`break`/`continue`/`return` script interpreter**, and every builtin |

`builtins_native.rs` has no file here on purpose — its reimplementations
of `ls`/`cat`/`grep`/etc. aren't needed; those already work correctly
through `execute.h#`'s external-command path using the real system
binaries.

## Why `execute.rs` + `script.rs` + `builtins.rs` became one file

They're mutually recursive in the original (script bodies run
commands via `execute::run`; `source`/`.` needs the script parser;
builtins need the same `Shell` state execute.rs defines) — and H#'s
`use` statement currently only resolves `"std -> x"` (see below); there
is no working mechanism for two local project files to `use` each
other, let alone in a genuinely circular shape. One combined file
sidesteps the problem instead of fighting a toolchain limitation.

## Status: this is a real, tested control-flow interpreter

Unlike the single-file version delivered earlier in this project,
`execute.h#` now includes a **complete port of `script.rs`**: `if`/
`elif`/`else`/`fi`, `while`/`do`/`done`, `until`/`do`/`done`, `for VAR
in words; do`/`done`, `function NAME { }` and `NAME() { }`, `break`,
`continue`, `return [code]` — tested with nested loops, `elif` chains,
functions with positional parameters, and (importantly) **typed
interactively at the live prompt**, not just in sourced files: `main.h#`
detects an unclosed block by keyword-nesting and keeps reading
continuation lines (`> `) until it balances, then runs the whole thing
as one parsed block — the same code path `source`/`.` uses for a file.
Only genuinely not ported: C-style `for ((init;cond;update))` and
`case`/`esac` (real omissions — those lines run as a literal, usually-
failing command instead).

**Condition evaluation** (`if COND`, `while COND`) needed a real exit
code, which `process::run()` doesn't provide — `execute.h#` recovers
one by appending an invisible `printf` marker after the command and
parsing it back out of the captured output before ever showing it to
the user.

## `use "bytes -> hprompt"` — status, same as before

Confirmed directly against the H# compiler/interpreter source:
`ImportKind::BytesRepo` is produced by the parser but never consumed
by either backend (only `ImportKind::Std` is handled). `src/main.h#`
keeps `use "bytes -> hprompt"` exactly as requested — it's the
correct, forward-compatible way to depend on a published `bytes`
package — but **won't resolve anything** until that ships. Everything
in this project was tested by temporarily swapping that one line for
`use "std -> hprompt" from "hp"` (after installing `hprompt`'s
`lib.h#` as `/usr/lib/HackerOS/H#/std/hprompt.h#`), then swapping it
back before delivery. `install.sh` sets up every file for that same
local-testing path — see its own comments.

## Left out / limited, with the specific reason

| Feature | Status | Why |
|---|---|---|
| Real job control (`fg`/`bg` actually taking the terminal, `stop`/`SIGSTOP` reliably reaching the real work) | Best-effort | No `waitpid`/process-group/`tcsetpgrp` in H#. Additionally: `process::spawn(cmd)` returns the PID of the `sh -c cmd` **wrapper**, not of whatever `cmd` execs to — confirmed by tracing the process tree (a plain external command gets forked as a *child* of that wrapper, not exec'd over it), so killing the returned PID doesn't reliably stop the real work. Documented in `jobs.h#`'s own header. |
| Pipes/redirects implemented by hsh itself | Delegated | No fork/exec/dup2/pipe in H#. Any non-builtin, non-function line is handed whole to `process::run()` (a real `sh -c`), so pipes/redirects/globbing/`&&`/`;`/subshells all work — just via the system shell. |
| `case`/`esac`, C-style `for ((;;))` | Not ported | Real time/scope cut, not a silent approximation — noted above. |
| Live syntax highlighting, ghost-text hints, raw-mode settings TUI | Not possible | No raw terminal mode in H# yet (see hprompt's README). `smarthints.h#`'s typo-correction and next-command suggestion are shown statically (after the fact / once before the prompt) instead of live. |
| Git info | Synchronous per prompt render | No background task primitive in H#; the original polls a cache from a background thread. Costs a few `git` invocations per prompt inside a repo — acceptable for interactive use, not free. |

## Porting notes — interpreter quirks found building this

Beyond what was already documented for the single-file version and
`hprompt`, building this multi-file, much larger project surfaced
several more, all with minimal repros kept in this project's
development notes:

- **The `if`-branch-corrupts-a-later-struct-return bug generalizes
  further than first thought**: it's not limited to `while`-loop
  bodies. In a *plain function*, an `if` (with or without `else`)
  whose branch runs, followed *anywhere later in that same function*
  by a `return` of — or any read of a field on — a `mut` struct/
  hashmap parameter, can make that value read back as `nil`, even if
  the `if`'s own body never touches the parameter at all. The fix
  used everywhere in this project: push any conditional side effect
  that needs to happen right before touching the parameter again down
  into a small helper that takes no such parameter itself (see
  `hsh_util::write_raw`, `hsh_config::ensure_hshrc_exists`,
  `hsh_security::dangerous_and_declined`-shaped helpers throughout),
  or duplicate the `return` into every branch that could run before it.
- **A `mut` struct/hashmap **function parameter** doesn't get the
  in-place-mutation behavior a same-scope local variable gets** —
  `fn f(mut x: Foo) is x.field.insert(...) end` silently mutates
  nothing at all unless `x` is first rebound (`let mut y = x`) or the
  parameter itself is declared `mut` in the signature (the latter is
  what this project uses throughout) — confirmed by direct test.
- Even a **local, non-`mut` `let` variable** holding a native
  `hashmap_new()` value doesn't get mutated by `.insert()` — it must
  be `let mut`, even within a single function/scope with no call
  boundary involved at all.
- **`const` declarations are silently dropped** when a file is loaded
  as a `use "std -> x"` module (the loader only registers `fn`/
  `struct`/inline `mod` — not `const`) — every "constant" in this
  project is a zero-arg function instead (`fn cache_ttl_secs() -> int
  is return 300 end`).
- **Bare function names collide *across every simultaneously-loaded
  module*, not just within one file** — if two different `use "std ->
  x"` files each define a function with the same bare name (even for
  unrelated purposes), whichever loads last silently overwrites the
  binding for *both*, including breaking the *other* module's own
  internal unqualified calls to its own function. This is how
  `hsh_history.h#`'s original `history_load`/`history_add`/`history_
  save`/`history_len` broke `hprompt`'s own same-named functions
  merely by both being loaded in the same program — traced via a
  confusing, seemingly-unrelated crash deep inside `hprompt`'s own
  code. Renamed to `rich_history_*` throughout. Combined with the
  already-known "never name a function `{importalias}_{name}`" rule,
  the real rule is: **audit every function name against every other
  simultaneously-loaded module's names, not just against this file's
  own imports.**
- A literal `{word}` pair inside any double-quoted string —
  including ones that don't look like shell syntax, like a regex
  quantifier (`"...{16}"`) or a doc string mentioning `function name {
  }` — gets scanned for interpolation and can silently swallow itself
  (parses as an integer/expression, not always a hard error). Escaped
  as `"{{16}}"` throughout, or broken up with `+` concatenation.

## Trying it

```sh
./install.sh
# then, per the script's own note, swap the one `use "bytes -> ..."`
# line in src/main.h# for `use "std -> hprompt" from "hp"` until
# bytes-repo resolution ships
h# run src/main.h#
```

On first run, hsh creates `~/.hshrc` (all 7 sections, with example
aliases) and starts empty history files.

## License

Same as the original hsh.
