//! What can be done to a note from the sidebar (PLAN-006), from its menu
//! (`context.rs`: Open, Rename, Duplicate, Copy link, Show in folder,
//! Delete), F2 renaming the open note. A rename changes the title and the
//! file name (its date kept) and rewrites the links to it in other notes;
//! a delete says first how many notes link to it. Each can be undone.
use std::path::{Path, PathBuf};

use iced::widget::{column, text_input};
use iced::{Element, Task};

use super::links::relative;
use super::relink::{free_path, relinked, retitled};
use super::sidebar::Shown;
use super::tag_actions::{notes, question};
use super::undo::Undo;
use super::vault::Vault;
use super::{App, Message};

/// The rename field, focused when it opens.
const RENAME: iced::widget::Id = iced::widget::Id::new("livemark-note-rename");

#[derive(Debug, Clone)]
pub enum NoteMessage {
    /// Rename this note; `None` (F2) the open one.
    StartRename(Option<PathBuf>),
    RenameText(String),
    StartDelete(PathBuf),
    /// Rename or delete, as the open question says.
    Confirm,
    Cancel,
    Duplicate(PathBuf),
    CopyLink(PathBuf),
    ShowInFolder(PathBuf),
}

/// The question open under a note.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NoteAction {
    Rename { path: PathBuf, value: String },
    Delete(PathBuf),
}

impl App {
    pub(crate) fn note_update(&mut self, message: NoteMessage) -> Task<Message> {
        match message {
            NoteMessage::StartRename(path) => {
                // F2 (no path): the open note, whose row may be out of sight.
                let scrolls = path.is_none();
                let Some(path) = path.or_else(|| self.path.clone()) else {
                    return Task::none();
                };
                let Some(title) = self.title_of(&path) else {
                    return Task::none();
                };
                // Its row in sight: the sidebar shown, the note listed.
                self.settings.sidebar = true;
                self.listing = None;
                if !self.shown_keeps(&path) {
                    self.shown = Shown::All;
                }
                let scroll = if scrolls {
                    self.bring_into_view(&path)
                } else {
                    Task::none()
                };
                self.note_action = Some(NoteAction::Rename { path, value: title });
                return Task::batch([scroll, iced::widget::operation::focus(RENAME)]);
            }
            NoteMessage::RenameText(value) => {
                if let Some(NoteAction::Rename { value: v, .. }) = &mut self.note_action {
                    *v = value;
                }
            }
            NoteMessage::StartDelete(path) => self.note_action = Some(NoteAction::Delete(path)),
            NoteMessage::Cancel => self.note_action = None,
            NoteMessage::Confirm => {
                let result = match self.note_action.take() {
                    Some(NoteAction::Rename { path, value }) => {
                        let title = value.trim().to_owned();
                        if title.is_empty() || Some(&title) == self.title_of(&path).as_ref() {
                            return Task::none();
                        }
                        self.rename_note(&path, &title).map(|links| match links {
                            0 => format!("Renamed to {title}"),
                            1 => format!("Renamed to {title}: 1 link follows"),
                            n => format!("Renamed to {title}: {n} links follow"),
                        })
                    }
                    Some(NoteAction::Delete(path)) => {
                        let title = self.title_of(&path).unwrap_or_default();
                        self.delete_note(&path).map(|_| format!("Deleted {title}"))
                    }
                    None => return Task::none(),
                };
                self.said(result);
            }
            NoteMessage::Duplicate(path) => match self.duplicate_note(&path) {
                Ok((copy, title)) => {
                    self.said(Ok(format!("Duplicated as {title}")));
                    return self.update(Message::Opened(Some(copy)));
                }
                Err(error) => self.error = Some(error),
            },
            NoteMessage::CopyLink(path) => {
                let (Some(vault), Some(title)) = (&self.vault, self.title_of(&path)) else {
                    return Task::none();
                };
                let from = self.path.clone().unwrap_or_else(|| vault.root.join("_"));
                let label = title.replace('[', "\\[").replace(']', "\\]");
                let link = format!("[{label}]({})", relative(&from, &path));
                self.toast = Some(format!("Copied a link to {title}"));
                self.toast_undo = false;
                return iced::clipboard::write(link).discard();
            }
            NoteMessage::ShowInFolder(path) => {
                if let Err(error) = show_in_folder(&path) {
                    self.error = Some(error);
                }
            }
        }
        Task::none()
    }

