//! Enter, Backspace and Tab around list and quote markup (REFERENCE-001
//! section 7), ported from CodeMirror's `insertNewlineContinueMarkup` and
//! `deleteMarkupBackward` (`lang-markdown` `dist/index.js:91-389`, MIT),
//! with the containers read from pulldown-cmark's events instead of a
//! Lezer tree.
use std::time::Duration;

use self::context::{Blocks, contexts, item_number, renumber};
use super::line_ending;
use crate::doc::{Change, Doc, Kind, Selection};

mod context;

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
        let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
        let insert = format!("{ending}{indent}");
        let caret = pos + insert.len();
        doc.apply(
            vec![Change::insert(pos, insert)],
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
        let (del_to, insert) = match &next {
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
    if !continued || markup >= inner.to {
        let last = contexts.len() - 1;
        for (i, context) in contexts.iter().enumerate() {
            if i == last && !continued {
                insert += &context.marker(text, &blocks, 1);
            } else {
                let width = (i < last).then(|| contexts[i + 1].from.saturating_sub(insert.len()));
                insert += &context.blank(width, true);
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

/// Shift+Enter: a new line indented to the item's or quote's content,
/// without a new marker (REFERENCE-001 section 7).
pub fn soft_break(doc: &mut Doc, now: Duration) {
    let text = doc.text();
    let pos = doc.selection().range().start;
    let blocks = Blocks::new(text);
    let line_start = line_at(doc, pos).0;
    let mut contexts = contexts(text, &blocks, pos);
    while contexts.last().is_some_and(|c| c.from > pos - line_start) {
        contexts.pop();
    }
    let mut insert = line_ending(text).to_owned();
    for context in &contexts {
        insert += &context.blank(None, true);
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
    let on_item_line = inner
        .item
        .is_some_and(|(item, _)| line_start <= blocks.containers[item].range.start);
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
        let insert = inner.blank(Some(inner.to - inner.from), true);
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

/// Tab and Shift+Tab (REFERENCE-001 section 7). In a list item: the item
/// and its children move under the item before it (a first item stays), or
/// back out to its parent's column. Elsewhere Tab types a tab and
/// Shift+Tab takes one indent unit off the line.
pub fn indent(doc: &mut Doc, outdent: bool, now: Duration) {
    let text = doc.text();
    let selection = doc.selection();
    let pos = selection.head;
    let blocks = Blocks::new(text);
    let item = contexts(text, &blocks, pos)
        .into_iter()
        .rev()
        .find_map(|c| c.item);
    let Some((item, list)) = item else {
        if outdent {
            let (line_start, line) = line_at(doc, pos);
            let unit = if line.starts_with('\t') {
                1
            } else {
                line.bytes().take(4).take_while(|&b| b == b' ').count()
            };
            if unit > 0 {
                let moved = |at: usize| {
                    if at >= line_start + unit {
                        at - unit
                    } else {
                        at.min(line_start)
                    }
                };
                let selection = Selection {
                    anchor: moved(selection.anchor),
                    head: moved(selection.head),
                };
                doc.apply(
                    vec![Change::delete(line_start..line_start + unit)],
                    selection,
                    Kind::Other,
                    now,
                );
            }
        } else {
            let range = selection.range();
            let caret = range.start + 1;
            doc.apply(
                vec![Change {
                    range,
                    text: "\t".into(),
                }],
                Selection::caret(caret),
                Kind::Other,
                now,
            );
        }
        return;
    };
    let column = |start: usize| start - text[..start].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let item_start = blocks.containers[item].range.start;
    let items = &blocks.lists[list].items;
    let position = items.iter().position(|&i| i == item).unwrap_or(0);
    let shift: i64 = if outdent {
        // Out to the parent item's marker, or the line start.
        let parent = blocks
            .containers
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                *i != item
                    && c.list.is_some()
                    && c.range.start < item_start
                    && item_start <= c.range.end
            })
            .map(|(_, c)| column(c.range.start))
            .next_back();
        -((column(item_start) - parent.unwrap_or(0)) as i64)
    } else {
        // Under the previous item's text.
        let Some(&previous) = position.checked_sub(1).and_then(|p| items.get(p)) else {
            return;
        };
        let prev_start = blocks.containers[previous].range.start;
        let Some(prev) = contexts(text, &blocks, prev_start).pop() else {
            return;
        };
        prev.to as i64 - column(item_start) as i64
    };
    if shift == 0 {
        return;
    }
    // Every line of the item and its children moves.
    let range = &blocks.containers[item].range;
    let first_line = item_start - column(item_start);
    let mut changes = Vec::new();
    let mut line = first_line;
    loop {
        let rest = &text[line..range.end];
        let len = rest.find(['\n', '\r']).unwrap_or(rest.len());
        if shift > 0 && len > 0 {
            changes.push(Change::insert(line, " ".repeat(shift as usize)));
        } else if shift < 0 {
            let spaces = rest
                .bytes()
                .take(-shift as usize)
                .take_while(|&b| b == b' ')
                .count();
            changes.push(Change::delete(line..line + spaces));
        }
        match rest.find('\n') {
            Some(i) if line + i < range.end => line += i + 1,
            _ => break,
        }
    }
    // An ordered item moved in starts its own list at 1, and the items
    // after it in the old list move up a number.
    if !outdent && blocks.lists[list].ordered {
        if let Some((spaces, number)) = item_number(text, item_start) {
            let at = item_start + spaces;
            changes.push(Change {
                range: at..at + number.to_string().len(),
                text: "1".into(),
            });
        }
        renumber_after(text, &blocks, list, position, &mut changes);
    }
    changes.sort_by_key(|c| c.range.start);
    let selection = Selection {
        anchor: map(&changes, selection.anchor),
        head: map(&changes, selection.head),
    };
    doc.apply(changes, selection, Kind::Other, now);
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

/// After an ordered item moved into its own list, the consecutive items
/// that followed it move up a number.
fn renumber_after(
    text: &str,
    blocks: &Blocks,
    list: usize,
    position: usize,
    changes: &mut Vec<Change>,
) {
    let items = &blocks.lists[list].items;
    let Some((_, mut expected)) = items
        .get(position)
        .and_then(|&i| item_number(text, blocks.containers[i].range.start))
    else {
        return;
    };
    for &next in &items[position + 1..] {
        let start = blocks.containers[next].range.start;
        let Some((spaces, number)) = item_number(text, start) else {
            return;
        };
        if number != expected + 1 {
            return;
        }
        let at = start + spaces;
        changes.push(Change {
            range: at..at + number.to_string().len(),
            text: expected.to_string(),
        });
        expected = number;
    }
}

#[cfg(test)]
mod tests;
