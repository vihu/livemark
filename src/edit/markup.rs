//! Enter, Backspace and Tab around list and quote markup (REFERENCE-001
//! section 7), ported from CodeMirror's `insertNewlineContinueMarkup` and
//! `deleteMarkupBackward` (`lang-markdown` `dist/index.js:91-389`, MIT),
//! with the containers read from pulldown-cmark's events instead of a
//! Lezer tree.
use std::time::Duration;

use self::context::{Blocks, contexts, renumber};
use super::line_ending;
use crate::doc::{Change, Doc, Kind, Selection};

mod context;
mod indent;

pub use indent::indent;

/// `line_start` plus column `col` of `line`, kept inside the line and on a
/// character boundary: a context's columns come from the line its
/// container starts on, which can be another line (as in CodeMirror).
fn offset(line_start: usize, line: &str, col: usize) -> usize {
    let mut col = col.min(line.len());
    while !line.is_char_boundary(col) {
        col -= 1;
    }
    line_start + col
}

/// The line holding `pos`: its start and text, without the line ending.
fn line_at(doc: &Doc, pos: usize) -> (usize, &str) {
    let range = doc.line_range(doc.line_at(pos));
    (range.start, &doc.text()[range])
}

/// Enter with list or quote markup around the caret: a new line carrying
/// the same markup (the next number, an unchecked box), or one level less
/// on an empty item or a second empty quoted line. Inside fenced code the
/// new line keeps the indentation. Returns whether it applied.
pub fn continue_markup(doc: &mut Doc, now: Duration) -> bool {
    let selection = doc.selection();
    if !selection.range().is_empty() {
        return false;
    }
    let text = doc.text();
    let pos = selection.head;
    let blocks = Blocks::new(text);
    let (line_start, line) = line_at(doc, pos);
    let col = pos - line_start;
    let ending = line_ending(text);
    if blocks.fenced.iter().any(|f| f.start < pos && pos < f.end) {
        // The quote markers too: a line without them would end the quote,
        // and the code block with it (REFERENCE-001 section 9).
        let quotes = blocks
            .containers
            .iter()
            .filter(|c| c.list.is_none() && c.range.start <= pos && pos <= c.range.end)
            .count();
        let at = quote_prefix(line, quotes);
        let prefix =
            &line[..at + line[at..].len() - line[at..].trim_start_matches([' ', '\t']).len()];
        // With the caret in that prefix, the new line goes above, the
        // caret after the prefix (CodeMirror's `insertNewlineAndIndent`).
        let (at, insert, caret) = if col < prefix.len() {
            let insert = format!("{}{ending}", prefix.trim_end());
            let caret = line_start + insert.len() + prefix.len();
            (line_start, insert, caret)
        } else {
            let insert = format!("{ending}{prefix}");
            (pos, insert.clone(), pos + insert.len())
        };
        doc.apply(
            vec![Change::insert(at, insert)],
            Selection::caret(caret),
            Kind::Other,
            now,
        );
        return true;
    }
    let mut contexts = contexts(text, &blocks, pos);
    while contexts.last().is_some_and(|c| c.from > col) {
        contexts.pop();
    }
    let Some(inner) = contexts.last().cloned() else {
        return false;
    };
    if inner.to - inner.space_after.len() > col {
        return false;
    }
    let rest = line.get(inner.to.min(line.len())..).unwrap_or("");
    let empty_line = col >= inner.to - inner.space_after.len() && rest.trim().is_empty();
    let mut changes = Vec::new();
    // An empty item: one level of markup less, always (REFERENCE-001
    // section 7, a decision against CodeMirror's loosening a short list).
    if let (Some((item, list)), true) = (inner.item, empty_line) {
        let item_start = blocks.containers[item].range.start;
        let markup = &line[..offset(0, line, inner.to)];
        if item_start < line_start && !markup.bytes().all(|b| b.is_ascii_whitespace() || b == b'>')
        {
            return false;
        }
        let next = contexts.len().checked_sub(2).map(|i| contexts[i].clone());
        // A parent item starting on this line keeps its marker: only this
        // item's goes.
        let parent_here = next
            .as_ref()
            .is_some_and(|n| n.item.is_some() && inner_starts_here(&blocks, n, line_start));
        let (del_to, insert) = match &next {
            Some(_) if parent_here => (offset(line_start, line, inner.from), String::new()),
            Some(next) if next.item.is_some() => (
                offset(line_start, line, next.from),
                next.marker(text, &blocks, 1),
            ),
            Some(next) => (offset(line_start, line, next.to), String::new()),
            None => (line_start, String::new()),
        };
        let del_to = del_to.min(pos);
        let caret = del_to + insert.len();
        changes.push(Change {
            range: del_to..pos,
            text: insert,
        });
        if blocks.lists[list].ordered {
            renumber(text, &blocks, item, list, -2, &mut changes);
        }
        if let Some((parent, parent_list)) = next.and_then(|n| n.item)
            && blocks.lists[parent_list].ordered
            && !parent_here
        {
            renumber(text, &blocks, parent, parent_list, 0, &mut changes);
        }
        changes.sort_by_key(|c| c.range.start);
        doc.apply(changes, Selection::caret(caret), Kind::Other, now);
        return true;
    }
    // A second empty quoted line in a row ends the quote.
    if inner.quote && empty_line && line_start > 0 {
        let (prev_start, prev) = line_at(doc, line_start - 1);
        let quoted = prev
            .trim_end()
            .ends_with('>')
            .then(|| prev.trim_end().len() - 1);
        if quoted == Some(inner.from) {
            let first = prev_start + inner.from..prev_start + prev.len();
            let second = offset(line_start, line, inner.from)..line_start + line.len();
            let caret = second.start - first.len();
            doc.apply(
                vec![Change::delete(first), Change::delete(second)],
                Selection::caret(caret),
                Kind::Other,
                now,
            );
            return true;
        }
    }
    if let Some((item, list)) = inner.item
        && blocks.lists[list].ordered
    {
        renumber(text, &blocks, item, list, 0, &mut changes);
    }
    let continued = inner
        .item
        .is_some_and(|(item, _)| blocks.containers[item].range.start < line_start);
    let mut insert = String::new();
    let markup = line
        .bytes()
        .take_while(|b| b" \t0123456789.)-+*>".contains(b))
        .count();
    if !continued {
        // The item starts on this line: its new sibling repeats the line
        // up to it, then a fresh marker.
        insert += &lead(&blocks, &contexts, line, line_start);
        insert += &inner.marker(text, &blocks, 1);
    } else if markup >= inner.to {
        let last = contexts.len() - 1;
        for (i, context) in contexts.iter().enumerate() {
            if i == last && !continued {
                insert += &context.marker(text, &blocks, 1);
            } else {
                let width = (i < last).then(|| contexts[i + 1].from.saturating_sub(insert.len()));
                // The line's own whitespace up to the next markup (tabs
                // kept), else spaces.
                let own = (i < last)
                    .then(|| line.get(insert.len()..contexts[i + 1].from))
                    .flatten()
                    .filter(|gap| !gap.is_empty() && gap.trim().is_empty());
                match own {
                    Some(gap) => insert += gap,
                    None => insert += &context.blank(width, true),
                }
            }
        }
    }
    // Trailing whitespace before the caret goes (CodeMirror does the same).
    let from = line_start + line[..col].trim_end().len();
    let insert = format!("{ending}{insert}");
    let caret = from + insert.len();
    changes.push(Change {
        range: from..pos,
        text: insert,
    });
    changes.sort_by_key(|c| c.range.start);
    doc.apply(changes, Selection::caret(caret), Kind::Other, now);
    true
}

