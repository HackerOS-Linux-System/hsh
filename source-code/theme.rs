use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name:           String,
    pub prompt_char:    String,
    pub prompt_ok:      String,
    pub prompt_err:     String,
    pub dir_color:      String,
    pub git_color:      String,
    pub time_color:     String,
    pub hint_color:     String,
    pub cmd_ok_color:   String,
    pub cmd_err_color:  String,
    pub flag_color:     String,
    pub string_color:   String,
    pub var_color:      String,
    pub op_color:       String,
    pub path_color:     String,
    pub sep:            String,
    pub duration_color: String,
    pub error_color:    String,
}

impl Default for Theme {
    fn default() -> Self { Theme::default_theme() }
}

impl Theme {
    // ── 1. default — stonowany, czytelny, nowoczesny ──────────────────────────
    pub fn default_theme() -> Self {
        Theme {
            name:           "default".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;114m".into(),  // miętowy zielony
            prompt_err:     "\x1b[38;5;203m".into(),  // łososiowy czerwony
            dir_color:      "\x1b[38;5;110m".into(),  // stalowy błękit
            git_color:      "\x1b[38;5;179m".into(),  // złoty
            time_color:     "\x1b[38;5;242m".into(),  // ciemny szary
            hint_color:     "\x1b[38;5;236m".into(),  // bardzo ciemny
            cmd_ok_color:   "\x1b[38;5;114m".into(),  // miętowy
            cmd_err_color:  "\x1b[38;5;203m".into(),  // łososiowy
            flag_color:     "\x1b[38;5;179m".into(),  // złoty
            string_color:   "\x1b[38;5;150m".into(),  // jasna zieleń
            var_color:      "\x1b[38;5;110m".into(),  // stalowy błękit
            op_color:       "\x1b[38;5;242m".into(),  // szary
            path_color:     "\x1b[38;5;73m".into(),   // stalowy cyan
            sep:            "\x1b[38;5;238m\x1b[0m".into(),
            duration_color: "\x1b[38;5;242m".into(),
            error_color:    "\x1b[38;5;203m".into(),
        }
    }

    // ── 2. cosmic — cyberpunk fiolet/cyan ─────────────────────────────────────
    pub fn cosmic_theme() -> Self {
        Theme {
            name:           "cosmic".into(),
            prompt_char:    "⟩".into(),
            prompt_ok:      "\x1b[38;5;51m".into(),
            prompt_err:     "\x1b[38;5;213m".into(),
            dir_color:      "\x1b[38;5;141m".into(),
            git_color:      "\x1b[38;5;51m".into(),
            time_color:     "\x1b[38;5;105m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;51m".into(),
            cmd_err_color:  "\x1b[38;5;213m".into(),
            flag_color:     "\x1b[38;5;228m".into(),
            string_color:   "\x1b[38;5;213m".into(),
            var_color:      "\x1b[38;5;123m".into(),
            op_color:       "\x1b[38;5;141m".into(),
            path_color:     "\x1b[38;5;87m".into(),
            sep:            "\x1b[38;5;57m\x1b[0m".into(),
            duration_color: "\x1b[38;5;105m".into(),
            error_color:    "\x1b[38;5;213m".into(),
        }
    }

    // ── 3. nord — zimny, skandynawski błękit ──────────────────────────────────
    pub fn nord_theme() -> Self {
        Theme {
            name:           "nord".into(),
            prompt_char:    "→".into(),
            prompt_ok:      "\x1b[38;5;109m".into(),
            prompt_err:     "\x1b[38;5;167m".into(),
            dir_color:      "\x1b[38;5;117m".into(),
            git_color:      "\x1b[38;5;143m".into(),
            time_color:     "\x1b[38;5;103m".into(),
            hint_color:     "\x1b[38;5;239m".into(),
            cmd_ok_color:   "\x1b[38;5;109m".into(),
            cmd_err_color:  "\x1b[38;5;167m".into(),
            flag_color:     "\x1b[38;5;143m".into(),
            string_color:   "\x1b[38;5;108m".into(),
            var_color:      "\x1b[38;5;153m".into(),
            op_color:       "\x1b[38;5;103m".into(),
            path_color:     "\x1b[38;5;116m".into(),
            sep:            "\x1b[38;5;238m\x1b[0m".into(),
            duration_color: "\x1b[38;5;103m".into(),
            error_color:    "\x1b[38;5;167m".into(),
        }
    }

