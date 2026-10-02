//! Links between notes (PLAN-004 answer 7): plain markdown links,
//! `[Lisbon hotels](2026-10-02-lisbon-hotels.md)`, picked from a list
//! after `[[` (and tags after `#`), opened in the app when clicked, and
//! the notes linking to the open one listed under it (`Editor::set_footer`).
use std::path::{Path, PathBuf};

use livemark::widget::{Choice, Complete, FooterLink};

use super::App;
use super::vault::{self, Vault};

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

    /// The notes linking to the open one, under it (PLAN-005): each with
    /// the line its link sits in, a click opening it.
    pub(crate) fn refresh_footer(&mut self) {
        let links = match (&self.vault, &self.path) {
            (Some(vault), Some(path)) => footer_links(vault, path),
            _ => Vec::new(),
        };
        self.editor.set_footer("Linked from", links);
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
pub(crate) fn relative(from: &Path, to: &Path) -> String {
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

/// The notes linking to the note at `path`, most recent first, as links
/// from it with the line each link sits in.
pub(crate) fn footer_links(vault: &Vault, path: &Path) -> Vec<FooterLink> {
    vault
        .linked_from(path)
        .into_iter()
        .take(LINKED)
        .map(|note| FooterLink {
            label: note.title.clone(),
            detail: context(&note.text, &note.path, path),
            destination: relative(path, &note.path),
        })
        .collect()
}

/// The line of `text` (the note at `from`) with its first link to `to`:
/// that link's text in place of its markdown, list and quote markup off.
fn context(text: &str, from: &Path, to: &Path) -> String {
    let spans = livemark::parse::link_spans(text);
    let Some((range, _)) = spans
        .iter()
        .find(|(_, dest)| vault::resolve(from, dest).as_deref() == Some(to))
    else {
        return String::new();
    };
    let start = text[..range.start].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let end = range.end
        + text[range.end..]
            .find(['\n', '\r'])
            .unwrap_or(text.len() - range.end);
    let link = &text[range.clone()];
    let label = link
        .strip_prefix('[')
        .and_then(|rest| rest.split("](").next())
        .map_or(link, |label| label.split("][").next().unwrap_or(label));
    let line = format!(
        "{}{label}{}",
        &text[start..range.start],
        &text[range.end..end]
    );
    let line = line
        .trim_start_matches([' ', '\t', '>', '-', '*', '+'])
        .trim_start();
    let line = ["[ ] ", "[x] ", "[X] "]
        .iter()
        .find_map(|task| line.strip_prefix(task))
        .unwrap_or(line);
    line.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{context, relative};
    use std::path::Path;

    #[test]
    fn links_between_notes_are_relative() {
        let at = |from: &str, to: &str| relative(Path::new(from), Path::new(to));
        assert_eq!(at("/v/a.md", "/v/b c.md"), "b%20c.md");
        assert_eq!(at("/v/sub/a.md", "/v/b.md"), "../b.md");
        assert_eq!(at("/v/a.md", "/v/sub/b.md"), "sub/b.md");
    }

    #[test]
    fn the_linking_line_reads_as_text() {
        let line = |text: &str| context(text, Path::new("/v/a.md"), Path::new("/v/b.md"));
        assert_eq!(
            line("# A\n- [ ] Book it, see [the hotels](b.md#rooms) first.\nmore\n"),
            "Book it, see the hotels first."
        );
        assert_eq!(line("> [B][b] says so\n\n[b]: b.md\n"), "B says so");
        assert_eq!(line("no link here\n"), "");
    }
}