/// The line at `line_start` up to its innermost markup, for a new line
/// under it: quote markers and indentation kept (tabs too), the markers of
/// the items around it that start on this line turned to spaces.
fn lead(blocks: &Blocks, contexts: &[context::Context], line: &str, line_start: usize) -> String {
    let Some((inner, outer)) = contexts.split_last() else {
        return String::new();
    };
    // A lazy line, without its quotes' markers: the markup from the
    // contexts, as CodeMirror builds it.
    if contexts
        .iter()
        .any(|c| c.quote && !line.get(c.from..c.to).is_some_and(|s| s.contains('>')))
    {
        return outer.iter().map(|c| c.blank(None, true)).collect();
    }
    let mut lead = line[..offset(0, line, inner.from)].to_owned();
    for context in outer {
        if context.item.is_some() && inner_starts_here(blocks, context, line_start) {
            let end = context.to.min(lead.len());
            let start = (context.from + context.space_before.len()).min(end);
            if lead.is_char_boundary(start) && lead.is_char_boundary(end) {
                lead.replace_range(start..end, &" ".repeat(end - start));
            }
        }
    }
    lead
}

/// The length of up to `depth` quote markers (`>` with the indentation
/// before it, which can be a list item's, and one space after) at the start
/// of `line`.
fn quote_prefix(line: &str, depth: usize) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    for _ in 0..depth {
        let mut i = at;
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'>') {
            break;
        }
        i += 1;
        if bytes.get(i) == Some(&b' ') {
            i += 1;
        }
        at = i;
    }
    at
}

/// Where the line after the one at `line` starts, after any line ending.
fn next_line(text: &str, line: usize) -> Option<usize> {
    let i = line + text[line..].find(['\n', '\r'])?;
    Some(if text[i..].starts_with("\r\n") {
        i + 2
    } else {
        i + 1
    })
}

/// Whether `context`'s container starts on the line at `line_start`.
fn inner_starts_here(blocks: &Blocks, context: &context::Context, line_start: usize) -> bool {
    match context.item {
        Some((item, _)) => blocks.containers[item].range.start >= line_start,
        // A quote's marker is on every line it covers.
        None => true,
    }
}

