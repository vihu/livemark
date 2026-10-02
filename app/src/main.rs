//! livemark: a desktop editor for markdown files.
//!
//! ```text
//! livemark [file.md] [--dark|--light]
//! cargo run --release -p livemark-app -- [file.md] [--dark|--light]
//! ```
//!
//! Light or dark follows the system unless `--dark` or `--light` says.
//! Ctrl+O opens, Ctrl+S saves (asking where for a new file), Ctrl+Shift+S
//! saves as. Ctrl+click on a web or mail link (or Alt+Enter in it) opens it
//! in the system's browser or mail program. The title shows `*` while there are unsaved changes; closing
//! the window or opening another file then asks first.
mod file;

use std::path::PathBuf;

use iced::keyboard::{self, key};
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
}

/// What happens once unsaved changes are saved or discarded.
#[derive(Debug, Clone, Copy)]
enum After {
    Close,
    Open,
}

#[derive(Debug, Clone)]
enum Message {
    Editor(widget::Message),
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
}

impl App {
    fn open(path: Option<PathBuf>, theme: Option<Theme>) -> Self {
        let (text, error) = match path.as_deref().map(file::load) {
            Some(Ok(text)) => (text, None),
            Some(Err(error)) => (String::new(), Some(error)),
            None => (String::new(), None),
        };
        let editor = Editor::new(text);
        Self {
            path: path.filter(|_| error.is_none()),
            saved: editor.version(),
            editor,
            error,
            theme,
            pending: None,
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
                if let Some(link) = message.link() {
                    self.error = open_link(link).err();
                }
                return self.editor.update(message).map(Message::Editor);
            }
            Message::Open if self.unsaved() => self.pending = Some(After::Open),
            Message::Open => return Task::perform(pick_file(), Message::Opened),
            Message::Opened(Some(path)) => match file::load(&path) {
                Ok(text) => {
                    self.editor = Editor::new(text);
                    self.saved = self.editor.version();
                    self.path = Some(path);
                    self.error = None;
                    return Editor::focus();
                }
                Err(error) => self.error = Some(error),
            },
            Message::Opened(None) => {}
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
                self.path = Some(path);
                self.saved = version;
                if let Some(after) = self.pending.take() {
                    return self.carry_on(after);
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
                if let Some(after) = self.pending.take() {
                    return self.carry_on(after);
                }
            }
            Message::Unsaved(None) => self.pending = None,
        }
        Task::none()
    }

    fn carry_on(&mut self, after: After) -> Task<Message> {
        match after {
            After::Close => iced::exit(),
            After::Open => Task::perform(pick_file(), Message::Opened),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let editor = self.editor.view().map(Message::Editor);
        let bar: Option<Element<'_, Message>> = if self.pending.is_some() {
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
        match bar {
            Some(bar) => column![editor, container(bar).padding(12)].into(),
            None => editor,
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed {
                physical_key: key::Physical::Code(code),
                modifiers,
                ..
            } = event
            else {
                return None;
            };
            match code {
                key::Code::KeyO if modifiers.command() => Some(Message::Open),
                key::Code::KeyS if modifiers.command() => Some(Message::Save {
                    choose: modifiers.shift(),
                }),
                _ => None,
            }
        });
        let close = window::close_requests().map(|_| Message::CloseRequested);
        Subscription::batch([keys, close])
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
