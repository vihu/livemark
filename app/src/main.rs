//! livemark: a desktop editor for markdown files.
//!
//! ```text
//! livemark [file.md] [--dark|--light]
//! cargo run --release -p livemark-app -- [file.md] [--dark|--light]
//! ```
//!
//! Light or dark follows the system unless `--dark`, `--light` or the
//! settings say. The zoom, the window's size and the recent files are
//! remembered between starts (`settings.rs`).
//! Ctrl+N starts a new note, Ctrl+O opens, Ctrl+S saves (asking where for
//! a new file), Ctrl+Shift+S saves as. Ctrl+click on a web or mail link (or Alt+Enter in it) opens it
//! in the system's browser or mail program. The title shows `*` while there are unsaved changes; closing
//! the window or opening another file then asks first. When the window
//! comes back into focus and the file changed on disk, it is loaded again,
//! or with unsaved changes the app asks which to keep. File > Open vault
//! makes a folder of notes a vault with a sidebar (PLAN-004, `vault.rs`,
//! `sidebar.rs`); with one and no file given, the note last open in it
//! opens.
mod file;
mod note;
mod pictures;
mod quick;
mod search;
mod settings;
mod sidebar;
mod vault;
mod view;

use std::path::PathBuf;

use iced::{Task, Theme, window};
use livemark::widget::{self, Editor};
use settings::Settings;

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from);
    let settings_file = Settings::path();
    let settings = settings_file
        .as_deref()
        .map(Settings::load)
        .unwrap_or_default();
    // With a vault and no file asked for, the note last open in it.
    let path = path.or_else(|| {
        let vault = settings.vault.as_ref()?;
        let last = settings.recent.first()?;
        (last.starts_with(vault) && last.exists()).then(|| last.clone())
    });
    let theme = if args.iter().any(|a| a == "--dark") {
        Some(Theme::Dark)
    } else if args.iter().any(|a| a == "--light") {
        Some(Theme::Light)
    } else {
        iced_theme(settings.theme)
    };
    let size = settings
        .window
        .map_or(window::Settings::default().size, |(w, h)| {
            iced::Size::new(w, h)
        });
    iced::application(
        move || {
            let app = App::open(path.clone(), theme.clone())
                .with_settings(settings.clone(), settings_file.clone());
            (app, Editor::focus())
        },
        App::update,
        App::view,
    )
    .title(App::title)
    .window(window::Settings {
        size,
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "io.github.vihu.livemark".into(),
            ..Default::default()
        },
        ..Default::default()
    })
    // The prose font for the app's own widgets too: the toolbar, the File
    // menu and the bars.
    .fonts(livemark::fonts::ATKINSON_HYPERLEGIBLE_NEXT)
    .font(iced::Font::new(livemark::fonts::PROSE))
    .theme(|app: &App| app.theme.clone())
    .subscription(App::subscription)
    .exit_on_close_request(false)
    .run()
}

struct App {
    /// Where Ctrl+S writes; `None` for a new note.
    path: Option<PathBuf>,
    editor: Editor,
    /// `Editor::version` when last opened or saved.
    saved: u64,
    /// The last open or save error, until the next success.
    error: Option<String>,
    /// `None` follows the system.
    theme: Option<Theme>,
    /// What waits on an answer about unsaved changes.
    pending: Option<After>,
    /// The file's modification time when last opened or saved.
    stamp: Option<std::time::SystemTime>,
    /// The file changed on disk while there were unsaved changes.
    changed: bool,
    /// The editor version whose unsaved changes the user chose to discard.
    discarded: Option<u64>,
    /// Image destinations already looked for next to the note, for this
    /// editor and path.
    tried: std::collections::HashSet<String>,
    settings: Settings,
    /// Where the settings are written; `None` writes nothing (tests).
    settings_file: Option<PathBuf>,
    /// A picture pasted into a note not saved yet: kept once it is.
    waiting_picture: Option<Vec<u8>>,
    /// Whether the File menu is open.
    menu: bool,
    /// The vault open, if any (PLAN-004).
    vault: Option<vault::Vault>,
    /// The tag the sidebar's notes are filtered by.
    tag: Option<String>,
    /// The title being typed for a new note in the vault.
    naming: Option<String>,
    /// Quick open (Ctrl+P), while it is open.
    quick: Option<quick::Quick>,
    /// Search across the vault (Ctrl+Shift+F), while it is open.
    search: Option<search::Search>,
}

