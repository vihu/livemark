//! Pictures in the note (PLAN-002): those its images point to, read from
//! files next to it; a pasted one kept in `assets/`; a dropped one linked.
use std::path::PathBuf;

use iced::Task;

use super::{App, Message, file};

impl App {
    /// A pasted picture kept next to the note and linked at the caret
    /// (PLAN-002); a note not saved yet is saved first, asking where.
    pub(crate) fn paste_picture(&mut self, png: Vec<u8>) -> Task<Message> {
        let Some(note) = self.path.clone() else {
            self.waiting_picture = Some(png);
            return self.update(Message::Save { choose: true });
        };
        match file::save_picture(&note, &png, "png") {
            Ok(dest) => {
                self.editor.insert_text(&format!("![]({dest})"));
                self.editor.set_image(&dest, &png);
                self.tried.insert(dest);
            }
            Err(error) => self.error = Some(error),
        }
        Task::none()
    }

    /// A dropped picture is linked at the caret, by its path from the note;
    /// a dropped markdown file opens.
    pub(crate) fn dropped(&mut self, file: PathBuf) -> Task<Message> {
        if file::is_picture(&file) {
            let dest = match &self.path {
                Some(note) => file::link_to(note, &file),
                None => file.to_string_lossy().replace(' ', "%20"),
            };
            self.editor.insert_text(&format!("![]({dest})"));
            if let Ok(bytes) = std::fs::read(&file) {
                self.editor.set_image(&dest, &bytes);
            }
            Task::none()
        } else if file
            .extension()
            .is_some_and(|e| e == "md" || e == "markdown")
        {
            self.update(Message::Opened(Some(file)))
        } else {
            Task::none()
        }
    }

    pub(crate) fn with_images(mut self) -> Self {
        self.load_images();
        self
    }

    /// Hands the editor the pictures its images point to, from files next
    /// to the note (REFERENCE-001 section 5), each destination once.
    pub(crate) fn load_images(&mut self) {
        let Some(note) = self.path.clone() else {
            return;
        };
        for url in self.editor.image_urls() {
            if self.tried.insert(url.clone())
                && let Some(bytes) = file::image_bytes(&note, &url)
            {
                self.editor.set_image(&url, &bytes);
            }
        }
    }
}
