//! Opening, reloading and replacing the note: a file picked or dropped,
//! a new one, the file changed on disk, and what waits on the answer
//! about unsaved changes.
use std::path::PathBuf;

use iced::Task;
use livemark::widget::Editor;

use super::{After, App, Message, file};

impl App {
    /// Whether the file was modified on disk since it was opened or saved
    /// here.
    pub(crate) fn changed_on_disk(&self) -> bool {
        let stamp = self.path.as_deref().and_then(file::modified);
        stamp.is_some() && stamp != self.stamp
    }

    /// Loads the file again if it changed on disk; with unsaved changes,
    /// asks first.
    pub(crate) fn check_disk(&mut self) {
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
    pub(crate) fn reload(&mut self) {
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

    pub(crate) fn carry_on(&mut self, after: After) -> Task<Message> {
        match after {
            After::Close => self.exit(),
            After::Open => Task::perform(file::pick(), Message::Opened),
            After::New => self.new_note(),
            After::Load(path) => self.load(path),
        }
    }

    /// The file at `path` in place of the current note.
    pub(crate) fn load(&mut self, path: PathBuf) -> Task<Message> {
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
    pub(crate) fn new_note(&mut self) -> Task<Message> {
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