/// What happens once unsaved changes are saved or discarded.
#[derive(Debug, Clone)]
enum After {
    Close,
    Open,
    New,
    /// This file, picked while the dialog was open over unsaved typing.
    Load(PathBuf),
}

#[derive(Debug, Clone)]
enum Message {
    Editor(widget::Message),
    New,
    Open,
    Opened(Option<PathBuf>),
    Save {
        choose: bool,
    },
    /// Where it was written (`None` when the dialog was cancelled), and the
    /// editor version that was written.
    Saved(Result<Option<PathBuf>, String>, u64),
    CloseRequested,
    /// The answer to "unsaved changes": save first, or discard them.
    Unsaved(Option<bool>),
    /// The window came into focus: the file may have changed on disk.
    Focused,
    /// The answer to "changed on disk": load it (discarding the unsaved
    /// changes) or keep the text here.
    Reload(bool),
    /// Ctrl+= or Ctrl++ (1), Ctrl+- (-1): the text size a 10% step up or
    /// down; Ctrl+0 (0): back to 100%.
    Zoom(i8),
    /// The window's new size, remembered for the next start.
    Resized(iced::Size),
    /// A file dropped on the window.
    Dropped(PathBuf),
    /// Opens (true) or closes the File menu.
    Menu(bool),
    /// A recent note picked in the File menu.
    Recent(PathBuf),
    /// The theme picked in the File menu, remembered.
    Theme(settings::Theme),
    /// The vault and its sidebar.
    Vault(sidebar::VaultMessage),
    /// Quick open.
    Quick(quick::QuickMessage),
    /// Search across the vault.
    Search(search::SearchMessage),
}

impl App {
    fn open(path: Option<PathBuf>, theme: Option<Theme>) -> Self {
        let (text, error) = match path.as_deref() {
            // A new file: empty, saved there on Ctrl+S.
            Some(path) if !path.exists() => (String::new(), None),
            Some(path) => match file::load(path) {
                Ok(text) => (text, None),
                Err(error) => (String::new(), Some(error)),
            },
            None => (String::new(), None),
        };
        let editor = Editor::new(text);
        let path = path.filter(|_| error.is_none());
        Self {
            stamp: path.as_deref().and_then(file::modified),
            path,
            saved: editor.version(),
            editor,
            error,
            theme,
            pending: None,
            changed: false,
            discarded: None,
            tried: std::collections::HashSet::new(),
            settings: Settings::default(),
            settings_file: None,
            waiting_picture: None,
            menu: false,
            vault: None,
            tag: None,
            naming: None,
            quick: None,
            search: None,
        }
        .with_images()
    }

    /// The remembered settings applied (the zoom), kept in `file`, with
    /// the note opened first among the recent files.
    fn with_settings(mut self, settings: Settings, file: Option<PathBuf>) -> Self {
        self.settings = settings;
        self.settings_file = file;
        self.editor.set_zoom(self.settings.zoom);
        self.editor.set_split_ratio(self.settings.split);
        self.vault = self
            .settings
            .vault
            .as_deref()
            .and_then(|root| vault::Vault::open(root).ok());
        self.settings.recent.retain(|path| path.exists());
        if let Some(path) = self.path.clone() {
            self.settings.opened(&path);
        }
        self.remember();
        self
    }

    /// The editor's zoom remembered when it changed (keys, Ctrl+wheel),
    /// and where the side by side divider is: kept with the next write (at
    /// the latest on closing), not once per step of a drag.
    fn remember_zoom(&mut self) {
        self.settings.split = self.editor.split_ratio();
        if self.editor.zoom() != self.settings.zoom {
            self.settings.zoom = self.editor.zoom();
            self.remember();
        }
    }

    /// A new editor for `text`, drawn as the current one: its zoom, mode
    /// and split ratio carried over.
    fn editor_for(&self, text: String) -> Editor {
        let mut editor = Editor::new(text);
        editor.set_zoom(self.editor.zoom());
        editor.set_mode(self.editor.mode());
        editor.set_split_ratio(self.editor.split_ratio());
        editor
    }

    /// Writes the settings, when there is a file for them; a failure only
    /// loses what was remembered.
    fn remember(&self) {
        if let Some(file) = &self.settings_file {
            let _ = self.settings.save(file);
        }
    }

