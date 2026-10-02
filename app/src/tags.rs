//! Tag edits across the vault (PLAN-005): rename, merge (a rename onto a
//! tag a note already has) and delete, in the front matter's `tags` list
//! (`[a, b]`, `a, b` or `- a` lines) and in inline `#tags`. Only the tag's
//! own bytes change, so a note stays as it was written otherwise (line
//! endings, quoting, other keys). Every change can be undone while the
//! files are as it left them.
use std::ops::Range;
use std::path::PathBuf;

use super::vault::Vault;

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

/// The files an edit changed, as they were and as it left them.
#[derive(Debug, Clone)]
pub struct Undo {
    files: Vec<(PathBuf, String, String)>,
}

impl Undo {
    /// How many notes the edit changed.
    pub fn count(&self) -> usize {
        self.files.len()
    }

    /// Writes the files back as they were, each only while it is as the
    /// edit left it; returns how many were changed since and kept.
    pub fn restore(&self) -> Result<usize, String> {
        let mut kept = 0;
        for (path, before, after) in &self.files {
            match std::fs::read_to_string(path) {
                Ok(now) if now == *after => super::file::save(path, before)
                    .map_err(|e| format!("{}: {e}", path.display()))?,
                _ => kept += 1,
            }
        }
        Ok(kept)
    }
}

/// The notes `edit` changes: their paths and new texts.
pub fn plan(vault: &Vault, edit: &Edit) -> Vec<(PathBuf, String)> {
    vault
        .notes
        .iter()
        .filter(|note| note.tags.iter().any(|t| t == edit.from()))
        .filter_map(|note| Some((note.path.clone(), apply(&note.text, edit)?)))
        .collect()
}

/// Writes what `plan` worked out, keeping what each file held for Undo.
pub fn write(changes: Vec<(PathBuf, String)>) -> Result<Undo, String> {
    let mut files = Vec::new();
    for (path, after) in changes {
        let before =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        super::file::save(&path, &after).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push((path, before, after));
    }
    Ok(Undo { files })
}

