//! livemark: a desktop editor for markdown files.
//!
//! ```text
//! livemark [file.md] [--dark|--light]
//! livemark note "<title>" [--tags a,b] [--by <agent>] [--vault <dir>] < body.md
//! livemark tags [--vault <dir>]
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
mod appearance;
mod autosave;
mod cli;
mod context;
mod file;
mod icons;
mod into_vault;
mod links;
mod manager;
mod manager_view;
mod menu;
mod note;
mod note_actions;
mod opening;
mod pictures;
mod relink;
mod search;
mod search_view;
mod settings;
mod shell;
mod sidebar;
mod tag_actions;
mod tags;
mod undo;
mod update;
mod vault;
mod vaults;
mod view;

use std::path::PathBuf;

use iced::{Task, Theme, window};
use livemark::widget::{self, Editor};
use settings::Settings;

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `livemark note ...`, `livemark tags`: no window (`cli.rs`).
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
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
    // A flag wins over the settings' theme until one is picked.
    let theme = if args.iter().any(|a| a == "--dark") {
        Some(Theme::Dark)
    } else if args.iter().any(|a| a == "--light") {
        Some(Theme::Light)
    } else {
        None
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
            // Whether the system is light or dark, for System's picks.
            let mode = iced::system::theme()
                .map(|mode| Message::Appearance(appearance::AppearanceMessage::System(mode)));
            (app, Task::batch([Editor::focus(), mode]))
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
    .theme(App::current_theme)
    .scale_factor(|app: &App| app.settings.scale)
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
    /// A `--dark` or `--light` flag's theme, until one is picked.
    theme: Option<Theme>,
    /// Whether the system is light or dark, for System's picks.
    system_mode: iced::theme::Mode,
    /// The Appearance panel, in the note's place while it is open.
    appearance: bool,
    /// The sidebar's edge is being dragged.
    resizing: bool,
    /// The file whose "outside the vault" bar was closed.
    outside_dismissed: Option<PathBuf>,
    /// When the note was last edited, for autosave, and whether a wait
    /// for the quiet after it is under way.
    last_edit: Option<std::time::Instant>,
    autosave_waiting: bool,
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
    /// Whether the vault menu is open, and the list open beside it.
    menu: bool,
    submenu: Option<menu::Submenu>,
    /// The vault open, if any (PLAN-004).
    vault: Option<vault::Vault>,
    /// Which notes the sidebar lists.
    shown: sidebar::Shown,
    /// Whether the sidebar lists every tag, not only the top eight.
    all_tags: bool,
    /// The last change to the vault's files, while it can be undone.
    undo: Option<undo::Undo>,
    /// What the note over the text says, while it shows.
    toast: Option<String>,
    /// Whether it offers Undo (a change), or only says something done.
    toast_undo: bool,
    /// The question open under a note in the sidebar.
    note_action: Option<note_actions::NoteAction>,
    /// The right-click menu open, and where the pointer last pressed.
    context: Option<context::Menu>,
    pointer: iced::Point,
    /// The sidebar's tag under the pointer, its tag with the menu open,
    /// and the question open under one.
    hovered_tag: Option<String>,
    tag_action: Option<tag_actions::TagAction>,
    /// The tag manager, in place of the note while it is open.
    manager: Option<manager::Manager>,
    /// Look-alike tags the user chose to keep apart.
    kept_apart: Vec<(String, String)>,
    /// The title being typed for a new note in the vault.
    naming: Option<String>,
    /// The search field's results (Ctrl+P), while they show.
    search: Option<search::Search>,
    /// Every match listed in the sidebar (Ctrl+Enter), until cleared.
    listing: Option<search::Listing>,
    /// What the editor was completing when choices were last offered.
    completing: Option<livemark::widget::Completing>,
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
    /// Opens (true) or closes the vault menu.
    Menu(bool),
    /// The list beside the vault menu, or none.
    Submenu(Option<menu::Submenu>),
    /// Ctrl+\: the sidebar hidden, or shown again.
    Sidebar,
    /// The sidebar's edge dragged.
    Resize(shell::Resize),
    /// The last change to the vault's files taken back.
    Undo,
    /// The note over the text closed.
    DismissToast,
    /// Escape: a menu, or a question under a tag or a note, taken back.
    Escape,
    /// The pointer pressed here (the right button?), before anything heard.
    Pressed {
        at: iced::Point,
        right: bool,
    },
    /// A right-click menu and what it does.
    Context(context::ContextMessage),
    /// The clipboard's text, for the text's menu.
    Paste(Option<String>),
    /// The bar over a note outside the vault.
    IntoVault(into_vault::IntoVault),
    /// The window lost focus: autosave writes the note.
    Blurred,
    /// Autosave's wait for the quiet after typing is over.
    AutosaveTick,
    /// A recent note picked in the File menu.
    Recent(PathBuf),
    /// The Appearance panel and what it sets.
    Appearance(appearance::AppearanceMessage),
    /// The vault and its sidebar.
    Vault(sidebar::VaultMessage),
    /// A tag's menu and what it does.
    Tag(tag_actions::TagMessage),
    /// The tag manager.
    Manager(manager::ManagerMessage),
    /// A note's menu and what it does.
    Note(note_actions::NoteMessage),
    /// The search field.
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
            system_mode: iced::theme::Mode::None,
            appearance: false,
            resizing: false,
            outside_dismissed: None,
            last_edit: None,
            autosave_waiting: false,
            pending: None,
            changed: false,
            discarded: None,
            tried: std::collections::HashSet::new(),
            settings: Settings::default(),
            settings_file: None,
            waiting_picture: None,
            menu: false,
            submenu: None,
            vault: None,
            shown: sidebar::Shown::All,
            all_tags: false,
            undo: None,
            toast: None,
            toast_undo: false,
            context: None,
            pointer: iced::Point::ORIGIN,
            note_action: None,
            hovered_tag: None,
            tag_action: None,
            manager: None,
            kept_apart: Vec::new(),
            naming: None,
            search: None,
            listing: None,
            completing: None,
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
        if let Some(root) = self.settings.vault.clone() {
            match vault::Vault::open(&root) {
                Ok(vault) => {
                    if !self.settings.vaults.contains(&vault.root) {
                        self.settings.vaults.insert(0, vault.root.clone());
                    }
                    self.vault = Some(vault);
                }
                Err(error) => self.error = Some(format!("{}: {error}", root.display())),
            }
        }
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
}

/// Opens a web or mail link with the system's opener. Other links (local
/// files, other schemes) are refused: a note must not start programs.
pub(crate) fn open_link(link: &str) -> Result<(), String> {
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
mod manager_tests;
#[cfg(test)]
mod note_tests;
#[cfg(test)]
mod search_tests;
#[cfg(test)]
mod switch_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vault_tests;
