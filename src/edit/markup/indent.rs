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
    // The parent item: the innermost other item around this one.
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
        .map(|(i, _)| i)
        .next_back();
    // Lines move by their indentation, after the `>` of the quotes the list
    // is in: the item's own indentation is replaced by where it goes, as
    // text, so tabs and spaces both work and nothing is measured in columns.
    let quotes = blocks
        .containers
        .iter()
        .filter(|c| c.list.is_none() && c.range.start < item_start && item_start <= c.range.end)
        .count();
    let indent = |start: usize| -> (usize, &str) {
        let line = start - column(start);
        let rest = &text[line..];
        let len = rest.find(['\n', '\r']).unwrap_or(rest.len());
        let at = line + quote_prefix(&rest[..len], quotes);
        let blank =
            text[at..line + len].len() - text[at..line + len].trim_start_matches([' ', '\t']).len();
        (at, &text[at..at + blank])
    };
    let own = indent(item_start).1;
    // Lists indented with tabs get tabs (REFERENCE-001 section 7).
    let tabs = own.contains('\t')
        || blocks
            .containers
            .iter()
            .filter(|c| c.list.is_some())
            .any(|c| indent(c.range.start).1.contains('\t'));
    let target = if outdent {
        // Out to the parent item's indentation, or none.
        parent.map_or(String::new(), |p| {
            indent(blocks.containers[p].range.start).1.to_owned()
        })
    } else {
        // Under the previous item's text: as its children are indented, or
        // by its marker's width (the task box is text, CommonMark 5.3).
        let Some(&previous) = position.checked_sub(1).and_then(|p| items.get(p)) else {
            return;
        };
        let prev = &blocks.containers[previous];
        let child = blocks.containers.iter().find(|c| {
            c.list.is_some() && c.range.start > prev.range.start && c.range.start <= prev.range.end
        });
        match child {
            Some(child) => indent(child.range.start).1.to_owned(),
            None => {
                let (at, blank) = indent(prev.range.start);
                let unit = if tabs {
                    "\t".to_owned()
                } else {
                    " ".repeat(marker_width(&text[at + blank.len()..]))
                };
                format!("{blank}{unit}")
            }
        }
    };
    if target == own {
        return;
    }
    let mut changes = Vec::new();
    let retarget = |from: usize, end: usize, to: &str, changes: &mut Vec<Change>| {
        let mut line = from - column(from);
        loop {
            let (at, blank) = indent(line);
            let rest = &text[at..end.max(at)];
            let blank_line = rest
                .trim_start_matches([' ', '\t'])
                .starts_with(['\n', '\r'])
                || at + blank.len() >= end;
            if !blank_line && blank.starts_with(own) {
                changes.push(Change {
                    range: at..at + own.len(),
                    text: to.to_owned(),
                });
            }
            match next_line(text, line) {
                Some(next) if next < end => line = next,
                _ => break,
            }
        }
    };
    let end = blocks.containers[items[last]].range.end;
    retarget(item_start, end, &target, &mut changes);
    // Out: the items after them in their old list stay under the last one
    // lifted (MarkText's `tabCtrl.js:133-174`), indented to its text.
    if outdent && last + 1 < items.len() {
        let lifted = &text[indent(blocks.containers[items[last]].range.start).0..];
        let lifted = lifted.trim_start_matches([' ', '\t']);
        let mut width = marker_width(lifted);
        if blocks.lists[list].ordered {
            // Its number can gain a digit when it joins the outer list.
            width += lifted_digits(text, &blocks, parent, position, last)
                .saturating_sub(lifted.bytes().take_while(u8::is_ascii_digit).count());
        }
        let under = if tabs {
            format!("{target}\t")
        } else {
            format!("{target}{}", " ".repeat(width))
        };
        let from = blocks.containers[items[last + 1]].range.start;
        let end = blocks.containers[*items.last().unwrap_or(&item)].range.end;
        retarget(from, end, &under, &mut changes);
    }
    if blocks.lists[list].ordered {
        let number = |item: usize, n: u64, changes: &mut Vec<Change>| {
            if let Some((digits, _)) = item_number(text, blocks.containers[item].range.start) {
                changes.push(Change {
                    range: digits,
                    text: n.to_string(),
                });
            }
        };
        if outdent {
            // Out: the lifted items join their parent's list after it when
            // that list is ordered too, and the items after them in their
            // old list stay under the last one, a list from 1 (an ordered
            // list starting elsewhere cannot interrupt its text).
            for (n, &after) in items[last + 1..].iter().enumerate() {
                number(after, n as u64 + 1, &mut changes);
            }
            let outer = parent.and_then(|p| Some((p, blocks.containers[p].list?)));
            if let Some((p, outer)) = outer.filter(|&(_, l)| blocks.lists[l].ordered) {
                let start =
                    item_number(text, blocks.containers[p].range.start).map_or(1, |(_, n)| n);
                let lifted = (last - position + 1) as u64;
                for (n, &moved) in items[position..=last].iter().enumerate() {
                    number(moved, start + 1 + n as u64, &mut changes);
                }
                let siblings = &blocks.lists[outer].items;
                let at = siblings.iter().position(|&i| i == p).unwrap_or(0);
                let mut previous = start;
                for &after in &siblings[at + 1..] {
                    match item_number(text, blocks.containers[after].range.start) {
                        Some((_, n)) if n == previous + 1 => {
                            number(after, n + lifted, &mut changes);
                            previous = n;
                        }
                        _ => break,
                    }
                }
            }
        } else {
            // In: ordered items moved in start their own list at 1, and the
            // items after them in the old list move up as many numbers.
            for (n, &moved) in items[position..=last].iter().enumerate() {
                number(moved, n as u64 + 1, &mut changes);
            }
            renumber_after(text, &blocks, list, position..=last, &mut changes);
        }
    }
    changes.sort_by_key(|c| c.range.start);
    let selection = Selection {
        anchor: map(&changes, selection.anchor),
        head: map(&changes, selection.head),
    };
    doc.apply(changes, selection, Kind::Other, now);
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

/// Where the line after the one at `line` starts, after any line ending.
fn next_line(text: &str, line: usize) -> Option<usize> {
    let i = line + text[line..].find(['\n', '\r'])?;
    Some(if text[i..].starts_with("\r\n") {
        i + 2
    } else {
        i + 1
    })
}

/// How wide an item's marker is with the space after it (`- `, `10. `):
/// where its text starts, at least one space, at most four (more is code).
fn marker_width(item: &str) -> usize {
    let digits = item.bytes().take_while(u8::is_ascii_digit).count();
    let marker = if digits > 0 { digits + 1 } else { 1 };
    let spaces = item[marker.min(item.len())..]
        .bytes()
        .take_while(|&b| b == b' ')
        .count();
    marker + if (1..=4).contains(&spaces) { spaces } else { 1 }
}

/// How many digits the last lifted item's number has once it joins its
/// parent's list, when that list is ordered.
fn lifted_digits(
    text: &str,
    blocks: &Blocks,
    parent: Option<usize>,
    position: usize,
    last: usize,
) -> usize {
    let start = parent
        .filter(|&p| {
            blocks.containers[p]
                .list
                .is_some_and(|l| blocks.lists[l].ordered)
        })
        .and_then(|p| item_number(text, blocks.containers[p].range.start));
    match start {
        Some((_, n)) => (n + 1 + (last - position) as u64).to_string().len(),
        None => 0,
    }
}
