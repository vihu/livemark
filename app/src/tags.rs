//! Tag edits across the vault (PLAN-005): rename, merge (a rename onto a
//! tag a note already has) and delete, in the front matter's `tags` list
//! (`[a, b]`, `a, b` or `- a` lines) and in inline `#tags`. Only the tag's
//! own bytes change, so a note stays as it was written otherwise (line
//! endings, quoting, other keys). Every change can be undone while the
//! files are as it left them (`undo.rs`).
use std::ops::Range;
use std::path::PathBuf;

use livemark::parse::properties::{Form, TagList};

/// What to do to a tag (lowercase names, without `#`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// `from` becomes `to`; a note that has `to` already keeps one.
    Rename { from: String, to: String },
    /// Out of every front matter list; inline, the `#` goes, the word
    /// stays.
    Delete(String),
}

impl Edit {
    fn from(&self) -> &str {
        match self {
            Edit::Rename { from, .. } | Edit::Delete(from) => from,
        }
    }
}

/// `text` with `edit` applied, or `None` when nothing in it changes.
pub fn apply(text: &str, edit: &Edit) -> Option<String> {
    let mut changes = list_changes(text, edit);
    for range in livemark::parse::tags(text) {
        if text[range.start + 1..range.end].to_lowercase() != edit.from() {
            continue;
        }
        match edit {
            Edit::Rename { to, .. } => changes.push((range.start + 1..range.end, to.clone())),
            Edit::Delete(_) => changes.push((range.start..range.start + 1, String::new())),
        }
    }
    if changes.is_empty() {
        return None;
    }
    changes.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (range, with) in changes {
        out.push_str(&text[at..range.start]);
        out.push_str(&with);
        at = range.end;
    }
    out.push_str(&text[at..]);
    (out != text).then_some(out)
}

/// An item's text with its name replaced, its quotes and `#` kept.
fn renamed(item: &str, to: &str) -> String {
    let start = item
        .find(|c: char| !matches!(c, '"' | '\'' | '#'))
        .unwrap_or(0);
    let end = item
        .rfind(|c: char| !matches!(c, '"' | '\''))
        .map_or(item.len(), |i| {
            i + item[i..].chars().next().map_or(1, char::len_utf8)
        });
    format!("{}{to}{}", &item[..start], &item[end.max(start)..])
}

/// The changes to the front matter's `tags` list, as the library reads it
/// (`livemark::parse::properties`). A rename onto a name the list lacks
/// renames the first copy; every other copy of the name goes.
fn list_changes(text: &str, edit: &Edit) -> Vec<(Range<usize>, String)> {
    let Some(list) = livemark::parse::properties::properties(text).and_then(|p| p.tags) else {
        return Vec::new();
    };
    let names: Vec<String> = list.items.iter().map(|i| i.name.to_lowercase()).collect();
    let mut gone: Vec<usize> = (0..names.len())
        .filter(|&i| names[i] == edit.from())
        .collect();
    let mut changes = Vec::new();
    if let Edit::Rename { to, .. } = edit
        && !names.contains(to)
        && !gone.is_empty()
    {
        let item = &list.items[gone.remove(0)];
        changes.push((item.range.clone(), renamed(&text[item.range.clone()], to)));
    }
    changes.extend(cuts(&list, &gone).into_iter().map(|r| (r, String::new())));
    changes
}

/// What taking the items at `gone` (ascending) out of `list` removes: a
/// `- ` item's whole line; in `[a, b]` or `a, b`, each run of neighbours
/// with the comma and spaces up to the next item, or after the one before
/// when the run ends the list. Runs are apart, so the cuts never overlap.
fn cuts(list: &TagList, gone: &[usize]) -> Vec<Range<usize>> {
    let items = &list.items;
    if matches!(list.form, Form::Block { .. }) {
        return gone.iter().map(|&i| items[i].cut.clone()).collect();
    }
    let mut cuts = Vec::new();
    let mut i = 0;
    while i < gone.len() {
        let first = gone[i];
        while i + 1 < gone.len() && gone[i + 1] == gone[i] + 1 {
            i += 1;
        }
        let last = gone[i];
        i += 1;
        cuts.push(if last + 1 < items.len() {
            items[first].range.start..items[last + 1].range.start
        } else if first > 0 {
            items[first - 1].range.end..items[last].range.end
        } else {
            items[first].range.start..items[last].range.end
        });
    }
    cuts
}

