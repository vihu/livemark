//! Tab and Shift+Tab on list items (REFERENCE-001 section 7): the items a
//! selection reaches move together, with their children.
use std::time::Duration;

use self::lists::Lists;
use super::context::{Blocks, contexts, item_number};
use super::{line_at, map};
use crate::doc::{Change, Doc, Kind, Selection};

mod lists;

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
    let mut lists = Lists {
        text,
        blocks: &blocks,
        quotes: 0,
        tabs: false,
        ordered: false,
    };
    // A selection starting in a line's indentation or quote prefix belongs
    // to the item on that line, not to the one around it.
    let line_start = reach.start - lists.column(reach.start);
    let rest = &text[line_start..];
    let prefix = line_start + rest.len() - rest.trim_start_matches([' ', '\t', '>']).len();
    let item = contexts(text, &blocks, reach.start.max(prefix))
        .into_iter()
        .rev()
        .find_map(|c| c.item);
    let Some((item, list)) = item else {
        // In fenced code, never the indentation in front of the fence: it
        // is the list item's or the quote's the code is in.
        let floor = blocks
            .fenced
            .iter()
            .find(|f| f.start < pos && pos < f.end)
            .map_or(0, |f| lists.column(f.start));
        plain_indent(doc, outdent, floor, now);
        return;
    };
    let item_start = blocks.containers[item].range.start;
    // An item starting on its parent's line stays: moving it would split
    // the line.
    if lists.markers(item_start) {
        return;
    }
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
    // is in.
    lists.quotes = blocks
        .containers
        .iter()
        .filter(|c| c.list.is_none() && c.range.start < item_start && item_start <= c.range.end)
        .count();
    let own = lists.indent(item_start).1;
    // Lists indented with tabs get tabs (REFERENCE-001 section 7).
    lists.tabs = own.contains('\t')
        || blocks
            .containers
            .iter()
            .filter(|c| c.list.is_some())
            .any(|c| lists.indent(c.range.start).1.contains('\t'));
    lists.ordered = blocks.lists[list].ordered;
    let ordered = lists.ordered;
    // In: ordered items number from `first`.
    let mut first = 1;
    let (target, target_rel) = if outdent {
        // Out to the parent item's indentation; an item with none stays
        // (MarkText's `tabCtrl.js:103-113`).
        let Some(parent) = parent else {
            return;
        };
        let (at, blank) = lists.indent(blocks.containers[parent].range.start);
        let columns = lists.rel(at, &blank);
        (blank.into_owned(), columns)
    } else {
        // Under the previous item's text: as its children are indented, or
        // by its marker's width (the task box is text, CommonMark 5.3).
        let Some(&previous) = position.checked_sub(1).and_then(|p| items.get(p)) else {
            return;
        };
        let (child, number) = lists.nested(previous, item_start);
        first = number;
        match child {
            Some(child) => {
                let (at, blank) = lists.indent(child.range.start);
                let columns = lists.rel(at, &blank);
                (blank.into_owned(), columns)
            }
            None => {
                let (at, blank) = lists.indent(blocks.containers[previous].range.start);
                let marker = lists.marker(at + blank.len(), at, &blank, None);
                lists.under(at, &blank, marker)
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
                && lists.delimiter(blocks.containers[p].range.start) == lists.delimiter(item_start)
        })
        .and_then(|(p, l)| Some((p, l, item_number(text, blocks.containers[p].range.start)?.1)));
    // The number moved ordered item `n` (from 0) takes, if it changes.
    let renumbered = |n: usize| match (ordered, outdent) {
        (false, _) => None,
        (true, true) => outer.map(|(_, _, start)| start + 1 + n as u64),
        (true, false) => Some(first + n as u64),
    };
    let mut changes = Vec::new();
    for (n, &moved) in items[position..=last].iter().enumerate() {
        let range = &blocks.containers[moved].range;
        lists.retarget(
            range.start,
            range.end,
            (&target, target_rel),
            renumbered(n),
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
        let lifted_number = renumbered(last - position);
        let (child, number) = lists.nested(lifted, blocks.containers[items[last + 1]].range.start);
        after_first = number;
        let to = match child {
            Some(child) => {
                let (at, blank) = lists.indent(child.range.start);
                let (lifted_at, lifted_own) = lists.indent(blocks.containers[lifted].range.start);
                let new = lists
                    .place(lifted_at, &lifted_own, &lifted_own, &target, target_rel)
                    .2;
                let shift = lists.text_shift(lifted_at, &lifted_own, &new, lifted_number);
                let goal = (lists.rel(at, &blank) as isize + shift).max(0) as usize;
                let (_, _, new) = lists.place(at, &blank, &lifted_own, &target, goal);
                (new, goal)
            }
            None => {
                let (at, blank) = lists.indent(blocks.containers[lifted].range.start);
                let marker = lists.marker(at + blank.len(), at, &target, lifted_number);
                lists.under(at, &target, marker)
            }
        };
        for (n, &after) in items[last + 1..].iter().enumerate() {
            let range = &blocks.containers[after].range;
            let number = ordered.then_some(after_first + n as u64);
            lists.retarget(range.start, range.end, (&to.0, to.1), number, &mut changes);
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
            let range = &blocks.containers[item].range;
            let (at, blank) = lists.indent(range.start);
            lists.retarget(
                range.start,
                range.end,
                (&blank, lists.rel(at, &blank)),
                Some(n),
                changes,
            );
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

/// Tab and Shift+Tab outside list items: Tab types a tab over the
/// selection; Shift+Tab takes one indent unit (a tab, or up to four
/// spaces) off the caret's line, never from its first `floor` columns.
fn plain_indent(doc: &mut Doc, outdent: bool, floor: usize, now: Duration) {
    let selection = doc.selection();
    if !outdent {
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
        return;
    }
    let (line_start, line) = line_at(doc, selection.head);
    let lead = line.len() - line.trim_start_matches([' ', '\t']).len();
    let cut = floor.min(lead);
    let rest = &line[cut..lead];
    let unit = if rest.starts_with('\t') {
        1
    } else {
        rest.bytes().take(4).take_while(|&b| b == b' ').count()
    };
    let cut = line_start + cut;
    if unit == 0 {
        return;
    }
    let moved = |at: usize| {
        if at >= cut + unit {
            at - unit
        } else {
            at.min(cut)
        }
    };
    let selection = Selection {
        anchor: moved(selection.anchor),
        head: moved(selection.head),
    };
    doc.apply(
        vec![Change::delete(cut..cut + unit)],
        selection,
        Kind::Other,
        now,
    );
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

/// How many bytes an item's marker takes with the space after it (`- `,
/// `10. `, `-\t`): up to where its text starts, at least one space, at
/// most four (more is code), or up to a tab.
fn marker_width(item: &str) -> usize {
    let digits = item.bytes().take_while(u8::is_ascii_digit).count();
    let marker = if digits > 0 { digits + 1 } else { 1 };
    let rest = &item[marker.min(item.len())..];
    let run = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    marker
        + match rest[..run].find('\t') {
            Some(tab) => tab + 1,
            None if (1..=4).contains(&run) => run,
            None => 1,
        }
}