/// Shift+Enter: a new line indented to the item's or quote's content,
/// without a new marker (REFERENCE-001 section 7).
pub fn soft_break(doc: &mut Doc, now: Duration) {
    let text = doc.text();
    let pos = doc.selection().range().start;
    let blocks = Blocks::new(text);
    // In fenced code, as Enter: the line's indentation (its list item's
    // too) kept; over a selection, a plain line ending.
    if blocks.fenced.iter().any(|f| f.start < pos && pos < f.end) {
        if !continue_markup(doc, now) {
            let range = doc.selection().range();
            let ending = line_ending(doc.text()).to_owned();
            let caret = range.start + ending.len();
            doc.apply(
                vec![Change {
                    range,
                    text: ending,
                }],
                Selection::caret(caret),
                Kind::Other,
                now,
            );
        }
        return;
    }
    let line_start = line_at(doc, pos).0;
    let mut contexts = contexts(text, &blocks, pos);
    while contexts.last().is_some_and(|c| c.from > pos - line_start) {
        contexts.pop();
    }
    let mut insert = line_ending(text).to_owned();
    match contexts.last() {
        // On the item's or quote's own line: the line up to it, then blank
        // as wide as its markup.
        Some(inner) if inner_starts_here(&blocks, inner, line_start) => {
            let line = line_at(doc, pos).1;
            insert += &lead(&blocks, &contexts, line, line_start);
            insert += &inner.blank(None, true);
        }
        // On a continuation line, already indented: as it is.
        Some(_) => {
            let line = line_at(doc, pos).1;
            let lead = line.len() - line.trim_start_matches([' ', '\t', '>']).len();
            insert += &line[..lead];
        }
        None => {}
    }
    let range = doc.selection().range();
    let caret = range.start + insert.len();
    doc.apply(
        vec![Change {
            range,
            text: insert,
        }],
        Selection::caret(caret),
        Kind::Other,
        now,
    );
}

/// Backspace right after list or quote markup (CodeMirror's
/// `deleteMarkupBackward`): extra spaces after it go first; then an item
/// that is not its list's first becomes a continuation of the one before
/// (its marker turned to spaces); otherwise one level of markup goes.
/// Returns whether it applied.
pub fn delete_markup(doc: &mut Doc, now: Duration) -> bool {
    let selection = doc.selection();
    if !selection.range().is_empty() {
        return false;
    }
    let text = doc.text();
    let pos = selection.head;
    let blocks = Blocks::new(text);
    let Some(inner) = contexts(text, &blocks, pos).pop() else {
        return false;
    };
    let (line_start, line) = line_at(doc, pos);
    let col = pos - line_start;
    let space_end = inner.to - inner.space_after.len() + usize::from(!inner.space_after.is_empty());
    if col > space_end
        && line
            .get(space_end..col)
            .is_some_and(|s| s.trim().is_empty())
    {
        let at = line_start + space_end;
        doc.apply(
            vec![Change::delete(at..pos)],
            Selection::caret(at),
            Kind::Delete,
            now,
        );
        return true;
    }
    // On the item's first line, or after a quote's marker on this line.
    let on_item_line = match inner.item {
        Some((item, _)) => line_start <= blocks.containers[item].range.start,
        None => line
            .get(inner.from..inner.to)
            .is_some_and(|s| s.contains('>')),
    };
    let only_markup = line
        .get(..inner.to)
        .is_some_and(|s| s.bytes().all(|b| b.is_ascii_whitespace() || b == b'>'));
    if col != space_end || !(on_item_line || only_markup) {
        return false;
    }
    let start = offset(line_start, line, inner.from);
    if let Some((item, list)) = inner.item
        && blocks.lists[list].start < blocks.containers[item].range.start
        && line
            .get(inner.from..inner.to)
            .is_some_and(|s| !s.trim().is_empty())
    {
        let mut insert = inner.blank(Some(inner.to - inner.from), true);
        // After a `>` with no space, the first space is the quote's
        // (CommonMark 5.1): one more keeps the text in the item.
        if inner.space_before.is_empty() && line.get(..inner.from).is_some_and(|s| s.ends_with('>'))
        {
            insert.push(' ');
        }
        let caret = start + insert.len();
        doc.apply(
            vec![Change {
                range: start..offset(line_start, line, inner.to),
                text: insert,
            }],
            Selection::caret(caret),
            Kind::Delete,
            now,
        );
        return true;
    }
    // One level of markup: the marker, its indentation staying (so a tab
    // keeps the text in its parent item).
    let start = offset(line_start, line, inner.from + inner.space_before.len());
    if start < pos {
        doc.apply(
            vec![Change::delete(start..pos)],
            Selection::caret(start),
            Kind::Delete,
            now,
        );
        return true;
    }
    false
}

/// Where offset `at` of the old text lands after `changes` (sorted): after
/// text inserted right at it, at the start of text removed around it.
fn map(changes: &[Change], at: usize) -> usize {
    let mut delta = 0isize;
    for change in changes.iter().take_while(|c| c.range.start <= at) {
        if change.range.end <= at {
            delta += change.text.len() as isize - change.range.len() as isize;
        } else {
            return change.range.start.wrapping_add_signed(delta);
        }
    }
    at.wrapping_add_signed(delta)
}

#[cfg(test)]
mod tests;
