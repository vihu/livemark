//! livemark: a desktop editor for markdown files.
//!
//! ```text
//! livemark [file.md] [--dark|--light]
//! cargo run --release -p livemark-app -- [file.md] [--dark|--light]
//! ```
//!
//! Light or dark follows the system unless `--dark` or `--light` says.
//! Ctrl+N starts a new note, Ctrl+O opens, Ctrl+S saves (asking where for
//! a new file), Ctrl+Shift+S saves as. Ctrl+click on a web or mail link (or Alt+Enter in it) opens it
//! in the system's browser or mail program. The title shows `*` while there are unsaved changes; closing
//! the window or opening another file then asks first. When the window
//! comes back into focus and the file changed on disk, it is loaded again,
//! or with unsaved changes the app asks which to keep.
mod file;

use std::path::PathBuf;

use iced::keyboard;
use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Subscription, Task, Theme, window};
use livemark::widget::{self, Editor};

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from);
    let theme = if args.iter().any(|a| a == "--dark") {
        Some(Theme::Dark)
    } else if args.iter().any(|a| a == "--light") {
        Some(Theme::Light)
    } else {
        None
    };
    iced::application(
        move || (App::open(path.clone(), theme.clone()), Editor::focus()),
        App::update,
        App::view,
    )
    .title(App::title)
    .window(window::Settings {
        #[cfg(target_os = "linux")]
        platform_specific: window::settings::PlatformSpecific {
            application_id: "io.github.vihu.livemark".into(),
            ..Default::default()
        },
        ..Default::default()
    })
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
}

impl App {
    fn open(path: Option<PathBuf>, theme: Option<Theme>) -> Self {
        let (text, error) = match path.as_deref().map(file::load) {
            Some(Ok(text)) => (text, None),
            Some(Err(error)) => (String::new(), Some(error)),
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
        }
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
                self.editor = Editor::new(text);
                self.editor.select(selection.anchor, selection.head);
                self.saved = self.editor.version();
                self.stamp = file::modified(&path);
                self.error = None;
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
        match message {
            Message::Editor(message) => {
                // Only a failure is reported; an open or save error stays.
                if let Some(Err(error)) = message.link().map(open_link) {
                    self.error = Some(error);
                }
                return self.editor.update(message).map(Message::Editor);
            }
            Message::New if self.unsaved() => self.pending = Some(After::New),
            Message::New => return self.new_note(),
            Message::Open if self.unsaved() => self.pending = Some(After::Open),
            Message::Open => return Task::perform(pick_file(), Message::Opened),
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
                return Task::perform(save_file(path, text), move |result| {
                    Message::Saved(result, version)
                });
            }
            Message::Saved(Ok(Some(path)), version) => {
                self.error = None;
                self.stamp = file::modified(&path);
                self.changed = false;
                self.path = Some(path);
                self.saved = version;
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
            Message::CloseRequested => return iced::exit(),
            Message::Unsaved(Some(true)) => return self.update(Message::Save { choose: false }),
            Message::Unsaved(Some(false)) => {
                self.discarded = Some(self.editor.version());
                if let Some(after) = self.pending.take() {
                    return self.carry_on(after);
                }
            }
            Message::Unsaved(None) => self.pending = None,
            Message::Focused => self.check_disk(),
            Message::Reload(true) => self.reload(),
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
            After::Close => iced::exit(),
            After::Open => Task::perform(pick_file(), Message::Opened),
            After::New => self.new_note(),
            After::Load(path) => self.load(path),
        }
    }

    /// The file at `path` in place of the current note.
    fn load(&mut self, path: PathBuf) -> Task<Message> {
        match file::load(&path) {
            Ok(text) => {
                self.editor = Editor::new(text);
                self.saved = self.editor.version();
                self.stamp = file::modified(&path);
                self.changed = false;
                self.path = Some(path);
                self.error = None;
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
        self.editor = Editor::new(String::new());
        self.saved = self.editor.version();
        self.stamp = None;
        self.changed = false;
        self.error = None;
        Editor::focus()
    }

    fn view(&self) -> Element<'_, Message> {
        let editor = self.editor.view().map(Message::Editor);
        let bar: Option<Element<'_, Message>> = if self.changed {
            Some(
                row![
                    text("The file changed on disk.").width(Length::Fill),
                    button("Load it")
                        .style(button::danger)
                        .on_press(Message::Reload(true)),
                    button("Keep mine")
                        .style(button::secondary)
                        .on_press(Message::Reload(false)),
                ]
                .spacing(8)
                .align_y(iced::Center)
                .into(),
            )
        } else if self.pending.is_some() {
            Some(
                row![
                    text("Unsaved changes.").width(Length::Fill),
                    button("Save").on_press(Message::Unsaved(Some(true))),
                    button("Discard")
                        .style(button::danger)
                        .on_press(Message::Unsaved(Some(false))),
                    button("Cancel")
                        .style(button::secondary)
                        .on_press(Message::Unsaved(None)),
                ]
                .spacing(8)
                .align_y(iced::Center)
                .into(),
            )
        } else {
            self.error
                .as_ref()
                .map(|error| text(error).style(text::danger).into())
        };
        // Always a column, so the editor keeps its place in the widget tree
        // (and its focus) when a bar comes or goes.
        column![editor]
            .push(bar.map(|bar| container(bar).padding(12)))
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        // By the letter on the key in the layout, as the editor reads its
        // own shortcuts.
        let keys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                ..
            } = event
            else {
                return None;
            };
            if !modifiers.command() || modifiers.alt() {
                return None;
            }
            match key.to_latin(physical_key)? {
                'n' => Some(Message::New),
                'o' => Some(Message::Open),
                's' => Some(Message::Save {
                    choose: modifiers.shift(),
                }),
                _ => None,
            }
        });
        let close = window::close_requests().map(|_| Message::CloseRequested);
        let focus = window::events().filter_map(|(_, event)| {
            matches!(event, window::Event::Focused).then_some(Message::Focused)
        });
        Subscription::batch([keys, close, focus])
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