impl super::App {
    /// Carries out `edits` across the vault, in turn, as one change with
    /// one Undo (a merge of several tags); the open note is loaded again
    /// when it changed. Refused while the open note has unsaved changes the
    /// edits would touch.
    pub(crate) fn edit_tags(&mut self, edits: Vec<Edit>) -> Result<usize, String> {
        let Some(vault) = &mut self.vault else {
            return Err("No vault is open".into());
        };
        vault.refresh();
        // Each note's text as the edits before left it.
        let mut texts: std::collections::BTreeMap<PathBuf, String> = Default::default();
        for edit in &edits {
            for note in &vault.notes {
                let touched = texts.contains_key(&note.path);
                if !touched && !note.tags.iter().any(|t| t == edit.from()) {
                    continue;
                }
                let current = texts.get(&note.path).unwrap_or(&note.text);
                if let Some(after) = apply(current, edit) {
                    texts.insert(note.path.clone(), after);
                }
            }
        }
        let changes: Vec<(PathBuf, String)> = texts.into_iter().collect();
        let open = self.path.clone();
        let touches_open = open
            .as_ref()
            .is_some_and(|open| changes.iter().any(|(p, _)| p == open));
        if touches_open && self.unsaved() {
            let tags: Vec<String> = edits.iter().map(|e| format!("#{}", e.from())).collect();
            return Err(format!(
                "Save the open note first: it has {}",
                tags.join(" or ")
            ));
        }
        let undo = super::undo::write(changes)?;
        let count = undo.count();
        use super::sidebar::Shown;
        for edit in &edits {
            if self.shown == Shown::Tag(edit.from().to_owned()) {
                self.shown = match edit {
                    Edit::Rename { to, .. } => Shown::Tag(to.clone()),
                    Edit::Delete(_) => Shown::All,
                };
            }
        }
        self.refresh_vault();
        if touches_open {
            self.reload();
        }
        self.undo = Some(undo);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::{Edit, apply};

    fn rename(from: &str, to: &str) -> Edit {
        Edit::Rename {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn renames_touch_only_the_tag_in_every_list_form_and_the_text() {
        let text = "---\ntitle: Trip\ntags: [work, trips, \"#Ideas\"]\n---\nSee #trips and #Trips, not #tripsy.\n";
        assert_eq!(
            apply(text, &rename("trips", "travel")).unwrap(),
            "---\ntitle: Trip\ntags: [work, travel, \"#Ideas\"]\n---\nSee #travel and #travel, not #tripsy.\n"
        );
        assert_eq!(
            apply(text, &rename("ideas", "thoughts")).unwrap(),
            "---\ntitle: Trip\ntags: [work, trips, \"#thoughts\"]\n---\nSee #trips and #Trips, not #tripsy.\n"
        );
        let comma = "---\r\ntags: work, trips\r\n---\r\nbody\r\n";
        assert_eq!(
            apply(comma, &rename("trips", "travel")).unwrap(),
            "---\r\ntags: work, travel\r\n---\r\nbody\r\n"
        );
        let block = "---\ntags:\n  - work\n  - trips\nother: x\n---\n";
        assert_eq!(
            apply(block, &rename("trips", "travel")).unwrap(),
            "---\ntags:\n  - work\n  - travel\nother: x\n---\n"
        );
        assert_eq!(apply("no tags at all\n", &rename("trips", "travel")), None);
    }

    #[test]
    fn a_tag_no_note_has_changes_nothing_in_the_fixtures() {
        let docs = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/docs");
        let mut read = 0;
        for entry in std::fs::read_dir(docs).unwrap().flatten() {
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            read += 1;
            assert_eq!(apply(&text, &rename("no-such-tag", "x")), None);
            assert_eq!(apply(&text, &Edit::Delete("no-such-tag".into())), None);
        }
        assert!(read > 0);
    }

    #[test]
    fn a_merge_keeps_one_and_a_delete_drops_the_hash_in_the_text() {
        // The note has both: the renamed one goes.
        assert_eq!(
            apply(
                "---\ntags: [trips, travel]\n---\n",
                &rename("trips", "travel")
            )
            .unwrap(),
            "---\ntags: [travel]\n---\n"
        );
        assert_eq!(
            apply(
                "---\ntags: [travel, trips]\n---\n",
                &rename("trips", "travel")
            )
            .unwrap(),
            "---\ntags: [travel]\n---\n"
        );
        assert_eq!(
            apply(
                "---\ntags:\n- trips\n- travel\n---\n",
                &rename("trips", "travel")
            )
            .unwrap(),
            "---\ntags:\n- travel\n---\n"
        );
        let delete = Edit::Delete("old".into());
        assert_eq!(
            apply("---\ntags: [a, old, b]\n---\nThe #old plan.\n", &delete).unwrap(),
            "---\ntags: [a, b]\n---\nThe old plan.\n"
        );
        assert_eq!(
            apply("---\ntags: [old]\n---\n", &delete).unwrap(),
            "---\ntags: []\n---\n"
        );
        assert_eq!(
            apply("---\ntags: old\n---\n", &delete).unwrap(),
            "---\ntags: \n---\n"
        );
        // Code and links are not tags.
        assert_eq!(apply("`#old` [x](#old)\n", &delete), None);
    }

    #[test]
    fn copies_of_a_name_side_by_side_go_with_one_comma() {
        // Found in REVIEW-001: two cuts at the list's end overlapped.
        let delete = |name: &str| Edit::Delete(name.into());
        for (text, name, want) in [
            ("[x, Work, work]", "work", "[x]"),
            ("[a, a]", "a", "[]"),
            ("[a, a, x]", "a", "[x]"),
            ("[a, x, a]", "a", "[x]"),
            ("x, a, '#a'", "a", "x"),
        ] {
            let note = format!("---\ntags: {text}\n---\n");
            assert_eq!(
                apply(&note, &delete(name)).unwrap(),
                format!("---\ntags: {want}\n---\n"),
                "{text}"
            );
        }
        assert_eq!(
            apply("---\ntags: [b, a, a]\n---\n", &rename("a", "b")).unwrap(),
            "---\ntags: [b]\n---\n"
        );
        assert_eq!(
            apply("---\ntags: [a, A]\n---\n", &rename("a", "b")).unwrap(),
            "---\ntags: [b]\n---\n",
            "renamed once, the copy gone"
        );
        // Lone `\r` endings, the list not on the first line.
        assert_eq!(
            apply("---\rtitle: x\rtags: [a, b]\r---\r", &delete("a")).unwrap(),
            "---\rtitle: x\rtags: [b]\r---\r"
        );
        assert_eq!(
            apply("---\ntags:\n  - a\n  - a\n---\n", &delete("a")).unwrap(),
            "---\ntags:\n---\n"
        );
    }
}
