//! The markup around a position (CodeMirror's `getContext`, `Context`
//! and `renumberList`, `lang-markdown` `dist/index.js:91-180`, MIT): the
//! quotes and list items holding it, read from pulldown-cmark's events.
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

use crate::doc::Change;
use crate::parse;

/// The quotes, list items and lists of a document, and its fenced code.
pub(super) struct Blocks {
    pub(super) containers: Vec<Container>,
    pub(super) lists: Vec<List>,
    pub(super) fenced: Vec<Range<usize>>,
}

/// A quote or list item, its range without trailing line endings.
pub(super) struct Container {
    pub(super) range: Range<usize>,
    /// For an item: the list it is in.
    pub(super) list: Option<usize>,
}

pub(super) struct List {
    pub(super) start: usize,
    pub(super) ordered: bool,
    /// Its items, as indices into `containers`, in order.
    pub(super) items: Vec<usize>,
}

impl Blocks {
    pub(super) fn new(text: &str) -> Self {
        let mut blocks = Blocks {
            containers: Vec::new(),
            lists: Vec::new(),
            fenced: Vec::new(),
        };
        let mut open_lists = Vec::new();
        // Without line endings at the end, nor at the start: pulldown-cmark
        // can start a nested item at the line ending before its line.
        let trim = |range: Range<usize>| {
            let lead = text[range.clone()].len()
                - text[range.clone()].trim_start_matches(['\n', '\r']).len();
            let start = range.start + lead;
            start..start + text[start..range.end].trim_end_matches(['\n', '\r']).len()
        };
        for (event, range) in parse::events(text) {
            match event {
                Event::Start(Tag::List(first)) => {
                    open_lists.push(blocks.lists.len());
                    blocks.lists.push(List {
                        start: range.start,
                        ordered: first.is_some(),
                        items: Vec::new(),
                    });
                }
                Event::End(TagEnd::List(_)) => {
                    open_lists.pop();
                }
                Event::Start(Tag::Item) => {
                    let list = open_lists.last().copied();
                    if let Some(list) = list {
                        blocks.lists[list].items.push(blocks.containers.len());
                    }
                    blocks.containers.push(Container {
                        range: trim(range),
                        list,
                    });
                }
                Event::Start(Tag::BlockQuote(_)) => blocks.containers.push(Container {
                    range: trim(range),
                    list: None,
                }),
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_))) => blocks.fenced.push(range),
                _ => {}
            }
        }
        blocks
    }

    /// The quotes and items around `pos`, outermost first; none inside
    /// fenced code.
    fn around(&self, pos: usize) -> Vec<usize> {
        if self.fenced.iter().any(|f| f.start < pos && pos < f.end) {
            return Vec::new();
        }
        (0..self.containers.len())
            .filter(|&i| {
                let r = &self.containers[i].range;
                r.start <= pos && pos <= r.end
            })
            .collect()
    }
}

/// One level of markup on the line a container starts on (CodeMirror's
/// `Context`): columns in that line.
#[derive(Clone, Debug)]
pub(super) struct Context {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) space_before: String,
    pub(super) space_after: String,
    /// `>`, the bullet (with ` [ ]` for a task), or the ordered delimiter.
    pub(super) kind: String,
    pub(super) quote: bool,
    /// For an item: the container and its list.
    pub(super) item: Option<(usize, usize)>,
}

impl Context {
    /// Blank space as wide as this markup: the quote's `>` kept.
    pub(super) fn blank(&self, max_width: Option<usize>, trailing: bool) -> String {
        let mut result = self.space_before.clone();
        if self.quote {
            result.push('>');
        }
        match max_width {
            Some(width) => {
                while result.len() < width {
                    result.push(' ');
                }
                result
            }
            None => {
                let pad =
                    (self.to - self.from).saturating_sub(result.len() + self.space_after.len());
                result.push_str(&" ".repeat(pad));
                if trailing {
                    result.push_str(&self.space_after);
                }
                result
            }
        }
    }

    /// This markup for a new item, its number moved on by `add`.
    pub(super) fn marker(&self, text: &str, blocks: &Blocks, add: i64) -> String {
        let number = match self.item {
            Some((item, list)) if blocks.lists[list].ordered => {
                let n =
                    item_number(text, blocks.containers[item].range.start).map_or(1, |(_, n)| n);
                (n as i64 + add).max(0).to_string()
            }
            _ => String::new(),
        };
        format!(
            "{}{number}{}{}",
            self.space_before, self.kind, self.space_after
        )
    }
}

