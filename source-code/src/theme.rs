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
            hint_color:     "\x1b[38;5;236m".into(),  // bardzo ciemny (ledwo widoczny)
            cmd_ok_color:   "\x1b[38;5;114m".into(),  // miętowy
            cmd_err_color:  "\x1b[38;5;203m".into(),  // łososiowy
            flag_color:     "\x1b[38;5;179m".into(),  // złoty
            string_color:   "\x1b[38;5;150m".into(),  // jasna zieleń
            var_color:      "\x1b[38;5;110m".into(),  // stalowy błękit
            op_color:       "\x1b[38;5;242m".into(),  // szary — operatory nie krzykliwe
            path_color:     "\x1b[38;5;73m".into(),   // stalowy cyan
            sep:            "\x1b[38;5;238m\x1b[0m".into(), // niewidoczny separator
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
            prompt_ok:      "\x1b[38;5;109m".into(),  // nord teal
            prompt_err:     "\x1b[38;5;167m".into(),  // nord red
            dir_color:      "\x1b[38;5;117m".into(),  // nord frost
            git_color:      "\x1b[38;5;143m".into(),  // nord yellow-green
            time_color:     "\x1b[38;5;103m".into(),  // nord comment
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
            prompt_ok:      "\x1b[38;5;142m".into(),  // gruvbox green
            prompt_err:     "\x1b[38;5;167m".into(),  // gruvbox red
            dir_color:      "\x1b[38;5;214m".into(),  // gruvbox orange
            git_color:      "\x1b[38;5;108m".into(),  // gruvbox aqua
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
            prompt_ok:      "\x1b[38;5;84m".into(),   // dracula green
            prompt_err:     "\x1b[38;5;203m".into(),  // dracula red
            dir_color:      "\x1b[38;5;183m".into(),  // dracula purple
            git_color:      "\x1b[38;5;159m".into(),  // dracula cyan
            time_color:     "\x1b[38;5;102m".into(),
            hint_color:     "\x1b[38;5;238m".into(),
            cmd_ok_color:   "\x1b[38;5;84m".into(),
            cmd_err_color:  "\x1b[38;5;203m".into(),
            flag_color:     "\x1b[38;5;228m".into(),
            string_color:   "\x1b[38;5;228m".into(),  // dracula yellow
            var_color:      "\x1b[38;5;183m".into(),
            op_color:       "\x1b[38;5;212m".into(),  // dracula pink
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
            prompt_ok:      "\x1b[38;5;46m".into(),   // matrix green
            prompt_err:     "\x1b[38;5;196m".into(),
            dir_color:      "\x1b[38;5;40m".into(),   // dark green
            git_color:      "\x1b[38;5;34m".into(),
            time_color:     "\x1b[38;5;28m".into(),
            hint_color:     "\x1b[38;5;22m".into(),   // bardzo ciemna zieleń
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
            "default"  => Some(Theme::default_theme()),
            "cosmic"   => Some(Theme::cosmic_theme()),
            "nord"     => Some(Theme::nord_theme()),
            "gruvbox"  => Some(Theme::gruvbox_theme()),
            "dracula"  => Some(Theme::dracula_theme()),
            "hackeros" => Some(Theme::hackeros_theme()),
            _          => None,
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
        ]
    }
}

fn theme_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    PathBuf::from(format!("{}/.config/hackeros/hsh/theme.json", home))
}
