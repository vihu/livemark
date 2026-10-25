//! Changes to the vault's files and their Undo (PLAN-005, PLAN-006): a tag
//! renamed, merged or deleted across notes; a note renamed (its file moved,
//! the links to it rewritten), duplicated or deleted. Undo puts back what
//! the change did, each file only while it is as the change left it, for
//! as long as the app is open; the note low over the text offers it.
use std::path::{Path, PathBuf};

use iced::widget::{button, container, row, text};
use iced::{Element, Theme};

use super::{App, Message, file};

/// What a change did, to be put back.
#[derive(Debug, Clone, Default)]
pub struct Undo {
    /// Files written: as they were and as the change left them.
    pub files: Vec<(PathBuf, String, String)>,
    /// A file moved, from and to, after `files` were written.
    pub moved: Option<(PathBuf, PathBuf)>,
    /// A file the change made.
    pub created: Option<PathBuf>,
    /// A file the change removed, with its text.
    pub deleted: Option<(PathBuf, String)>,
    /// The note open before the change, open again when Undo removes the
    /// one it made.
    pub back: Option<PathBuf>,
}

impl Undo {
    /// How many notes the change wrote.
    pub fn count(&self) -> usize {
        self.files.len()
    }

    /// Whether the change touched the file at `path`.
    pub fn touches(&self, path: &Path) -> bool {
        self.files.iter().any(|(p, ..)| p == path)
            || self
                .moved
                .as_ref()
                .is_some_and(|(from, to)| from == path || to == path)
            || self.created.as_deref() == Some(path)
            || self.deleted.as_ref().is_some_and(|(p, _)| p == path)
    }

    /// Puts back what the change did: a moved file back first (the texts
    /// were its before the move), then each written file while it is as
    /// the change left it, a made file removed, a removed one written
    /// again. Returns how many were changed since and kept.
    pub fn restore(&self) -> Result<usize, String> {
        let failed = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
        if let Some((from, to)) = &self.moved
            && to.exists()
            && !from.exists()
        {
            super::into_vault::move_file(to, from).map_err(|e| failed(to, e))?;
        }
        let mut kept = 0;
        for (path, before, after) in &self.files {
            match std::fs::read_to_string(path) {
                Ok(now) if now == *after => {
                    file::save(path, before).map_err(|e| failed(path, e))?
                }
                _ => kept += 1,
            }
        }
        if let Some(path) = &self.created
            && path.exists()
        {
            std::fs::remove_file(path).map_err(|e| failed(path, e))?;
        }
        if let Some((path, text)) = &self.deleted
            && !path.exists()
        {
            file::save(path, text).map_err(|e| failed(path, e))?;
        }
        Ok(kept)
    }
}

/// Writes each file's new text: `(file, the text the change was worked
/// out from, the new text)`. Every file is read first, and nothing is
/// written unless each still holds the text the change started from (a
/// change on disk since the vault read it is never written over). When a
/// write fails part way, the error comes with the Undo of the files
/// written so far, for the caller to keep.
pub fn write(changes: Vec<(PathBuf, String, String)>) -> Result<Undo, Box<(Undo, String)>> {
    let nothing = |error: String| Box::new((Undo::default(), error));
    for (path, before, _) in &changes {
        let now = std::fs::read_to_string(path)
            .map_err(|e| nothing(format!("{}: {e}", path.display())))?;
        if now != *before {
            return Err(nothing(format!(
                "{} changed on disk since it was read; nothing was written",
                path.display()
            )));
        }
    }
    let mut undo = Undo::default();
    for (path, before, after) in changes {
        if let Err(e) = file::save(&path, &after) {
            return Err(Box::new((undo, format!("{}: {e}", path.display()))));
        }
        undo.files.push((path, before, after));
    }
    Ok(undo)
}