    // ── 4. gruvbox — ciepły retro amber ───────────────────────────────────────
    pub fn gruvbox_theme() -> Self {
        Theme {
            name:           "gruvbox".into(),
            prompt_char:    "▸".into(),
            prompt_ok:      "\x1b[38;5;142m".into(),
            prompt_err:     "\x1b[38;5;167m".into(),
            dir_color:      "\x1b[38;5;214m".into(),
            git_color:      "\x1b[38;5;108m".into(),
            time_color:     "\x1b[38;5;243m".into(),
            hint_color:     "\x1b[38;5;239m".into(),
            cmd_ok_color:   "\x1b[38;5;142m".into(),
            cmd_err_color:  "\x1b[38;5;167m".into(),
            flag_color:     "\x1b[38;5;214m".into(),
            string_color:   "\x1b[38;5;108m".into(),
            var_color:      "\x1b[38;5;214m".into(),
            op_color:       "\x1b[38;5;243m".into(),
            path_color:     "\x1b[38;5;108m".into(),
            sep:            "\x1b[38;5;239m\x1b[0m".into(),
            duration_color: "\x1b[38;5;243m".into(),
            error_color:    "\x1b[38;5;167m".into(),
        }
    }

    // ── 5. dracula — ciemny fiolet/różowy ─────────────────────────────────────
    pub fn dracula_theme() -> Self {
        Theme {
            name:           "dracula".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;84m".into(),
            prompt_err:     "\x1b[38;5;203m".into(),
            dir_color:      "\x1b[38;5;183m".into(),
            git_color:      "\x1b[38;5;159m".into(),
            time_color:     "\x1b[38;5;102m".into(),
            hint_color:     "\x1b[38;5;238m".into(),
            cmd_ok_color:   "\x1b[38;5;84m".into(),
            cmd_err_color:  "\x1b[38;5;203m".into(),
            flag_color:     "\x1b[38;5;228m".into(),
            string_color:   "\x1b[38;5;228m".into(),
            var_color:      "\x1b[38;5;183m".into(),
            op_color:       "\x1b[38;5;212m".into(),
            path_color:     "\x1b[38;5;159m".into(),
            sep:            "\x1b[38;5;61m\x1b[0m".into(),
            duration_color: "\x1b[38;5;102m".into(),
            error_color:    "\x1b[38;5;203m".into(),
        }
    }

    // ── 6. hackeros — matrix zielony na czarnym ────────────────────────────────
    pub fn hackeros_theme() -> Self {
        Theme {
            name:           "hackeros".into(),
            prompt_char:    "#".into(),
            prompt_ok:      "\x1b[38;5;46m".into(),
            prompt_err:     "\x1b[38;5;196m".into(),
            dir_color:      "\x1b[38;5;40m".into(),
            git_color:      "\x1b[38;5;34m".into(),
            time_color:     "\x1b[38;5;28m".into(),
            hint_color:     "\x1b[38;5;22m".into(),
            cmd_ok_color:   "\x1b[38;5;46m".into(),
            cmd_err_color:  "\x1b[38;5;196m".into(),
            flag_color:     "\x1b[38;5;118m".into(),
            string_color:   "\x1b[38;5;82m".into(),
            var_color:      "\x1b[38;5;154m".into(),
            op_color:       "\x1b[38;5;40m".into(),
            path_color:     "\x1b[38;5;76m".into(),
            sep:            "\x1b[38;5;22m\x1b[0m".into(),
            duration_color: "\x1b[38;5;28m".into(),
            error_color:    "\x1b[38;5;196m".into(),
        }
    }

