//! What the app remembers between starts (PLAN-002): the zoom, the theme,
//! the window's size, the most recent files, where the side by side
//! divider was (PLAN-003), the vault and whether the sidebar shows. A TOML
//! file read and written by serde (PLAN-007); a key missing takes its
//! default, a value out of range too, and a file that does not read is set
//! aside as `settings.toml.broken`, never written over.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many recent files are kept.
const RECENT: usize = 10;

/// The sidebar's width, and how far its edge drags.
pub const SIDEBAR_WIDTH: f32 = 260.0;
pub const SIDEBAR_WIDTHS: std::ops::RangeInclusive<f32> = 200.0..=480.0;

/// The theme: as the system is (between a light and a dark one picked),
/// or one of iced's (PLAN-006); kept as `kanagawa-wave` and the like.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    /// The note's text size (`Editor::set_zoom`).
    pub zoom: f32,
    /// The whole interface's scale (PLAN-006).
    pub scale: f32,
    pub theme: Theme,
    /// What System follows by day and by night.
    #[serde(rename = "theme-by-day")]
    pub light: Theme,
    #[serde(rename = "theme-by-night")]
    pub dark: Theme,
    /// The window's size when it was last closed.
    pub window: Option<(f32, f32)>,
    /// Most recent first.
    pub recent: Vec<PathBuf>,
    /// The markdown's share of the width side by side.
    pub split: f32,
    /// The vault last opened (PLAN-004).
    pub vault: Option<PathBuf>,
    /// The vaults opened, most recent first (PLAN-006).
    pub vaults: Vec<PathBuf>,
    /// Whether the sidebar shows (Ctrl+\ hides it, PLAN-006).
    pub sidebar: bool,
    /// The sidebar's width, its edge dragged (PLAN-006).
    pub sidebar_width: f32,
    /// Whether notes are written as they are typed (PLAN-006).
    pub autosave: bool,
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
            vaults: Vec::new(),
            sidebar: true,
            sidebar_width: SIDEBAR_WIDTH,
            autosave: true,
        }
    }
}

impl Settings {
    /// Where they are kept: `livemark/settings.toml` in the platform's
    /// config folder (`~/.config` on Linux, `~/Library/Application Support`
    /// on macOS).
    pub fn path() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("livemark").join("settings.toml"))
    }

    /// The settings in the file at `path`, the defaults when it is missing.
    /// A file that does not read is moved aside, so it is not written over,
    /// and the error says where it went.
    pub fn load(path: &Path) -> Result<Self, String> {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Ok(Self::default());
        };
        toml::from_str::<Self>(&text)
            .map(Self::checked)
            .map_err(|error| {
                let aside = path.with_extension("toml.broken");
                let _ = std::fs::rename(path, &aside);
                format!(
                    "Settings reset: {} did not read ({}); kept as {}",
                    path.display(),
                    error.message(),
                    aside.display()
                )
            })
    }

    /// Writes them to `path`, making its folder.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Values out of range back to their defaults; System's pick by day a
    /// light theme, by night a dark one; at most ten recent files.
    fn checked(mut self) -> Self {
        let default = Self::default();
        if !(0.5..=3.0).contains(&self.zoom) {
            self.zoom = default.zoom;
        }
        if !(0.5..=2.0).contains(&self.scale) {
            self.scale = default.scale;
        }
        if !(0.2..=0.8).contains(&self.split) {
            self.split = default.split;
        }
        if !SIDEBAR_WIDTHS.contains(&self.sidebar_width) {
            self.sidebar_width = default.sidebar_width;
        }
        if self.light.is_dark() {
            self.light = default.light;
        }
        if !self.dark.is_dark() {
            self.dark = default.dark;
        }
        self.window = self.window.filter(|&(w, h)| w >= 200.0 && h >= 150.0);
        self.recent.truncate(RECENT);
        self
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

    fn read(text: &str) -> Settings {
        toml::from_str::<Settings>(text).unwrap().checked()
    }

    #[test]
    fn settings_round_trip_and_survive_hand_edits() {
        let mut settings = Settings {
            zoom: 1.25,
            scale: 1.5,
            theme: Theme::KanagawaWave,
            light: Theme::KanagawaLotus,
            dark: Theme::CatppuccinMocha,
            window: Some((1200.0, 800.0)),
            recent: Vec::new(),
            split: 0.375,
            vault: Some("/notes".into()),
            vaults: vec!["/notes".into(), "/work".into()],
            sidebar: false,
            sidebar_width: 320.0,
            autosave: false,
        };
        settings.opened(std::path::Path::new("/notes/a.md"));
        settings.opened(std::path::Path::new("/notes/b.md"));
        settings.opened(std::path::Path::new("/notes/a.md"));
        let text = toml::to_string(&settings).unwrap();
        assert!(text.contains("theme = \"kanagawa-wave\""), "{text}");
        assert_eq!(read(&text), settings);
        assert_eq!(
            settings.recent[0].to_str(),
            Some("/notes/a.md"),
            "most recent first, once"
        );
        // Missing keys and values out of range take the defaults.
        assert_eq!(read(""), Settings::default());
        assert_eq!(
            read("zoom = 9\nsplit = 0.9\nwindow = [10, 10]\n"),
            Settings::default()
        );
        assert_eq!(read("zoom = 2\n").zoom, 2.0, "a whole number");
        // A dark theme is not System's by day.
        assert_eq!(read("theme-by-day = \"dark\"\n").light, Theme::Light);
        // At most ten recent files.
        let mut many = Settings::default();
        for i in 0..15 {
            many.opened(std::path::Path::new(&format!("/notes/{i}.md")));
        }
        assert_eq!(many.recent.len(), 10);
    }

    #[test]
    fn a_file_that_does_not_read_is_set_aside_not_written_over() {
        let dir = std::env::temp_dir().join(format!("livemark-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.toml");
        assert_eq!(Settings::load(&file), Ok(Settings::default()), "missing");
        std::fs::write(&file, "theme = \"purple\"\n").unwrap();
        let error = Settings::load(&file).unwrap_err();
        assert!(error.contains("settings.toml.broken"), "{error}");
        assert!(!file.exists());
        let aside = dir.join("settings.toml.broken");
        assert_eq!(
            std::fs::read_to_string(&aside).unwrap(),
            "theme = \"purple\"\n"
        );
        std::fs::remove_file(aside).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
