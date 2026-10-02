//! What the app remembers between starts (PLAN-002): the zoom, the theme,
//! the window's size, the most recent files, where the side by side
//! divider was (PLAN-003), the vault and whether the sidebar shows. One `key = value` a line,
//! parsed by hand (no crate for it); unknown, misspelled or broken lines are
//! ignored, so a hand edit never stops the app from starting.
use std::path::{Path, PathBuf};

/// How many recent files are kept.
const RECENT: usize = 10;

/// The theme: as the system is (between a light and a dark one picked),
/// or one of iced's (PLAN-006).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
    KanagawaWave,
    KanagawaLotus,
    SolarizedLight,
    SolarizedDark,
    GruvboxLight,
    GruvboxDark,
    CatppuccinMocha,
    CatppuccinFrappe,
}

impl Theme {
    /// Every theme but System, light ones first.
    pub const NAMED: [Theme; 10] = [
        Theme::Light,
        Theme::KanagawaLotus,
        Theme::SolarizedLight,
        Theme::GruvboxLight,
        Theme::Dark,
        Theme::KanagawaWave,
        Theme::SolarizedDark,
        Theme::GruvboxDark,
        Theme::CatppuccinMocha,
        Theme::CatppuccinFrappe,
    ];

    /// Its name as shown, and as kept in the settings file.
    pub fn name(self) -> &'static str {
        match self {
            Theme::System => "System",
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::KanagawaWave => "Kanagawa Wave",
            Theme::KanagawaLotus => "Kanagawa Lotus",
            Theme::SolarizedLight => "Solarized Light",
            Theme::SolarizedDark => "Solarized Dark",
            Theme::GruvboxLight => "Gruvbox Light",
            Theme::GruvboxDark => "Gruvbox Dark",
            Theme::CatppuccinMocha => "Catppuccin Mocha",
            Theme::CatppuccinFrappe => "Catppuccin Frappe",
        }
    }

    pub fn is_dark(self) -> bool {
        !matches!(
            self,
            Theme::System
                | Theme::Light
                | Theme::KanagawaLotus
                | Theme::SolarizedLight
                | Theme::GruvboxLight
        )
    }

    /// The theme named `name` (any case, spaces or hyphens).
    fn named(name: &str) -> Option<Theme> {
        let wanted = name.trim().to_lowercase().replace('-', " ");
        std::iter::once(Theme::System)
            .chain(Theme::NAMED)
            .find(|theme| theme.name().to_lowercase() == wanted)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The note's text size (`Editor::set_zoom`).
    pub zoom: f32,
    /// The whole interface's scale (PLAN-006).
    pub scale: f32,
    pub theme: Theme,
    /// What System follows by day and by night.
    pub light: Theme,
    pub dark: Theme,
    /// The window's size when it was last closed.
    pub window: Option<(f32, f32)>,
    /// Most recent first.
    pub recent: Vec<PathBuf>,
    /// The markdown's share of the width side by side.
    pub split: f32,
    /// The vault last opened (PLAN-004).
    pub vault: Option<PathBuf>,
    /// Whether the sidebar shows (Ctrl+\ hides it, PLAN-006).
    pub sidebar: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            scale: 1.0,
            theme: Theme::System,
            light: Theme::Light,
            dark: Theme::Dark,
            window: None,
            recent: Vec::new(),
            split: 0.5,
            vault: None,
            sidebar: true,
        }
    }
}