    // ── 7. solarized-light ─────────────────────────────────────────────────────
    pub fn solarized_light() -> Self {
        Theme {
            name:           "solarized-light".into(),
            prompt_char:    "➤".into(),
            prompt_ok:      "\x1b[38;5;108m".into(),
            prompt_err:     "\x1b[38;5;160m".into(),
            dir_color:      "\x1b[38;5;33m".into(),
            git_color:      "\x1b[38;5;136m".into(),
            time_color:     "\x1b[38;5;102m".into(),
            hint_color:     "\x1b[38;5;250m".into(),
            cmd_ok_color:   "\x1b[38;5;108m".into(),
            cmd_err_color:  "\x1b[38;5;160m".into(),
            flag_color:     "\x1b[38;5;136m".into(),
            string_color:   "\x1b[38;5;142m".into(),
            var_color:      "\x1b[38;5;33m".into(),
            op_color:       "\x1b[38;5;240m".into(),
            path_color:     "\x1b[38;5;66m".into(),
            sep:            "\x1b[38;5;248m\x1b[0m".into(),
            duration_color: "\x1b[38;5;102m".into(),
            error_color:    "\x1b[38;5;160m".into(),
        }
    }

    // ── 8. solarized-dark ──────────────────────────────────────────────────────
    pub fn solarized_dark() -> Self {
        Theme {
            name:           "solarized-dark".into(),
            prompt_char:    "➤".into(),
            prompt_ok:      "\x1b[38;5;108m".into(),
            prompt_err:     "\x1b[38;5;160m".into(),
            dir_color:      "\x1b[38;5;33m".into(),
            git_color:      "\x1b[38;5;136m".into(),
            time_color:     "\x1b[38;5;102m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;108m".into(),
            cmd_err_color:  "\x1b[38;5;160m".into(),
            flag_color:     "\x1b[38;5;136m".into(),
            string_color:   "\x1b[38;5;142m".into(),
            var_color:      "\x1b[38;5;33m".into(),
            op_color:       "\x1b[38;5;245m".into(),
            path_color:     "\x1b[38;5;66m".into(),
            sep:            "\x1b[38;5;236m\x1b[0m".into(),
            duration_color: "\x1b[38;5;102m".into(),
            error_color:    "\x1b[38;5;160m".into(),
        }
    }

    // ── 9. tomorrow-night ──────────────────────────────────────────────────────
    pub fn tomorrow_night() -> Self {
        Theme {
            name:           "tomorrow-night".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;114m".into(),
            prompt_err:     "\x1b[38;5;203m".into(),
            dir_color:      "\x1b[38;5;111m".into(),
            git_color:      "\x1b[38;5;185m".into(),
            time_color:     "\x1b[38;5;244m".into(),
            hint_color:     "\x1b[38;5;236m".into(),
            cmd_ok_color:   "\x1b[38;5;114m".into(),
            cmd_err_color:  "\x1b[38;5;203m".into(),
            flag_color:     "\x1b[38;5;185m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;111m".into(),
            op_color:       "\x1b[38;5;242m".into(),
            path_color:     "\x1b[38;5;110m".into(),
            sep:            "\x1b[38;5;238m\x1b[0m".into(),
            duration_color: "\x1b[38;5;242m".into(),
            error_color:    "\x1b[38;5;203m".into(),
        }
    }

    // ── 10. monokai ────────────────────────────────────────────────────────────
    pub fn monokai() -> Self {
        Theme {
            name:           "monokai".into(),
            prompt_char:    "»".into(),
            prompt_ok:      "\x1b[38;5;148m".into(),
            prompt_err:     "\x1b[38;5;197m".into(),
            dir_color:      "\x1b[38;5;81m".into(),
            git_color:      "\x1b[38;5;185m".into(),
            time_color:     "\x1b[38;5;245m".into(),
            hint_color:     "\x1b[38;5;238m".into(),
            cmd_ok_color:   "\x1b[38;5;148m".into(),
            cmd_err_color:  "\x1b[38;5;197m".into(),
            flag_color:     "\x1b[38;5;185m".into(),
            string_color:   "\x1b[38;5;148m".into(),
            var_color:      "\x1b[38;5;81m".into(),
            op_color:       "\x1b[38;5;246m".into(),
            path_color:     "\x1b[38;5;141m".into(),
            sep:            "\x1b[38;5;237m\x1b[0m".into(),
            duration_color: "\x1b[38;5;245m".into(),
            error_color:    "\x1b[38;5;197m".into(),
        }
    }

