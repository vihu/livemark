//! Tab and Shift+Tab on list items (REFERENCE-001 section 7): the items a
//! selection reaches move together, with their children.
use std::time::Duration;

use super::context::{Blocks, Container, contexts, item_number};
use super::{line_at, map, quote_prefix};
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
    let ordered = blocks.lists[list].ordered;
    // Indentation `blank` at `at` in columns from where its quotes' text
    // starts: a `>` with no space after it takes the first column of what
    // follows as its own (CommonMark 5.1); a tab goes to the next stop.
    let rel = |at: usize, blank: &str| {
        let start = columns(0, &text[at - column(at)..at]);
        let bare = !blank.is_empty() && text[..at].ends_with('>');
        columns(start, blank) - start - usize::from(bare)
    };
    let spaces =
        |at: usize, rel: usize| " ".repeat(rel + usize::from(rel > 0 && text[..at].ends_with('>')));
    // Under an item's text, from indentation `blank` at `at`: a tab where
    // tabs are used and it reaches that far, else spaces as wide as the
    // item's marker. The text and its columns.
    let under = |at: usize, blank: &str, marker: usize| {
        let want = rel(at, blank) + marker;
        let tab = format!("{blank}\t");
        if tabs && rel(at, &tab) >= want {
            let columns = rel(at, &tab);
            (tab, columns)
        } else if blank.is_empty() {
            (spaces(at, want), want)
        } else {
            (format!("{blank}{}", " ".repeat(marker)), want)
        }
    };
    let delimiter = |start: usize| {
        item_number(text, start).map(|(digits, _)| text.as_bytes().get(digits.end).copied())
    };
    // Item `p`'s first nested item, and the number an ordered item from
    // `start` takes after it: on from the ordered list that ends `p` when
    // it joins that list (MarkText's `tabCtrl.js:182-196`), else 1.
    let nested = |p: usize, start: usize| {
        let p = &blocks.containers[p];
        let inside = |c: &&Container| {
            c.list.is_some() && c.range.start > p.range.start && c.range.start <= p.range.end
        };
        let number = blocks
            .containers
            .iter()
            .filter(|c| inside(c) && c.range.end == p.range.end)
            .min_by_key(|c| c.range.start)
            .filter(|c| {
                ordered
                    && c.list.is_some_and(|l| blocks.lists[l].ordered)
                    && delimiter(c.range.start) == delimiter(start)
            })
            .and_then(|c| item_number(text, c.range.start))
            .map_or(1, |(_, n)| n + 1);
        (blocks.containers.iter().find(inside), number)
    };
    // In: ordered items number from `first`.
    let mut first = 1;
    let (target, target_rel) = if outdent {
        // Out to the parent item's indentation; an item with none stays
        // (MarkText's `tabCtrl.js:103-113`).
        let Some(parent) = parent else {
            return;
        };
        let (at, blank) = indent(blocks.containers[parent].range.start);
        (blank.to_owned(), rel(at, blank))
    } else {
        // Under the previous item's text: as its children are indented, or
        // by its marker's width (the task box is text, CommonMark 5.3).
        let Some(&previous) = position.checked_sub(1).and_then(|p| items.get(p)) else {
            return;
        };
        let (child, number) = nested(previous, item_start);
        first = number;
        match child {
            Some(child) => {
                let (at, blank) = indent(child.range.start);
                (blank.to_owned(), rel(at, blank))
            }
            None => {
                let (at, blank) = indent(blocks.containers[previous].range.start);
                under(at, blank, marker_width(&text[at + blank.len()..]))
            }
        }
    };
    if target == own {
        return;
    }
    // Out into an ordered list with the same delimiter, the lifted items
    // are numbered on from their parent; with another, they are a list of
    // their own.
    let outer = parent
        .and_then(|p| Some((p, blocks.containers[p].list?)))
        .filter(|&(p, l)| {
            blocks.lists[l].ordered
                && delimiter(blocks.containers[p].range.start) == delimiter(item_start)
        })
        .and_then(|(p, l)| Some((p, l, item_number(text, blocks.containers[p].range.start)?.1)));
    // The number moved ordered item `n` (from 0) takes, if it changes.
    let renumbered = |n: usize| match (ordered, outdent) {
        (false, _) => None,
        (true, true) => outer.map(|(_, _, start)| start + 1 + n as u64),
        (true, false) => Some(first + n as u64),
    };
    // How many digits `item`'s number gains (or loses) as `n`.
    let grows = |item: usize, n: Option<u64>| match (
        n,
        item_number(text, blocks.containers[item].range.start),
    ) {
        (Some(n), Some((digits, _))) => n.to_string().len() as isize - digits.len() as isize,
        _ => 0,
    };
    // A line's indentation `blank` at `at` that was `own` plus more, put
    // at `goal` columns: `own` replaced by `to` where that lands there
    // (tabs kept), else spaces. The change and the new indentation.
    let place = |at: usize, blank: &str, own: &str, to: &str, goal: usize| {
        if let Some(rest) = blank.strip_prefix(own) {
            let kept = format!("{to}{rest}");
            if rel(at, &kept) == goal {
                return (at..at + own.len(), to.to_owned(), kept);
            }
        }
        let new = spaces(at, goal);
        (at..at + blank.len(), new.clone(), new)
    };
    let mut changes = Vec::new();
    // The lines of the item from `from` to `end` move as its first line
    // goes to `to`, `to_rel` columns in; lines after the first move `grow`
    // columns more, with the item's text when its number changes width.
    let retarget = |from: usize,
                    end: usize,
                    (to, to_rel): (&str, usize),
                    grow: isize,
                    changes: &mut Vec<Change>| {
        let first = from - column(from);
        let (at, own) = indent(first);
        let shift = to_rel as isize - rel(at, own) as isize;
        let mut line = first;
        loop {
            let (at, blank) = indent(line);
            let rest = &text[at..end.max(at)];
            let blank_line = rest
                .trim_start_matches([' ', '\t'])
                .starts_with(['\n', '\r'])
                || at + blank.len() >= end;
            if !blank_line {
                let more = if line == first { 0 } else { grow };
                let goal = (rel(at, blank) as isize + shift + more).max(0) as usize;
                let (range, new, _) = place(at, blank, own, to, goal);
                if text[range.clone()] != new {
                    changes.push(Change { range, text: new });
                }
            }
            match next_line(text, line) {
                Some(next) if next < end => line = next,
                _ => break,
            }
        }
    };
    for (n, &moved) in items[position..=last].iter().enumerate() {
        let range = &blocks.containers[moved].range;
        let grow = grows(moved, renumbered(n));
        retarget(
            range.start,
            range.end,
            (&target, target_rel),
            grow,
            &mut changes,
        );
    }
    // Out: the items after them in their old list stay under the last one
    // lifted (MarkText's `tabCtrl.js:133-174`): as its children are
    // indented, or to its text, numbered on from its ordered child list or
    // from 1 (an ordered list starting elsewhere cannot interrupt text).
    let mut after_first = 1;
    if outdent && last + 1 < items.len() {
        let lifted = items[last];
        let grow = grows(lifted, renumbered(last - position));
        let (child, number) = nested(lifted, blocks.containers[items[last + 1]].range.start);
        after_first = number;
        let to = match child {
            Some(child) => {
                let (at, blank) = indent(child.range.start);
                let (lifted_at, lifted_own) = indent(blocks.containers[lifted].range.start);
                let shift = target_rel as isize - rel(lifted_at, lifted_own) as isize;
                let goal = (rel(at, blank) as isize + shift + grow).max(0) as usize;
                let (_, _, new) = place(at, blank, lifted_own, &target, goal);
                (new, goal)
            }
            None => {
                let at = indent(blocks.containers[lifted].range.start).0;
                let marker = marker_width(text[at..].trim_start_matches([' ', '\t']));
                under(at, &target, marker.saturating_add_signed(grow))
            }
        };
        for (n, &after) in items[last + 1..].iter().enumerate() {
            let range = &blocks.containers[after].range;
            let grow = grows(after, ordered.then_some(after_first + n as u64));
            retarget(range.start, range.end, (&to.0, to.1), grow, &mut changes);
        }
    }
    if ordered {
        let number = |item: usize, n: u64, changes: &mut Vec<Change>| {
            if let Some((digits, _)) = item_number(text, blocks.containers[item].range.start) {
                changes.push(Change {
                    range: digits,
                    text: n.to_string(),
                });
            }
        };
        // An item that stays where it is but whose number changes width:
        // its other lines move with its text.
        let follow = |item: usize, n: u64, changes: &mut Vec<Change>| {
            let grow = grows(item, Some(n));
            if grow != 0 {
                let range = &blocks.containers[item].range;
                let (at, blank) = indent(range.start);
                retarget(
                    range.start,
                    range.end,
                    (blank, rel(at, blank)),
                    grow,
                    changes,
                );
            }
        };
        for (n, &moved) in items[position..=last].iter().enumerate() {
            if let Some(new) = renumbered(n) {
                number(moved, new, &mut changes);
            }
        }
        if outdent {
            for (n, &after) in items[last + 1..].iter().enumerate() {
                number(after, after_first + n as u64, &mut changes);
            }
            // The outer list's items after the parent move up as many.
            if let Some((p, outer, start)) = outer {
                let lifted = (last - position + 1) as u64;
                let siblings = &blocks.lists[outer].items;
                let at = siblings.iter().position(|&i| i == p).unwrap_or(0);
                let mut previous = start;
                for &after in &siblings[at + 1..] {
                    match item_number(text, blocks.containers[after].range.start) {
                        Some((_, n)) if n == previous + 1 => {
                            number(after, n + lifted, &mut changes);
                            follow(after, n + lifted, &mut changes);
                            previous = n;
                        }
                        _ => break,
                    }
                }
            }
        } else {
            // In: the items after them in the old list move up as many
            // numbers.
            for (after, n) in renumber_after(text, &blocks, list, position..=last) {
                number(after, n, &mut changes);
                follow(after, n, &mut changes);
            }
        }
    }
    changes.sort_by_key(|c| c.range.start);
    let selection = Selection {
        anchor: map(&changes, selection.anchor),
        head: map(&changes, selection.head),
    };
    doc.apply(changes, selection, Kind::Other, now);
}

/// The column `s` ends at, from column `from` (a tab goes to the next
/// multiple of 4, CommonMark 2.2).
fn columns(from: usize, s: &str) -> usize {
    s.chars().fold(
        from,
        |at, c| if c == '\t' { at + 4 - at % 4 } else { at + 1 },
    )
}

/// After ordered items `moved` went into their own list, the consecutive
/// items that followed them take over their numbers: each with its new one.
fn renumber_after(
    text: &str,
    blocks: &Blocks,
    list: usize,
    moved: std::ops::RangeInclusive<usize>,
) -> Vec<(usize, u64)> {
    let items = &blocks.lists[list].items;
    let number = |i: usize| item_number(text, blocks.containers[items[i]].range.start);
    let (Some((_, mut next)), Some((_, mut previous))) =
        (number(*moved.start()), number(*moved.end()))
    else {
        return Vec::new();
    };
    let mut renumbered = Vec::new();
    for &item in &items[moved.end() + 1..] {
        match item_number(text, blocks.containers[item].range.start) {
            Some((_, number)) if number == previous + 1 => {
                renumbered.push((item, next));
                previous = number;
                next += 1;
            }
            _ => break,
        }
    }
    renumbered
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