    /// Closes the window, the settings written first.
    fn exit(&mut self) -> Task<Message> {
        self.remember();
        iced::exit()
    }

    /// Whether the file was modified on disk since it was opened or saved
    /// here.
    fn changed_on_disk(&self) -> bool {
        let stamp = self.path.as_deref().and_then(file::modified);
        stamp.is_some() && stamp != self.stamp
    }

    /// Loads the file again if it changed on disk; with unsaved changes,
    /// asks first.
    fn check_disk(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let stamp = file::modified(path);
        if stamp.is_none() || stamp == self.stamp {
            return;
        }
        if self.unsaved() {
            self.changed = true;
        } else {
            self.reload();
        }
    }

    /// The file's text from disk, the selection kept where it fits.
    fn reload(&mut self) {
        self.changed = false;
        let Some(path) = self.path.clone() else {
            return;
        };
        match file::load(&path) {
            Ok(text) => {
                let selection = self.editor.selection();
                self.editor = self.editor_for(text);
                self.tried.clear();
                self.editor.select(selection.anchor, selection.head);
                self.saved = self.editor.version();
                self.discarded = None;
                self.stamp = file::modified(&path);
                self.error = None;
                self.load_images();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn unsaved(&self) -> bool {
        self.editor.version() != self.saved
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or("untitled".into(), |n| n.to_string_lossy().into_owned());
        let star = if self.unsaved() { "*" } else { "" };
        format!("{name}{star} - livemark")
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        // A File menu item picked.
        if matches!(
            message,
            Message::New | Message::Open | Message::Save { .. } | Message::Recent(_)
        ) {
            self.menu = false;
        }
        match message {
            Message::Editor(message) if message.pasted_image().is_some() => {
                return self.paste_picture(message.pasted_image().unwrap_or_default());
            }
            Message::Dropped(file) => return self.dropped(file),
            Message::Vault(message) => return self.vault_update(message),
            Message::Quick(message) => return self.quick_update(message),
            Message::Search(message) => return self.search_update(message),
            Message::Editor(message) => {
                if let Some(tag) = message.tag() {
                    self.show_tag(tag);
                }
                // Only a failure is reported; an open or save error stays.
                if let Some(Err(error)) = message.link().map(open_link) {
                    self.error = Some(error);
                }
                let task = self.editor.update(message).map(Message::Editor);
                self.load_images();
                self.remember_zoom();
                return task;
            }
            // In a vault a new note is named first and made there.
            Message::New if self.vault.is_some() => {
                return self.vault_update(sidebar::VaultMessage::NewNote);
            }
            Message::New if self.unsaved() => self.pending = Some(After::New),
            Message::New => return self.new_note(),
            Message::Open if self.unsaved() => self.pending = Some(After::Open),
            Message::Open => return Task::perform(file::pick(), Message::Opened),
            // Typing while the dialog was open is not dropped unasked.
            Message::Opened(Some(path))
                if self.unsaved() && self.discarded != Some(self.editor.version()) =>
            {
                self.pending = Some(After::Load(path));
            }
            Message::Opened(Some(path)) => return self.load(path),
            Message::Opened(None) => {}
            // Never over a file changed on disk since it was opened or
            // saved: ask first (Load it, Keep mine), then save again.
            Message::Save { choose: false } if self.changed_on_disk() => self.changed = true,
            Message::Save { choose } => {
                let text = self.editor.text().to_owned();
                let version = self.editor.version();
                let path = self.path.clone().filter(|_| !choose);
                return Task::perform(file::save_as(path, text), move |result| {
                    Message::Saved(result, version)
                });
            }
            Message::Saved(Ok(Some(path)), version) => {
                self.error = None;
                self.stamp = file::modified(&path);
                self.changed = false;
                // Saved somewhere new: its images are looked for there.
                if self.path.as_ref() != Some(&path) {
                    self.tried.clear();
                }
                self.settings.opened(&path);
                self.remember();
                self.path = Some(path);
                self.saved = version;
                self.load_images();
                self.refresh_vault();
                // A picture pasted before the note had a place.
                if let Some(png) = self.waiting_picture.take() {
                    let _ = self.paste_picture(png);
                }
                // Typing while the save dialog was open: ask again.
                if let Some(after) = self.pending.take() {
                    if self.unsaved() {
                        self.pending = Some(after);
                    } else {
                        return self.carry_on(after);
                    }
                }
            }
            Message::Saved(Ok(None), _) => self.pending = None,
            Message::Saved(Err(error), _) => {
                self.error = Some(error);
                self.pending = None;
            }
            Message::CloseRequested if self.unsaved() => self.pending = Some(After::Close),
            Message::CloseRequested => return self.exit(),
            Message::Unsaved(Some(true)) => return self.update(Message::Save { choose: false }),
            Message::Unsaved(Some(false)) => {
                self.discarded = Some(self.editor.version());
                if let Some(after) = self.pending.take() {
                    return self.carry_on(after);
                }
            }
            Message::Unsaved(None) => self.pending = None,
            Message::Focused => {
                self.check_disk();
                self.refresh_vault();
            }
            Message::Resized(size) => self.settings.window = Some((size.width, size.height)),
            Message::Reload(true) => {
                // Nothing is unsaved after it: a close or open waiting on
                // the unsaved text is asked for again.
                self.pending = None;
                self.reload();
            }
            Message::Zoom(step) => {
                let tenths = (self.editor.zoom() * 10.0).round() + f32::from(step);
                self.editor
                    .set_zoom(if step == 0 { 1.0 } else { tenths / 10.0 });
                self.remember_zoom();
            }
            Message::Menu(open) => {
                self.menu = open;
                // Notes moved or deleted since are not offered.
                let count = self.settings.recent.len();
                self.settings.recent.retain(|path| path.exists());
                if self.settings.recent.len() != count {
                    self.remember();
                }
            }
            Message::Recent(path) if !path.exists() => {
                self.error = Some(format!("No longer there: {}", path.display()));
                self.settings.recent.retain(|recent| *recent != path);
                self.remember();
            }
            Message::Recent(path) => return self.update(Message::Opened(Some(path))),
            Message::Theme(theme) => {
                self.theme = iced_theme(theme);
                self.settings.theme = theme;
                self.remember();
            }
            Message::Reload(false) => {
                // Keep this text; saving will replace the file's.
                self.changed = false;
                self.stamp = self.path.as_deref().and_then(file::modified);
            }
        }
        Task::none()
    }

    fn carry_on(&mut self, after: After) -> Task<Message> {
        match after {
            After::Close => self.exit(),
            After::Open => Task::perform(file::pick(), Message::Opened),
            After::New => self.new_note(),
            After::Load(path) => self.load(path),
        }
    }

    /// The file at `path` in place of the current note.
    fn load(&mut self, path: PathBuf) -> Task<Message> {
        match file::load(&path) {
            Ok(text) => {
                self.editor = self.editor_for(text);
                self.tried.clear();
                self.saved = self.editor.version();
                // Versions start again with a new editor.
                self.discarded = None;
                self.stamp = file::modified(&path);
                self.changed = false;
                self.settings.opened(&path);
                self.remember();
                self.path = Some(path);
                self.error = None;
                self.load_images();
                Editor::focus()
            }
            Err(error) => {
                self.error = Some(error);
                Task::none()
            }
        }
    }

    /// An empty, untitled note in place of the current one.
    fn new_note(&mut self) -> Task<Message> {
        self.path = None;
        self.editor = self.editor_for(String::new());
        self.tried.clear();
        self.saved = self.editor.version();
        self.discarded = None;
        self.stamp = None;
        self.changed = false;
        self.error = None;
        Editor::focus()
    }
}

/// The iced theme for a remembered one; `None` follows the system.
fn iced_theme(theme: settings::Theme) -> Option<Theme> {
    match theme {
        settings::Theme::System => None,
        settings::Theme::Light => Some(Theme::Light),
        settings::Theme::Dark => Some(Theme::Dark),
    }
}

/// Opens a web or mail link with the system's opener. Other links (local
/// files, other schemes) are refused: a note must not start programs.
fn open_link(link: &str) -> Result<(), String> {
    let web = ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| link.starts_with(scheme));
    if !web {
        return Err(format!("Not opened (web and mail links only): {link}"));
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut child = std::process::Command::new(opener)
        .arg(link)
        .spawn()
        .map_err(|e| format!("{opener}: {e}"))?;
    // Reaped in the background, so no zombie is left behind.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests;