impl Settings {
    /// Where they are kept: `$XDG_CONFIG_HOME/livemark/settings` (or
    /// `~/.config/...`) on Linux, `~/Library/Application Support/livemark/
    /// settings` on macOS.
    pub fn path() -> Option<PathBuf> {
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(target_os = "macos") {
            home()?.join("Library/Application Support")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| Some(home()?.join(".config")))?
        };
        Some(base.join("livemark").join("settings"))
    }

    /// The settings in the file at `path`, or the defaults when it is
    /// missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Writes them to `path`, making its folder.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(path, self.render())
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "zoom" => {
                    if let Ok(zoom) = value.parse::<f32>()
                        && (0.5..=3.0).contains(&zoom)
                    {
                        settings.zoom = zoom;
                    }
                }
                "split" => {
                    if let Ok(split) = value.parse::<f32>()
                        && (0.2..=0.8).contains(&split)
                    {
                        settings.split = split;
                    }
                }
                "scale" => {
                    if let Ok(scale) = value.parse::<f32>()
                        && (0.5..=2.0).contains(&scale)
                    {
                        settings.scale = scale;
                    }
                }
                "theme" => settings.theme = Theme::named(value).unwrap_or_default(),
                "theme by day" => {
                    if let Some(theme) = Theme::named(value).filter(|t| !t.is_dark()) {
                        settings.light = theme;
                    }
                }
                "theme by night" => {
                    if let Some(theme) = Theme::named(value).filter(|t| t.is_dark()) {
                        settings.dark = theme;
                    }
                }
                "window" => {
                    let size = value.split_once('x').and_then(|(w, h)| {
                        Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?))
                    });
                    settings.window = size.filter(|&(w, h)| w >= 200.0 && h >= 150.0);
                }
                "vault" if !value.is_empty() => settings.vault = Some(PathBuf::from(value)),
                "sidebar" => settings.sidebar = value != "hidden",
                "recent" if !value.is_empty() && settings.recent.len() < RECENT => {
                    settings.recent.push(PathBuf::from(value));
                }
                _ => {}
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        let mut text = String::from(
            "# livemark's settings: one `key = value` a line; unknown lines are ignored.\n",
        );
        text += &format!("zoom = {}\n", self.zoom);
        text += &format!("scale = {}\n", self.scale);
        text += &format!("theme = {}\n", self.theme.name().to_lowercase());
        text += &format!("theme by day = {}\n", self.light.name().to_lowercase());
        text += &format!("theme by night = {}\n", self.dark.name().to_lowercase());
        text += &format!("split = {}\n", self.split);
        if let Some((width, height)) = self.window {
            text += &format!("window = {}x{}\n", width.round(), height.round());
        }
        if let Some(vault) = &self.vault {
            text += &format!("vault = {}\n", vault.display());
        }
        if !self.sidebar {
            text += "sidebar = hidden\n";
        }
        for path in &self.recent {
            text += &format!("recent = {}\n", path.display());
        }
        text
    }

    /// `file` opened or saved: first among the recent files.
    pub fn opened(&mut self, file: &Path) {
        let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
        self.recent.retain(|path| *path != file);
        self.recent.insert(0, file);
        self.recent.truncate(RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, Theme};

    #[test]
    fn settings_round_trip_and_survive_hand_edits() {
        let mut settings = Settings {
            zoom: 1.3,
            scale: 1.2,
            theme: Theme::KanagawaWave,
            light: Theme::KanagawaLotus,
            dark: Theme::CatppuccinMocha,
            window: Some((1200.0, 800.0)),
            recent: Vec::new(),
            split: 0.35,
            vault: Some("/notes".into()),
            sidebar: false,
        };
        settings.opened(std::path::Path::new("/notes/a.md"));
        settings.opened(std::path::Path::new("/notes/b.md"));
        settings.opened(std::path::Path::new("/notes/a.md"));
        assert_eq!(Settings::parse(&settings.render()), settings);
        assert_eq!(
            settings.recent[0].to_str(),
            Some("/notes/a.md"),
            "most recent first, once"
        );
        // Broken, unknown and out-of-range lines fall back to the defaults.
        let broken = Settings::parse("zoom = huge\ntheme = purple\nwindow = 10x\ncolor = red\n=\n");
        assert_eq!(broken, Settings::default());
        assert_eq!(Settings::parse("zoom = 9\n").zoom, 1.0);
        assert_eq!(Settings::parse("split = 0.9\n").split, 0.5);
        // Themes by name, any case; a dark one is not System's by day.
        assert_eq!(
            Settings::parse("theme = Gruvbox-Dark\n").theme,
            Theme::GruvboxDark
        );
        assert_eq!(Settings::parse("theme = light\n").theme, Theme::Light);
        assert_eq!(Settings::parse("theme by day = dark\n").light, Theme::Light);
        // At most ten recent files.
        let mut many = Settings::default();
        for i in 0..15 {
            many.opened(std::path::Path::new(&format!("/notes/{i}.md")));
        }
        assert_eq!(many.recent.len(), 10);
        assert_eq!(
            Settings::load(std::path::Path::new("/nonexistent/livemark")),
            Settings::default()
        );
    }
}
