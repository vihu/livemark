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

/// Writes the notes' new texts, keeping what each file held for Undo.
pub fn write(changes: Vec<(PathBuf, String)>) -> Result<Undo, String> {
    let mut files = Vec::new();
    for (path, after) in changes {
        let before =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        file::save(&path, &after).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push((path, before, after));
    }
    Ok(Undo {
        files,
        ..Undo::default()
    })
}

impl App {
    /// Takes the last change back; says how many notes were changed since
    /// and kept as they are.
    pub(crate) fn undo_last(&mut self) -> Result<usize, String> {
        let Some(undo) = self.undo.take() else {
            return Ok(0);
        };
        let open = self.path.clone();
        let touched_open = open.as_deref().is_some_and(|open| undo.touches(open));
        if touched_open && self.unsaved() {
            self.undo = Some(undo);
            return Err("Save the open note first, then undo".into());
        }
        let kept = undo.restore()?;
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
                    self.editor = self.editor_for(String::new());
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

    /// The note a change across the vault leaves, with its Undo; or a word
    /// on something done, with only its cross.
    pub(crate) fn undo_toast(&self) -> Option<Element<'_, Message>> {
        let said = self.toast.as_ref()?;
        Some(
            container(
                row![text(said.as_str()).size(13)]
                    .push((self.toast_undo && self.undo.is_some()).then(|| {
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