/// The markup levels around `pos` (CodeMirror's `getContext`).
pub(super) fn contexts(text: &str, blocks: &Blocks, pos: usize) -> Vec<Context> {
    let mut contexts = Vec::new();
    for index in blocks.around(pos) {
        let container = &blocks.containers[index];
        let start = container.range.start;
        let line_start = text[..start].rfind(['\n', '\r']).map_or(0, |i| i + 1);
        let rest = &text[start..];
        let from = start - line_start;
        // Tabs too: Obsidian indents lists with tabs.
        let spaces = rest
            .bytes()
            .take_while(|&b| b == b' ' || b == b'\t')
            .count();
        let indent = &rest[..spaces];
        let after = &rest[spaces..];
        let context = match container.list {
            None => after.strip_prefix('>').map(|tail| {
                let space = if tail.starts_with(' ') { " " } else { "" };
                Context {
                    from,
                    to: from + spaces + 1 + space.len(),
                    space_before: indent.to_owned(),
                    space_after: space.into(),
                    kind: ">".into(),
                    quote: true,
                    item: None,
                }
            }),
            Some(list) if blocks.lists[list].ordered => {
                let digits = after.bytes().take_while(u8::is_ascii_digit).count();
                let delimiter = after[digits..]
                    .chars()
                    .next()
                    .filter(|c| matches!(c, '.' | ')'));
                (digits > 0).then_some(()).and(delimiter).map(|delimiter| {
                    let tail = &after[digits + 1..];
                    let mut space = tail.bytes().take_while(|&b| b == b' ').count();
                    let mut len = spaces + digits + 1 + space;
                    if space >= 4 {
                        space -= 4;
                        len -= 4;
                    }
                    Context {
                        from,
                        to: from + len,
                        space_before: indent.to_owned(),
                        space_after: " ".repeat(space),
                        kind: delimiter.to_string(),
                        quote: false,
                        item: Some((index, list)),
                    }
                })
            }
            Some(list) => bullet(after).map(|(bullet, task, space)| {
                let mut space_after = space;
                let mut len = spaces + 1 + task.len() + space;
                if space_after > 4 {
                    space_after -= 4;
                    len -= 4;
                }
                // A task continues unchecked.
                let kind = format!("{bullet}{}", task.replace(['x', 'X'], " "));
                Context {
                    from,
                    to: from + len,
                    space_before: indent.to_owned(),
                    space_after: " ".repeat(space_after),
                    kind,
                    quote: false,
                    item: Some((index, list)),
                }
            }),
        };
        contexts.extend(context);
    }
    contexts
}

/// `-`, `+` or `*`, then an optional task box after 1 to 4 spaces, then at
/// least one space: the bullet, the box with its spaces, and the spaces.
fn bullet(text: &str) -> Option<(char, &str, usize)> {
    let bullet = text
        .chars()
        .next()
        .filter(|c| matches!(c, '-' | '+' | '*'))?;
    let rest = &text[1..];
    let lead = rest.bytes().take_while(|&b| b == b' ').count();
    let boxed = (1..=4).contains(&lead)
        && rest[lead..].len() >= 3
        && rest.as_bytes()[lead] == b'['
        && matches!(rest.as_bytes()[lead + 1], b' ' | b'x' | b'X')
        && rest.as_bytes()[lead + 2] == b']';
    let task = if boxed { &rest[..lead + 3] } else { "" };
    let space = rest[task.len()..]
        .bytes()
        .take_while(|&b| b == b' ')
        .count();
    (space > 0).then_some((bullet, task, space))
}

/// The indentation and number of the ordered item starting at `start`.
pub(super) fn item_number(text: &str, start: usize) -> Option<(Range<usize>, u64)> {
    let rest = &text[start..];
    let spaces = rest
        .bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count();
    // CommonMark allows nine digits at most.
    let digits = rest[spaces..]
        .bytes()
        .take(9)
        .take_while(u8::is_ascii_digit)
        .count();
    let at = start + spaces;
    rest[spaces..spaces + digits]
        .parse()
        .ok()
        .map(|n| (at..at + digits, n))
}

/// Renumbers the consecutive ordered items after `after` in its list,
/// each moved by `offset + 1` (CodeMirror's `renumberList`).
pub(super) fn renumber(
    text: &str,
    blocks: &Blocks,
    item: usize,
    list: usize,
    offset: i64,
    changes: &mut Vec<Change>,
) {
    let items = &blocks.lists[list].items;
    let Some(position) = items.iter().position(|&i| i == item) else {
        return;
    };
    let Some((_, mut prev)) = item_number(text, blocks.containers[item].range.start) else {
        return;
    };
    for &next in &items[position + 1..] {
        let start = blocks.containers[next].range.start;
        let Some((digits, number)) = item_number(text, start) else {
            return;
        };
        if number != prev + 1 {
            return;
        }
        let new = (prev as i64 + 2 + offset).max(0).to_string();
        changes.push(Change {
            range: digits,
            text: new,
        });
        prev = number;
    }
}
