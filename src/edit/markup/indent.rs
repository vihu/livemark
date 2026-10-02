//! Tab and Shift+Tab on list items (REFERENCE-001 section 7): the items a
//! selection reaches move together, with their children.
use std::time::Duration;

use super::context::{Blocks, contexts, item_number};
use super::{line_at, map};
use crate::doc::{Change, Doc, Kind, Selection};

/// Tab and Shift+Tab (REFERENCE-001 section 7). In a list item: the item
/// and its children move under the item before it (a first item stays), or
/// back out to its parent's column; the sibling items after it that the
/// selection reaches move the same way, with theirs. Elsewhere Tab types a
/// tab and Shift+Tab takes one indent unit off the line.
pub fn indent(doc: &mut Doc, outdent: bool, now: Duration) {
    let text = doc.text();
    let selection = doc.selection();
    let pos = selection.head;
    let reach = selection.range();
    let blocks = Blocks::new(text);
    let column = |start: usize| start - text[..start].rfind(['\n', '\r']).map_or(0, |i| i + 1);
    // A selection starting in a line's indentation or quote prefix belongs
    // to the item on that line, not to the one around it.
    let line_start = reach.start - column(reach.start);
    let rest = &text[line_start..];
    let prefix = line_start + rest.len() - rest.trim_start_matches([' ', '\t', '>']).len();
    let item = contexts(text, &blocks, reach.start.max(prefix))
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
    let item_start = blocks.containers[item].range.start;
    let items = &blocks.lists[list].items;
    let position = items.iter().position(|&i| i == item).unwrap_or(0);
    // The last sibling the selection reaches; one ending at a line's start
    // leaves that line out (CodeMirror's `selectedLineBlocks`).
    let last = items
        .iter()
        .enumerate()
        .skip(position + 1)
        .take_while(|&(_, &i)| blocks.containers[i].range.start < reach.end)
        .last()
        .map_or(position, |(j, _)| j);
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
    // Every line of the items and their children moves, after the `>` of
    // the quotes the list is in.
    let quotes = blocks
        .containers
        .iter()
        .filter(|c| c.list.is_none() && c.range.start < item_start && item_start <= c.range.end)
        .count();
    let end = blocks.containers[items[last]].range.end;
    let first_line = item_start - column(item_start);
    let mut changes = Vec::new();
    let mut line = first_line;
    loop {
        let rest = &text[line..end];
        let len = rest.find(['\n', '\r']).unwrap_or(rest.len());
        let at = line + quote_prefix(&rest[..len], quotes);
        if shift > 0 && at < line + len {
            changes.push(Change::insert(at, " ".repeat(shift as usize)));
        } else if shift < 0 {
            let spaces = text[at..end]
                .bytes()
                .take(-shift as usize)
                .take_while(|&b| b == b' ')
                .count();
            changes.push(Change::delete(at..at + spaces));
        }
        match rest.find('\n') {
            Some(i) if line + i < end => line += i + 1,
            _ => break,
        }
    }
    // Ordered items moved in start their own list at 1, and the items
    // after them in the old list move up as many numbers.
    if !outdent && blocks.lists[list].ordered {
        for (n, &moved) in items[position..=last].iter().enumerate() {
            let start = blocks.containers[moved].range.start;
            if let Some((digits, _)) = item_number(text, start) {
                changes.push(Change {
                    range: digits,
                    text: (n + 1).to_string(),
                });
            }
        }
        renumber_after(text, &blocks, list, position..=last, &mut changes);
    }
    changes.sort_by_key(|c| c.range.start);
    let selection = Selection {
        anchor: map(&changes, selection.anchor),
        head: map(&changes, selection.head),
    };
    doc.apply(changes, selection, Kind::Other, now);
}

/// The length of up to `depth` quote markers (`>` with up to three spaces
/// before and one after) at the start of `line`.
fn quote_prefix(line: &str, depth: usize) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    for _ in 0..depth {
        let mut i = at;
        while i < bytes.len() && i - at < 3 && bytes[i] == b' ' {
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

/// After ordered items `moved` went into their own list, the consecutive
/// items that followed them take over their numbers.
fn renumber_after(
    text: &str,
    blocks: &Blocks,
    list: usize,
    moved: std::ops::RangeInclusive<usize>,
    changes: &mut Vec<Change>,
) {
    let items = &blocks.lists[list].items;
    let number = |i: usize| item_number(text, blocks.containers[items[i]].range.start);
    let (Some((_, mut next)), Some((_, mut previous))) =
        (number(*moved.start()), number(*moved.end()))
    else {
        return;
    };
    for &item in &items[moved.end() + 1..] {
        let start = blocks.containers[item].range.start;
        let Some((digits, number)) = item_number(text, start) else {
            return;
        };
        if number != previous + 1 {
            return;
        }
        changes.push(Change {
            range: digits,
            text: next.to_string(),
        });
        previous = number;
        next += 1;
    }
}