    // ── 11. ayu-dark ───────────────────────────────────────────────────────────
    pub fn ayu_dark() -> Self {
        Theme {
            name:           "ayu-dark".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;150m".into(),
            prompt_err:     "\x1b[38;5;210m".into(),
            dir_color:      "\x1b[38;5;111m".into(),
            git_color:      "\x1b[38;5;179m".into(),
            time_color:     "\x1b[38;5;243m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;150m".into(),
            cmd_err_color:  "\x1b[38;5;210m".into(),
            flag_color:     "\x1b[38;5;179m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;111m".into(),
            op_color:       "\x1b[38;5;245m".into(),
            path_color:     "\x1b[38;5;146m".into(),
            sep:            "\x1b[38;5;238m\x1b[0m".into(),
            duration_color: "\x1b[38;5;243m".into(),
            error_color:    "\x1b[38;5;210m".into(),
        }
    }

    // ── 12. ayu-light ──────────────────────────────────────────────────────────
    pub fn ayu_light() -> Self {
        Theme {
            name:           "ayu-light".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;142m".into(),
            prompt_err:     "\x1b[38;5;160m".into(),
            dir_color:      "\x1b[38;5;32m".into(),
            git_color:      "\x1b[38;5;136m".into(),
            time_color:     "\x1b[38;5;102m".into(),
            hint_color:     "\x1b[38;5;248m".into(),
            cmd_ok_color:   "\x1b[38;5;142m".into(),
            cmd_err_color:  "\x1b[38;5;160m".into(),
            flag_color:     "\x1b[38;5;136m".into(),
            string_color:   "\x1b[38;5;142m".into(),
            var_color:      "\x1b[38;5;32m".into(),
            op_color:       "\x1b[38;5;243m".into(),
            path_color:     "\x1b[38;5;66m".into(),
            sep:            "\x1b[38;5;250m\x1b[0m".into(),
            duration_color: "\x1b[38;5;102m".into(),
            error_color:    "\x1b[38;5;160m".into(),
        }
    }

    // ── 13. one-dark ───────────────────────────────────────────────────────────
    pub fn one_dark() -> Self {
        Theme {
            name:           "one-dark".into(),
            prompt_char:    "$".into(),
            prompt_ok:      "\x1b[38;5;114m".into(),
            prompt_err:     "\x1b[38;5;203m".into(),
            dir_color:      "\x1b[38;5;110m".into(),
            git_color:      "\x1b[38;5;179m".into(),
            time_color:     "\x1b[38;5;242m".into(),
            hint_color:     "\x1b[38;5;236m".into(),
            cmd_ok_color:   "\x1b[38;5;114m".into(),
            cmd_err_color:  "\x1b[38;5;203m".into(),
            flag_color:     "\x1b[38;5;179m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;110m".into(),
            op_color:       "\x1b[38;5;242m".into(),
            path_color:     "\x1b[38;5;73m".into(),
            sep:            "\x1b[38;5;238m\x1b[0m".into(),
            duration_color: "\x1b[38;5;242m".into(),
            error_color:    "\x1b[38;5;203m".into(),
        }
    }

    // ── 14. catppuccin-mocha ───────────────────────────────────────────────────
    pub fn catppuccin_mocha() -> Self {
        Theme {
            name:           "catppuccin-mocha".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;84m".into(),
            prompt_err:     "\x1b[38;5;210m".into(),
            dir_color:      "\x1b[38;5;111m".into(),
            git_color:      "\x1b[38;5;216m".into(),
            time_color:     "\x1b[38;5;245m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;84m".into(),
            cmd_err_color:  "\x1b[38;5;210m".into(),
            flag_color:     "\x1b[38;5;216m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;111m".into(),
            op_color:       "\x1b[38;5;243m".into(),
            path_color:     "\x1b[38;5;147m".into(),
            sep:            "\x1b[38;5;236m\x1b[0m".into(),
            duration_color: "\x1b[38;5;245m".into(),
            error_color:    "\x1b[38;5;210m".into(),
        }
    }

    // ── 15. everforest ─────────────────────────────────────────────────────────
    pub fn everforest() -> Self {
        Theme {
            name:           "everforest".into(),
            prompt_char:    "➜".into(),
            prompt_ok:      "\x1b[38;5;108m".into(),
            prompt_err:     "\x1b[38;5;167m".into(),
            dir_color:      "\x1b[38;5;109m".into(),
            git_color:      "\x1b[38;5;142m".into(),
            time_color:     "\x1b[38;5;245m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;108m".into(),
            cmd_err_color:  "\x1b[38;5;167m".into(),
            flag_color:     "\x1b[38;5;142m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;109m".into(),
            op_color:       "\x1b[38;5;245m".into(),
            path_color:     "\x1b[38;5;109m".into(),
            sep:            "\x1b[38;5;236m\x1b[0m".into(),
            duration_color: "\x1b[38;5;245m".into(),
            error_color:    "\x1b[38;5;167m".into(),
        }
    }

