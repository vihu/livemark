//! Tags from the properties line (PLAN-005): a chip's cross takes its tag
//! out of the front matter's list, or takes the `#` off a tag written in
//! the text (the word stays); "+ tag" and Ctrl/Cmd+T put the caret where a
//! new tag goes in the list, adding the list, and front matter, when
//! missing. Each is one undo step; nothing else in the text changes.
use std::time::Duration;

use super::line_ending;
use crate::doc::{Change, Doc, Kind, Selection};
use crate::parse::properties::Form;
use crate::style::Styled;

/// A tag on the properties line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chip {
    /// Item `.0` of the front matter's `tags` list.
    Listed(usize),
    /// A tag written in the text, by name (any case).
    Inline(String),
}

/// Takes `chip` out: its list item with a comma beside it (or its line),
/// or the `#` of every inline tag with its name. The selection keeps its
/// place in the text around.
pub fn remove_tag(doc: &mut Doc, styled: &Styled, chip: &Chip, now: Duration) {
    let cuts: Vec<std::ops::Range<usize>> = match chip {
        Chip::Listed(index) => styled
            .properties()
            .and_then(|p| p.tags.as_ref())
            .and_then(|list| list.items.get(*index))
            .map(|item| vec![item.cut.clone()])
            .unwrap_or_default(),
        Chip::Inline(name) => styled
            .tags()
            .iter()
            .filter(|(_, tag)| tag.to_lowercase() == name.to_lowercase())
            .map(|(range, _)| range.start..range.start + 1)
            .collect(),
    };
    if cuts.is_empty() {
        return;
    }
    // Each end moved back by what goes before it.
    let map = |at: usize| {
        let gone: usize = cuts
            .iter()
            .map(|cut| at.min(cut.end).saturating_sub(cut.start))
            .sum();
        at - gone
    };
    let selection = doc.selection();
    let selection = Selection {
        anchor: map(selection.anchor),
        head: map(selection.head),
    };
    let changes = cuts
        .into_iter()
        .map(|range| Change {
            range,
            text: String::new(),
        })
        .collect();
    doc.apply(changes, selection, Kind::Other, now);
}

/// Puts the caret where a new tag goes: after the list's last item, with
/// a comma or a new `- ` line, or inside a new `tags: []` (in new front
/// matter when the note has none).
pub fn add_tag(doc: &mut Doc, styled: &Styled, now: Duration) {
    let text = doc.text();
    let nl = line_ending(text);
    let (at, insert, caret): (std::ops::Range<usize>, String, usize) = match styled.properties() {
        None => {
            let at = text.len() - text.trim_start_matches('\u{feff}').len();
            let insert = format!("---{nl}tags: []{nl}---{nl}");
            let caret = format!("---{nl}tags: [").len();
            (at..at, insert, caret)
        }
        Some(properties) => match &properties.tags {
            None => {
                let at = properties.closing;
                (at..at, format!("tags: []{nl}"), "tags: [".len())
            }
            Some(list) => {
                let last = list.items.last().map(|item| item.range.end);
                match (&list.form, last) {
                    (Form::Flow { close, .. }, None) => (*close..*close, String::new(), 0),
                    (Form::Flow { .. } | Form::Plain { .. }, Some(end)) => {
                        (end..end, ", ".into(), 2)
                    }
                    (Form::Block { indent, end }, _) => {
                        let insert = format!("{nl}{indent}- ");
                        let caret = insert.len();
                        (*end..*end, insert, caret)
                    }
                    (Form::Empty { rest }, _) => (rest.clone(), " []".into(), 2),
                    (Form::Plain { values }, None) => (values.end..values.end, String::new(), 0),
                }
            }
        },
    };
    let caret = Selection::caret(at.start + caret);
    doc.apply(
        vec![Change {
            range: at,
            text: insert,
        }],
        caret,
        Kind::Other,
        now,
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Chip, add_tag, remove_tag};
    use crate::doc::{Doc, Selection};
    use crate::style::Styled;

    /// `text` after adding a tag, with `|` at the caret.
    fn added(text: &str) -> String {
        let mut doc = Doc::new(text.to_owned());
        let styled = Styled::new(doc.text());
        add_tag(&mut doc, &styled, Duration::ZERO);
        let head = doc.selection().head;
        format!("{}|{}", &doc.text()[..head], &doc.text()[head..])
    }

    #[test]
    fn a_new_tag_goes_after_the_last_in_every_list_form() {
        assert_eq!(
            added("---\ntags: [a, b]\n---\n"),
            "---\ntags: [a, b, |]\n---\n"
        );
        assert_eq!(added("---\ntags: []\n---\n"), "---\ntags: [|]\n---\n");
        assert_eq!(added("---\ntags: a\n---\n"), "---\ntags: a, |\n---\n");
        assert_eq!(
            added("---\r\ntags:\r\n  - a\r\n---\r\n"),
            "---\r\ntags:\r\n  - a\r\n  - |\r\n---\r\n"
        );
        assert_eq!(added("---\ntags:\n---\n"), "---\ntags: [|]\n---\n");
        assert_eq!(
            added("---\ntitle: x\n---\nbody\n"),
            "---\ntitle: x\ntags: [|]\n---\nbody\n"
        );
        assert_eq!(added("body\n"), "---\ntags: [|]\n---\nbody\n");
        // Undone in one step.
        let mut doc = Doc::new("body\n".to_owned());
        add_tag(&mut doc, &Styled::new("body\n"), Duration::ZERO);
        doc.undo();
        assert_eq!(doc.text(), "body\n");
    }

    #[test]
    fn a_cross_takes_out_one_item_or_every_inline_hash() {
        let remove = |text: &str, chip: Chip, caret: usize| {
            let mut doc = Doc::new(text.to_owned());
            doc.set_selection(Selection::caret(caret));
            let styled = Styled::new(doc.text());
            remove_tag(&mut doc, &styled, &chip, Duration::ZERO);
            (doc.text().to_owned(), doc.selection().head)
        };
        let text = "---\ntags: [a, b]\n---\nSee #Idea and #idea.\n";
        assert_eq!(
            remove(text, Chip::Listed(0), text.len()),
            (
                "---\ntags: [b]\n---\nSee #Idea and #idea.\n".into(),
                text.len() - 3
            )
        );
        assert_eq!(
            remove(text, Chip::Inline("idea".into()), text.len()).0,
            "---\ntags: [a, b]\n---\nSee Idea and idea.\n"
        );
        let block = "---\ntags:\n  - a\n  - b\n---\n";
        assert_eq!(
            remove(block, Chip::Listed(1), 0).0,
            "---\ntags:\n  - a\n---\n"
        );
    }
}