async fn pick_file() -> Option<PathBuf> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Markdown", &["md", "markdown", "txt"])
        .pick_file()
        .await?;
    Some(file.path().to_owned())
}

/// Saves to `path`, or asks where first when there is none.
async fn save_file(path: Option<PathBuf>, text: String) -> Result<Option<PathBuf>, String> {
    let path = match path {
        Some(path) => path,
        None => {
            let dialog = rfd::AsyncFileDialog::new()
                .add_filter("Markdown", &["md"])
                .set_file_name("untitled.md");
            let Some(file) = dialog.save_file().await else {
                return Ok(None);
            };
            file.path().to_owned()
        }
    };
    file::save(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{App, Message};

    /// Writes `text` to `path` with a modification time `secs` from now.
    fn write(path: &std::path::Path, text: &str, secs: u64) {
        std::fs::write(path, text).unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() + Duration::from_secs(secs))
            .unwrap();
    }

    #[test]
    fn a_file_changed_on_disk_loads_on_focus_or_asks_with_edits() {
        let dir = std::env::temp_dir().join(format!("livemark-focus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.md");
        write(&path, "one\n", 0);
        let mut app = App::open(Some(path.clone()), None);
        let _ = app.update(Message::Focused);
        assert_eq!(app.editor.text(), "one\n", "unchanged");
        write(&path, "two\n", 10);
        let _ = app.update(Message::Focused);
        assert_eq!(app.editor.text(), "two\n", "loaded again");
        assert!(!app.unsaved());
        // With unsaved changes it asks; keeping them stops the asking.
        app.saved = u64::MAX;
        write(&path, "three\n", 20);
        let _ = app.update(Message::Focused);
        assert!(app.changed);
        assert_eq!(app.editor.text(), "two\n");
        let _ = app.update(Message::Reload(false));
        let _ = app.update(Message::Focused);
        assert!(!app.changed, "kept");
        write(&path, "four\n", 30);
        let _ = app.update(Message::Focused);
        let _ = app.update(Message::Reload(true));
        assert_eq!(app.editor.text(), "four\n", "loaded, edits dropped");
        // Saving over a change made on disk asks first; keeping mine lets
        // the next save through.
        write(&path, "five\n", 40);
        let _ = app.update(Message::Save { choose: false });
        assert!(app.changed, "asked, not saved");
        let _ = app.update(Message::Reload(false));
        let _ = app.update(Message::Save { choose: false });
        assert!(!app.changed, "saving");
        // A file picked while there is unsaved typing waits for an answer;
        // one picked after discarding loads.
        let other = dir.join("other.md");
        write(&other, "other\n", 0);
        app.saved = u64::MAX;
        let _ = app.update(Message::Opened(Some(other.clone())));
        assert!(matches!(app.pending, Some(super::After::Load(_))));
        assert_ne!(app.editor.text(), "other\n");
        let _ = app.update(Message::Unsaved(None));
        app.discarded = Some(app.editor.version());
        let _ = app.update(Message::Opened(Some(other.clone())));
        assert_eq!(app.editor.text(), "other\n");
        std::fs::remove_file(&other).unwrap();
        app.path = Some(path.clone());
        // Ctrl+N with unsaved changes asks first; discarding starts afresh.
        app.saved = u64::MAX;
        let _ = app.update(Message::New);
        assert_eq!(app.editor.text(), "other\n", "asked first");
        let _ = app.update(Message::Unsaved(Some(false)));
        assert_eq!((app.editor.text(), app.path.is_none()), ("", true));
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