    // ── 16. tokyo-night ────────────────────────────────────────────────────────
    pub fn tokyo_night() -> Self {
        Theme {
            name:           "tokyo-night".into(),
            prompt_char:    "❯".into(),
            prompt_ok:      "\x1b[38;5;114m".into(),
            prompt_err:     "\x1b[38;5;203m".into(),
            dir_color:      "\x1b[38;5;111m".into(),
            git_color:      "\x1b[38;5;179m".into(),
            time_color:     "\x1b[38;5;245m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;114m".into(),
            cmd_err_color:  "\x1b[38;5;203m".into(),
            flag_color:     "\x1b[38;5;179m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;111m".into(),
            op_color:       "\x1b[38;5;242m".into(),
            path_color:     "\x1b[38;5;146m".into(),
            sep:            "\x1b[38;5;236m\x1b[0m".into(),
            duration_color: "\x1b[38;5;245m".into(),
            error_color:    "\x1b[38;5;203m".into(),
        }
    }

    // ── 17. kanagawa ───────────────────────────────────────────────────────────
    pub fn kanagawa() -> Self {
        Theme {
            name:           "kanagawa".into(),
            prompt_char:    "λ".into(),
            prompt_ok:      "\x1b[38;5;150m".into(),
            prompt_err:     "\x1b[38;5;210m".into(),
            dir_color:      "\x1b[38;5;110m".into(),
            git_color:      "\x1b[38;5;179m".into(),
            time_color:     "\x1b[38;5;245m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;150m".into(),
            cmd_err_color:  "\x1b[38;5;210m".into(),
            flag_color:     "\x1b[38;5;179m".into(),
            string_color:   "\x1b[38;5;150m".into(),
            var_color:      "\x1b[38;5;110m".into(),
            op_color:       "\x1b[38;5;243m".into(),
            path_color:     "\x1b[38;5;146m".into(),
            sep:            "\x1b[38;5;236m\x1b[0m".into(),
            duration_color: "\x1b[38;5;245m".into(),
            error_color:    "\x1b[38;5;210m".into(),
        }
    }

    // ── 18. rose-pine ──────────────────────────────────────────────────────────
    pub fn rose_pine() -> Self {
        Theme {
            name:           "rose-pine".into(),
            prompt_char:    "🌹".into(),
            prompt_ok:      "\x1b[38;5;211m".into(),
            prompt_err:     "\x1b[38;5;203m".into(),
            dir_color:      "\x1b[38;5;183m".into(),
            git_color:      "\x1b[38;5;216m".into(),
            time_color:     "\x1b[38;5;247m".into(),
            hint_color:     "\x1b[38;5;237m".into(),
            cmd_ok_color:   "\x1b[38;5;211m".into(),
            cmd_err_color:  "\x1b[38;5;203m".into(),
            flag_color:     "\x1b[38;5;216m".into(),
            string_color:   "\x1b[38;5;217m".into(),
            var_color:      "\x1b[38;5;183m".into(),
            op_color:       "\x1b[38;5;245m".into(),
            path_color:     "\x1b[38;5;147m".into(),
            sep:            "\x1b[38;5;236m\x1b[0m".into(),
            duration_color: "\x1b[38;5;247m".into(),
            error_color:    "\x1b[38;5;203m".into(),
        }
    }

    // ── 19. cyberpunk ──────────────────────────────────────────────────────────
    pub fn cyberpunk() -> Self {
        Theme {
            name:           "cyberpunk".into(),
            prompt_char:    "⚡".into(),
            prompt_ok:      "\x1b[38;5;201m".into(),
            prompt_err:     "\x1b[38;5;198m".into(),
            dir_color:      "\x1b[38;5;51m".into(),
            git_color:      "\x1b[38;5;201m".into(),
            time_color:     "\x1b[38;5;129m".into(),
            hint_color:     "\x1b[38;5;235m".into(),
            cmd_ok_color:   "\x1b[38;5;51m".into(),
            cmd_err_color:  "\x1b[38;5;198m".into(),
            flag_color:     "\x1b[38;5;201m".into(),
            string_color:   "\x1b[38;5;213m".into(),
            var_color:      "\x1b[38;5;51m".into(),
            op_color:       "\x1b[38;5;201m".into(),
            path_color:     "\x1b[38;5;87m".into(),
            sep:            "\x1b[38;5;57m\x1b[0m".into(),
            duration_color: "\x1b[38;5;129m".into(),
            error_color:    "\x1b[38;5;198m".into(),
        }
    }