    /// A change done (its toast with Undo) or refused (the error bar).
    fn said(&mut self, result: Result<String, String>) {
        match result {
            Ok(said) => {
                self.toast = Some(said);
                self.toast_undo = true;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn title_of(&self, path: &Path) -> Option<String> {
        let vault = self.vault.as_ref()?;
        let note = vault.notes.iter().find(|n| n.path == path)?;
        Some(note.title.clone())
    }

    /// The note list scrolled so the row of `path` is at its top, with its
    /// menu or question under it in sight.
    fn bring_into_view(&self, path: &Path) -> Task<Message> {
        let Some(vault) = &self.vault else {
            return Task::none();
        };
        let Some(index) = vault
            .notes
            .iter()
            .filter(|note| self.shown.keeps(note))
            .position(|note| note.path == path)
        else {
            return Task::none();
        };
        iced::widget::operation::scroll_to(
            super::sidebar::NOTES,
            iced::widget::scrollable::AbsoluteOffset {
                x: None,
                y: Some(index as f32 * super::sidebar::NOTE_ROW),
            },
            iced::widget::operation::Animation::Instant,
        )
    }

    fn shown_keeps(&self, path: &Path) -> bool {
        self.vault.as_ref().is_some_and(|vault| {
            vault
                .notes
                .iter()
                .any(|note| note.path == path && self.shown.keeps(note))
        })
    }

    /// Refused while the open note, among `paths`, has unsaved changes.
    fn guard(&self, paths: &[&Path]) -> Result<(), String> {
        let open = self.path.as_deref();
        if self.unsaved() && open.is_some_and(|open| paths.contains(&open)) {
            return Err("Save the open note first".into());
        }
        Ok(())
    }

    /// The note at `path` titled `title`: its title in its text, its file
    /// renamed (its date kept), the links to it in other notes rewritten;
    /// one Undo. Returns how many links followed.
    pub(crate) fn rename_note(&mut self, path: &Path, title: &str) -> Result<usize, String> {
        let Some(vault) = &mut self.vault else {
            return Err("No vault is open".into());
        };
        vault.refresh();
        let Some(note) = vault.notes.iter().find(|n| n.path == path) else {
            return Err(format!("{} is not in the vault", path.display()));
        };
        let new_path = free_path(path, title);
        let mut changes: Vec<(PathBuf, String)> = Vec::new();
        if let Some(text) = retitled(&note.text, &note.title, title) {
            changes.push((path.to_path_buf(), text));
        }
        let mut links = 0;
        for other in vault.linked_from(path) {
            if let Some((text, n)) = relinked(&other.text, &other.path, path, &new_path) {
                changes.push((other.path.clone(), text));
                links += n;
            }
        }
        let touched: Vec<&Path> = changes.iter().map(|(p, _)| p.as_path()).collect();
        self.guard(&[touched.as_slice(), &[path]].concat())?;
        let mut undo = super::undo::write(changes)?;
        if new_path != path {
            std::fs::rename(path, &new_path).map_err(|e| format!("{}: {e}", path.display()))?;
            undo.moved = Some((path.to_path_buf(), new_path.clone()));
        }
        let open_touched = self.path.as_deref().is_some_and(|open| undo.touches(open));
        if self.path.as_deref() == Some(path) {
            self.path = Some(new_path.clone());
        }
        self.undo = Some(undo);
        for recent in &mut self.settings.recent {
            if recent == path {
                *recent = new_path.clone();
            }
        }
        self.remember();
        self.refresh_vault();
        if open_touched {
            self.reload();
        }
        Ok(links)
    }

    /// Removes the note at `path`, its text kept for Undo; the open note
    /// gives way to an empty one.
    pub(crate) fn delete_note(&mut self, path: &Path) -> Result<(), String> {
        self.guard(&[path])?;
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        self.undo = Some(Undo {
            deleted: Some((path.to_path_buf(), text)),
            ..Undo::default()
        });
        if self.path.as_deref() == Some(path) {
            self.path = None;
            self.editor = self.editor_for(String::new());
            self.saved = self.editor.version();
            self.stamp = None;
        }
        self.settings.recent.retain(|recent| recent != path);
        self.remember();
        self.refresh_vault();
        Ok(())
    }

    /// A copy of the note at `path` titled "... copy", next to it; Undo
    /// removes it. Returns its file and title.
    fn duplicate_note(&mut self, path: &Path) -> Result<(PathBuf, String), String> {
        let vault = self.vault.as_ref().ok_or("No vault is open")?;
        let note = vault
            .notes
            .iter()
            .find(|n| n.path == path)
            .ok_or_else(|| format!("{} is not in the vault", path.display()))?;
        let title = format!("{} copy", note.title);
        let copy = free_path(path, &title);
        let text = retitled(&note.text, &note.title, &title).unwrap_or_else(|| note.text.clone());
        let write = || -> std::io::Result<()> {
            use std::io::Write;
            std::fs::File::create_new(&copy)?.write_all(text.as_bytes())
        };
        write().map_err(|e| format!("{}: {e}", copy.display()))?;
        self.undo = Some(Undo {
            created: Some(copy.clone()),
            back: Some(path.to_path_buf()),
            ..Undo::default()
        });
        self.refresh_vault();
        Ok((copy, title))
    }
}

/// The rename field in place of a note's row, and what will happen.
pub(crate) fn rename_view(vault: &Vault, path: &Path, value: &str) -> Element<'static, Message> {
    let title = value.trim();
    let current = vault
        .notes
        .iter()
        .find(|n| n.path == path)
        .map(|n| n.title.as_str());
    let said = if title.is_empty() || Some(title) == current {
        "Type a new title; the file is renamed with it.".to_owned()
    } else {
        let file = free_path(path, title);
        let name = file
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let linking = vault.linked_from(path).len();
        match linking {
            0 => format!("Renames the file to {name}."),
            1 => format!("Renames the file to {name}. 1 note links here: its link follows."),
            n => format!("Renames the file to {name}. {n} notes link here: their links follow."),
        }
    };
    column![
        text_input("Title", value.to_owned())
            .id(RENAME)
            .on_input(|value| Message::Note(NoteMessage::RenameText(value)))
            .on_submit(Message::Note(NoteMessage::Confirm))
            .size(14)
            .padding([4, 8]),
        question(
            said,
            "Rename",
            false,
            Message::Note(NoteMessage::Confirm),
            Message::Note(NoteMessage::Cancel),
        ),
    ]
    .spacing(4)
    .into()
}

/// The question under a note's row before it is deleted.
pub(crate) fn delete_view(vault: &Vault, path: &Path, title: &str) -> Element<'static, Message> {
    let said = match vault.linked_from(path).len() {
        0 => format!("Delete {title}?"),
        1 => format!("Delete {title}? 1 note links to it; that link will point nowhere."),
        n => format!(
            "Delete {title}? {} link to it; their links will point nowhere.",
            notes(n)
        ),
    };
    question(
        said,
        "Delete",
        true,
        Message::Note(NoteMessage::Confirm),
        Message::Note(NoteMessage::Cancel),
    )
}

/// Shows the file in the system's file manager: selected on macOS, its
/// folder opened elsewhere.
fn show_in_folder(path: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(path));
        command
    };
    let mut child = command
        .spawn()
        .map_err(|e| format!("Show in folder: {e}"))?;
    // Reaped in the background, so no zombie is left behind.
    std::thread::spawn(move || child.wait());
    Ok(())
}