impl App {
    /// Takes the last change back; says how many notes were changed since
    /// and kept as they are.
    pub(crate) fn undo_last(&mut self) -> Result<usize, String> {
        let Some(undo) = self.vault_view.undo.take() else {
            return Ok(0);
        };
        let open = self.path.clone();
        let touched_open = open.as_deref().is_some_and(|open| undo.touches(open));
        if touched_open && self.unsaved() {
            self.vault_view.undo = Some(undo);
            return Err("Save the open note first, then undo".into());
        }
        // A restore that fails part way can be tried again: each step
        // checks the file is still as the change left it.
        let kept = match undo.restore() {
            Ok(kept) => kept,
            Err(error) => {
                self.vault_view.undo = Some(undo);
                self.refresh_vault();
                return Err(error);
            }
        };
        // The open note follows its file back.
        if let Some((from, to)) = &undo.moved
            && open.as_ref() == Some(to)
        {
            self.path = Some(from.clone());
        }
        self.refresh_vault();
        if undo.created.is_some() && undo.created == open {
            // The copy that was open is gone: the note it came from, or an
            // empty one, in its place.
            match &undo.back {
                Some(back) => {
                    let _ = self.load(back.clone());
                }
                None => {
                    self.path = None;
                    self.replace_editor(String::new());
                    self.saved = self.editor.version();
                }
            }
        } else if let Some((path, _)) = &undo.deleted
            && self.path.is_none()
            && !self.unsaved()
        {
            let _ = self.load(path.clone());
        } else if touched_open {
            self.reload();
        }
        Ok(kept)
    }

    /// What a change across the vault wrote, or why it did not: one that
    /// failed part way keeps the Undo of the files it wrote, offered in
    /// the note over the text, the error in the bar.
    pub(crate) fn written(
        &mut self,
        written: Result<Undo, Box<(Undo, String)>>,
    ) -> Result<Undo, String> {
        written.map_err(|failed| {
            let (partial, error) = *failed;
            self.keep_partial(partial);
            error
        })
    }

    /// Shows `said` in the note over the text, with Undo when `undo`.
    pub(crate) fn show_toast(&mut self, said: String, undo: bool) {
        self.vault_view.toast = Some(super::Toast { said, undo });
    }

    /// What the note over the text says, while it shows (for the tests).
    #[cfg(test)]
    pub(crate) fn toast(&self) -> Option<&str> {
        self.vault_view
            .toast
            .as_ref()
            .map(|toast| toast.said.as_str())
    }

    /// The Undo of what a failed change wrote, when it wrote anything.
    pub(crate) fn keep_partial(&mut self, partial: Undo) {
        if partial.count() == 0 && partial.moved.is_none() {
            return;
        }
        let said = format!(
            "Stopped part way: {} written",
            super::tag_actions::notes(partial.count())
        );
        self.show_toast(said, true);
        self.vault_view.undo = Some(partial);
        self.refresh_vault();
    }

    /// The note a change across the vault leaves, with its Undo; or a word
    /// on something done, with only its cross.
    pub(crate) fn undo_toast(&self) -> Option<Element<'_, Message>> {
        let toast = self.vault_view.toast.as_ref()?;
        Some(
            container(
                row![text(toast.said.as_str()).size(13)]
                    .push((toast.undo && self.vault_view.undo.is_some()).then(|| {
                        button(text("Undo").size(13))
                            .padding([4, 10])
                            .on_press(Message::Undo)
                    }))
                    .push(
                        button(text("\u{d7}").size(14))
                            .padding([2, 8])
                            .style(|theme: &Theme, _| button::Style {
                                text_color: theme.palette().background.base.color,
                                ..button::Style::default()
                            })
                            .on_press(Message::DismissToast),
                    )
                    .spacing(12)
                    .align_y(iced::Center),
            )
            .padding([8, 10])
            .style(|theme: &Theme| {
                let palette = theme.palette();
                container::Style {
                    background: Some(palette.background.base.text.into()),
                    text_color: Some(palette.background.base.color),
                    border: iced::Border::default().rounded(10),
                    ..container::Style::default()
                }
            })
            .into(),
        )
    }
}