    // ── 20. nord-light (wersja jasna) ──────────────────────────────────────────
    pub fn nord_light() -> Self {
        Theme {
            name:           "nord-light".into(),
            prompt_char:    "→".into(),
            prompt_ok:      "\x1b[38;5;108m".into(),
            prompt_err:     "\x1b[38;5;167m".into(),
            dir_color:      "\x1b[38;5;66m".into(),
            git_color:      "\x1b[38;5;136m".into(),
            time_color:     "\x1b[38;5;102m".into(),
            hint_color:     "\x1b[38;5;250m".into(),
            cmd_ok_color:   "\x1b[38;5;108m".into(),
            cmd_err_color:  "\x1b[38;5;167m".into(),
            flag_color:     "\x1b[38;5;136m".into(),
            string_color:   "\x1b[38;5;108m".into(),
            var_color:      "\x1b[38;5;66m".into(),
            op_color:       "\x1b[38;5;245m".into(),
            path_color:     "\x1b[38;5;66m".into(),
            sep:            "\x1b[38;5;250m\x1b[0m".into(),
            duration_color: "\x1b[38;5;102m".into(),
            error_color:    "\x1b[38;5;167m".into(),
        }
    }

    // ── Metody zarządzania ─────────────────────────────────────────────────────
    pub fn load() -> Self {
        let path = theme_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(t) = serde_json::from_str::<Theme>(&data) {
                    return t;
                }
            }
        }
        Theme::default_theme()
    }

    pub fn save(&self) {
        let path = theme_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, data);
        }
    }

    pub fn by_name(name: &str) -> Option<Theme> {
        match name {
            "default"          => Some(Theme::default_theme()),
            "cosmic"           => Some(Theme::cosmic_theme()),
            "nord"             => Some(Theme::nord_theme()),
            "gruvbox"          => Some(Theme::gruvbox_theme()),
            "dracula"          => Some(Theme::dracula_theme()),
            "hackeros"         => Some(Theme::hackeros_theme()),
            "solarized-light"  => Some(Theme::solarized_light()),
            "solarized-dark"   => Some(Theme::solarized_dark()),
            "tomorrow-night"   => Some(Theme::tomorrow_night()),
            "monokai"          => Some(Theme::monokai()),
            "ayu-dark"         => Some(Theme::ayu_dark()),
            "ayu-light"        => Some(Theme::ayu_light()),
            "one-dark"         => Some(Theme::one_dark()),
            "catppuccin-mocha" => Some(Theme::catppuccin_mocha()),
            "everforest"       => Some(Theme::everforest()),
            "tokyo-night"      => Some(Theme::tokyo_night()),
            "kanagawa"         => Some(Theme::kanagawa()),
            "rose-pine"        => Some(Theme::rose_pine()),
            "cyberpunk"        => Some(Theme::cyberpunk()),
            "nord-light"       => Some(Theme::nord_light()),
            _ => None,
        }
    }

    pub fn all_themes() -> Vec<Theme> {
        vec![
            Theme::default_theme(),
            Theme::cosmic_theme(),
            Theme::nord_theme(),
            Theme::gruvbox_theme(),
            Theme::dracula_theme(),
            Theme::hackeros_theme(),
            Theme::solarized_light(),
            Theme::solarized_dark(),
            Theme::tomorrow_night(),
            Theme::monokai(),
            Theme::ayu_dark(),
            Theme::ayu_light(),
            Theme::one_dark(),
            Theme::catppuccin_mocha(),
            Theme::everforest(),
            Theme::tokyo_night(),
            Theme::kanagawa(),
            Theme::rose_pine(),
            Theme::cyberpunk(),
            Theme::nord_light(),
        ]
    }
}

fn theme_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(format!("{}/.config/hackeros/hsh/theme.json", home))
}
