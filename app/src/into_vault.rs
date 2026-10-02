//! A note outside the open vault (PLAN-006): a bar over it says so, with
//! Move into vault (the file goes to the vault's folder) and Copy into
//! vault (the original stays where it is, the copy opens); either adds
//! front matter when the note has none (its title from its first heading
//! or its name, today as created), and can be undone. The cross hides the
//! bar for that file until another opens.
use std::path::{Path, PathBuf};

use iced::widget::{Space, button, container, row, text};
use iced::{Element, Length, Task, Theme};

use super::icons::{Icon, Tone, icon};
use super::shell::{file_name, folder};
use super::undo::Undo;
use super::{App, Message};

#[derive(Debug, Clone, Copy)]
pub enum IntoVault {
    Move,
    Copy,
    Dismiss,
}

impl App {
    pub(crate) fn bring_in_update(&mut self, message: IntoVault) -> Task<Message> {
        let Some(path) = self.path.clone() else {
            return Task::none();
        };
        if let IntoVault::Dismiss = message {
            self.outside_dismissed = Some(path);
            return Task::none();
        }
        if self.unsaved() {
            self.error = Some("Save the note first, then move or copy it".into());
            return Task::none();
        }
        let moving = matches!(message, IntoVault::Move);
        match self.bring_in(&path, moving) {
            Ok(target) => {
                self.toast = Some(format!(
                    "{} into the vault as {}",
                    if moving { "Moved" } else { "Copied" },
                    file_name(&target)
                ));
                self.toast_undo = true;
                self.error = None;
                if moving {
                    self.path = Some(target);
                    self.refresh_vault();
                    self.reload();
                } else {
                    self.refresh_vault();
                    return self.load(target);
                }
            }
            Err(error) => self.error = Some(error),
        }
        Task::none()
    }

    /// The note at `path` moved or copied into the vault's folder, front
    /// matter added when it has none; its Undo kept. Returns where it went.
    fn bring_in(&mut self, path: &Path, moving: bool) -> Result<PathBuf, String> {
        let root = self.vault.as_ref().ok_or("No vault is open")?.root.clone();
        let failed = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
        let text = std::fs::read_to_string(path).map_err(|e| failed(path, e))?;
        let text = with_front_matter(&text, path);
        let name = path
            .file_name()
            .map_or_else(|| "note.md".into(), |n| n.to_string_lossy().into_owned());
        let stem = name.trim_end_matches(".md").to_owned();
        let target = (1..)
            .map(|n| match n {
                1 => root.join(&name),
                n => root.join(format!("{stem}-{n}.md")),
            })
            .find(|candidate| !candidate.exists())
            .unwrap_or_else(|| root.join(&name));
        let undo = if moving {
            let mut undo = super::undo::write(vec![(path.to_path_buf(), text)])?;
            move_file(path, &target).map_err(|e| failed(path, e))?;
            undo.moved = Some((path.to_path_buf(), target.clone()));
            undo
        } else {
            let write = || -> std::io::Result<()> {
                use std::io::Write;
                std::fs::File::create_new(&target)?.write_all(text.as_bytes())
            };
            write().map_err(|e| failed(&target, e))?;
            Undo {
                created: Some(target.clone()),
                back: Some(path.to_path_buf()),
                ..Undo::default()
            }
        };
        self.undo = Some(undo);
        Ok(target)
    }

    /// The bar over a note outside the vault, unless dismissed for it.
    pub(crate) fn outside_bar(&self) -> Option<Element<'_, Message>> {
        let path = self.path.as_deref()?;
        if !self.outside_vault(path) || self.outside_dismissed.as_deref() == Some(path) {
            return None;
        }
        let said = format!(
            "{} is outside your vault, in {}: no tags, search or links for it.",
            file_name(path),
            folder(path)
        );
        let send = |message| Message::IntoVault(message);
        Some(
            container(
                row![
                    icon(Icon::Folder, 16.0, Tone::Quiet),
                    text(said).size(13).width(Length::Fill),
                    button(text("Move into vault").size(13))
                        .padding([5, 12])
                        .style(button::primary)
                        .on_press(send(IntoVault::Move)),
                    button(text("Copy into vault").size(13))
                        .padding([5, 12])
                        .style(button::secondary)
                        .on_press(send(IntoVault::Copy)),
                    button(text("\u{d7}").size(15))
                        .padding([2, 8])
                        .style(button::text)
                        .on_press(send(IntoVault::Dismiss)),
                ]
                .spacing(10)
                .align_y(iced::Center),
            )
            .padding([8, 12])
            .style(|theme: &Theme| {
                let warning = theme.palette().warning.base.color;
                container::Style {
                    background: Some(warning.scale_alpha(0.14).into()),
                    border: iced::Border {
                        color: warning.scale_alpha(0.5),
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    ..container::Style::default()
                }
            })
            .into(),
        )
    }

    /// The bar's place over the note: always there, empty without it, so
    /// the editor keeps its place in the widget tree.
    pub(crate) fn outside_place(&self) -> Element<'_, Message> {
        match self.outside_bar() {
            Some(bar) => container(bar).padding([10, 16]).into(),
            None => container(Space::new().height(0)).into(),
        }
    }
}

/// `text` with front matter when it has none: its first `#` heading or the
/// file's name as the title, today as created.
fn with_front_matter(text: &str, path: &Path) -> String {
    if livemark::parse::properties::properties(text).is_some() {
        return text.to_owned();
    }
    let title = text
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| file_name(path).trim_end_matches(".md").to_owned());
    let header = super::note::Header {
        title: &title,
        tags: &[],
        created: &super::note::today(),
        by: None,
    };
    let mut front = super::note::front_matter(&header);
    if !text.is_empty() {
        front.push('\n');
    }
    front + text
}

/// Moves a file, across file systems too (a copy, then the original
/// removed).
pub(crate) fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}
