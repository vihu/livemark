//! Links between notes (PLAN-004 answer 7): plain markdown links,
//! `[Lisbon hotels](2026-10-02-lisbon-hotels.md)`, picked from a list
//! after `[[` (and tags after `#`), opened in the app when clicked, and
//! the notes linking to the open one listed in the sidebar.
use std::path::{Path, PathBuf};

use iced::widget::text::{Ellipsis, Wrapping};
use iced::widget::{button, column, lazy, text};
use iced::{Element, Length, Theme};
use livemark::widget::{Choice, Complete};

use super::vault::{self, Vault};
use super::{App, Message};

/// The most notes listed as linking here.
const LINKED: usize = 8;

impl App {
    /// The editor's completion answered from the vault, when what is
    /// being typed changed: notes by title after `[[`, tags after `#`.
    pub(crate) fn offer_choices(&mut self) {
        let completing = self.editor.completing();
        if completing == self.completing {
            return;
        }
        self.completing = completing.clone();
        let (Some(completing), Some(vault)) = (completing, &self.vault) else {
            return;
        };
        let choices = match completing.kind {
            Complete::Link => {
                let from = self.path.clone().unwrap_or_else(|| vault.root.join("_"));
                link_choices(vault, &from, &completing.query)
            }
            Complete::Tag => {
                let query = completing.query.to_lowercase();
                vault
                    .tags()
                    .into_iter()
                    .filter(|(tag, _)| tag.starts_with(&query) && *tag != query)
                    .map(|(tag, count)| Choice {
                        label: format!("#{tag}"),
                        detail: crate::tag_actions::notes(count),
                        insert: format!("#{tag}"),
                    })
                    .collect()
            }
        };
        self.editor.set_choices(choices);
    }

    /// The note a clicked link points to, when it is a markdown file that
    /// exists: opened here rather than in the browser.
    pub(crate) fn note_link(&self, dest: &str) -> Option<PathBuf> {
        let from = self
            .path
            .clone()
            .or_else(|| self.vault.as_ref().map(|vault| vault.root.join("_")))?;
        let path = vault::resolve(&from, dest)?;
        path.is_file()
            .then(|| std::fs::canonicalize(&path).unwrap_or(path))
    }

    /// The notes linking to the open one, under the sidebar's list.
    pub(crate) fn linked_from(&self) -> Option<Element<'_, Message>> {
        let (vault, path) = (self.vault.as_ref()?, self.path.as_ref()?);
        if vault.linked_from(path).is_empty() {
            return None;
        }
        Some(
            lazy((vault.generation, path.clone()), move |(_, path)| {
                linked_list(vault, path)
            })
            .into(),
        )
    }
}

/// Notes matching `query` by title, as links from the note at `from`.
pub(crate) fn link_choices(vault: &Vault, from: &Path, query: &str) -> Vec<Choice> {
    let mut found: Vec<(i32, usize, &vault::Note)> = vault
        .notes
        .iter()
        .enumerate()
        .filter(|(_, note)| note.path != from)
        .filter_map(|(rank, note)| Some((crate::search::score(&note.title, query)?, rank, note)))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found
        .into_iter()
        .take(8)
        .map(|(_, _, note)| {
            let title = note.title.replace('[', "\\[").replace(']', "\\]");
            Choice {
                label: note.title.clone(),
                detail: note.created.clone().unwrap_or_default(),
                insert: format!("[{title}]({})", relative(from, &note.path)),
            }
        })
        .collect()
}

/// The path to `to` from the folder of the note at `from`, `../` where
/// needed, spaces as `%20`: a link that still works in the git web UI.
fn relative(from: &Path, to: &Path) -> String {
    let base: Vec<_> = from
        .parent()
        .unwrap_or(Path::new(""))
        .components()
        .collect();
    let target: Vec<_> = to.components().collect();
    let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); base.len() - shared];
    parts.extend(
        target[shared..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/").replace(' ', "%20")
}

fn linked_list(vault: &Vault, path: &Path) -> Element<'static, Message> {
    let notes = vault.linked_from(path);
    let mut list = column![text("Linked from").size(12).style(text::secondary)].spacing(1);
    for note in notes.iter().take(LINKED) {
        list = list.push(
            button(
                text(note.title.clone())
                    .size(13)
                    .wrapping(Wrapping::None)
                    .ellipsis(Ellipsis::End),
            )
            .width(Length::Fill)
            .padding([3, 8])
            .style(|theme: &Theme, status| super::sidebar::choice(theme, status, false))
            .on_press(Message::Opened(Some(note.path.clone()))),
        );
    }
    if notes.len() > LINKED {
        list = list.push(
            text(format!("and {} more", notes.len() - LINKED))
                .size(11)
                .style(text::secondary),
        );
    }
    list.into()
}

#[cfg(test)]
mod tests {
    use super::relative;
    use std::path::Path;

    #[test]
    fn links_between_notes_are_relative() {
        let at = |from: &str, to: &str| relative(Path::new(from), Path::new(to));
        assert_eq!(at("/v/a.md", "/v/b c.md"), "b%20c.md");
        assert_eq!(at("/v/sub/a.md", "/v/b.md"), "../b.md");
        assert_eq!(at("/v/a.md", "/v/sub/b.md"), "sub/b.md");
    }
}