/// `text` with `edit` applied, or `None` when nothing in it changes.
pub fn apply(text: &str, edit: &Edit) -> Option<String> {
    let mut changes: Vec<(Range<usize>, String)> = Vec::new();
    if let Some(yaml) = livemark::parse::front_matter_text(text) {
        let start = yaml.as_ptr() as usize - text.as_ptr() as usize;
        changes.extend(
            list_changes(yaml, edit)
                .into_iter()
                .map(|(r, s)| (r.start + start..r.end + start, s)),
        );
    }
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

/// A tag's name as a list item spells it: quotes and `#` off, lowercase.
fn name(item: &str) -> String {
    item.trim()
        .trim_matches(['"', '\''])
        .trim()
        .trim_start_matches('#')
        .to_lowercase()
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

/// The changes to the `tags` list in `yaml`, in its offsets.
fn list_changes(yaml: &str, edit: &Edit) -> Vec<(Range<usize>, String)> {
    let mut changes = Vec::new();
    let mut lines = yaml.split_inclusive('\n').scan(0, |at, line| {
        let start = *at;
        *at += line.len();
        Some((start, line))
    });
    while let Some((start, line)) = lines.next() {
        let content = line.trim_end_matches(['\n', '\r']);
        let Some((key, value)) = content.split_once(':') else {
            continue;
        };
        if !matches!(key.trim(), "tags" | "tag") {
            continue;
        }
        let value_start = start + key.len() + 1;
        if value.trim().is_empty() {
            // A `- item` a line, up to the first line that is not one.
            let items: Vec<(usize, &str)> = lines
                .by_ref()
                .take_while(|(_, l)| l.trim_start().starts_with("- "))
                .collect();
            let names: Vec<String> = items
                .iter()
                .map(|(_, l)| name(l.trim_start()[2..].trim_end_matches(['\n', '\r'])))
                .collect();
            for ((at, item_line), item_name) in items.iter().zip(&names) {
                if *item_name != edit.from() {
                    continue;
                }
                let keep_one = match edit {
                    Edit::Rename { to, .. } => !names.contains(to),
                    Edit::Delete(_) => false,
                };
                if keep_one {
                    let dash = item_line.find("- ").unwrap_or(0) + 2;
                    let body = item_line[dash..].trim_end_matches(['\n', '\r']);
                    let Edit::Rename { to, .. } = edit else {
                        unreachable!()
                    };
                    changes.push((at + dash..at + dash + body.len(), renamed(body, to)));
                } else {
                    changes.push((*at..at + item_line.len(), String::new()));
                }
            }
            continue;
        }
        changes.extend(inline_list(value, value_start, edit));
    }
    changes
}

/// `[a, b]` or `a, b` after `tags:`, `value` starting at `offset`.
fn inline_list(value: &str, offset: usize, edit: &Edit) -> Vec<(Range<usize>, String)> {
    let open = value.find('[');
    let (from, to) = match open {
        Some(i) => (
            i + 1,
            value.rfind(']').filter(|&j| j > i).unwrap_or(value.len()),
        ),
        None => (0, value.len()),
    };
    // Items with their ranges, the spaces around each kept out of them.
    let mut items: Vec<Range<usize>> = Vec::new();
    let mut at = from;
    for part in value[from..to].split(',') {
        let lead = part.len() - part.trim_start().len();
        let len = part.trim().len();
        items.push(at + lead..at + lead + len);
        at += part.len() + 1;
    }
    let names: Vec<String> = items.iter().map(|r| name(&value[r.clone()])).collect();
    let mut changes = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if names[i] != edit.from() || item.is_empty() {
            continue;
        }
        let keep_one = match edit {
            Edit::Rename { to, .. } => !names.contains(to),
            Edit::Delete(_) => false,
        };
        let range = if keep_one {
            let Edit::Rename { to, .. } = edit else {
                unreachable!()
            };
            changes.push((
                offset + item.start..offset + item.end,
                renamed(&value[item.clone()], to),
            ));
            continue;
        } else if i + 1 < items.len() {
            // The item and the comma and spaces up to the next one.
            item.start..items[i + 1].start
        } else if i > 0 {
            // The last: from the comma after the one before.
            items[i - 1].end..item.end
        } else {
            item.clone()
        };
        changes.push((offset + range.start..offset + range.end, String::new()));
    }
    changes
}

impl super::App {
    /// Carries out `edit` across the vault; the open note is loaded again
    /// when it changed. Refused while the open note has unsaved changes the
    /// edit would touch.
    pub(crate) fn edit_tags(&mut self, edit: Edit) -> Result<usize, String> {
        let Some(vault) = &mut self.vault else {
            return Err("No vault is open".into());
        };
        vault.refresh();
        let changes = plan(vault, &edit);
        let open = self.path.clone();
        let touches_open = open
            .as_ref()
            .is_some_and(|open| changes.iter().any(|(p, _)| p == open));
        if touches_open && self.unsaved() {
            return Err(format!("Save the open note first: it has #{}", edit.from()));
        }
        let undo = write(changes)?;
        let count = undo.count();
        use super::sidebar::Shown;
        if self.shown == Shown::Tag(edit.from().to_owned()) {
            self.shown = match &edit {
                Edit::Rename { to, .. } => Shown::Tag(to.clone()),
                Edit::Delete(_) => Shown::All,
            };
        }
        self.refresh_vault();
        if touches_open {
            self.reload();
        }
        self.undo = Some(undo);
        Ok(count)
    }

    /// Takes the last tag edit back; says how many notes were changed
    /// since and kept as they are.
    pub(crate) fn undo_tags(&mut self) -> Result<usize, String> {
        let Some(undo) = self.undo.take() else {
            return Ok(0);
        };
        let open = self.path.clone();
        let touched_open = open
            .as_ref()
            .is_some_and(|open| undo.files.iter().any(|(p, ..)| p == open));
        if touched_open && self.unsaved() {
            self.undo = Some(undo);
            return Err("Save the open note first, then undo".into());
        }
        let kept = undo.restore()?;
        self.refresh_vault();
        if touched_open {
            self.reload();
        }
        Ok(kept)
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
}
