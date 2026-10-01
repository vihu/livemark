//! The walk over parse events that builds a [`Styled`]: styled ranges
//! flattened into runs, constructs with their markers, code blocks, quotes
//! and the points wrapped rows hang from.
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Tag, TagEnd};

use super::sweep::{Flag, sweep};
use super::{CodeBlock, Construct, Styled, Syntax};
use crate::parse;

/// Styles `text`.
pub(super) fn walk(text: &str) -> Styled {
    let mut walk = Walk {
        text,
        toggles: Vec::new(),
        constructs: Vec::new(),
        code_blocks: Vec::new(),
        quotes: Vec::new(),
        hangs: Vec::new(),
        texts: Vec::new(),
        open: Vec::new(),
        spans: Vec::new(),
        code: None,
        cells: None,
        quote_depth: 0,
        item: None,
    };
    for (event, range) in parse::events(text) {
        walk.inside_span(&range);
        walk.event(event, range);
    }
    walk.finish()
}

/// A link or image being read.
struct Span {
    /// Its construct; none for images and inside tables.
    construct: Option<usize>,
    range: Range<usize>,
    image: bool,
    /// `<url>` or `<email>`: one-byte markers at both ends.
    angle: bool,
    /// A `www.` link, bare URL or email address: no markers at all.
    bare: bool,
    /// The end of the last event inside it: where its text ends.
    inner_end: Option<usize>,
}

struct Walk<'a> {
    text: &'a str,
    /// Styled ranges, flattened into runs at the end.
    toggles: Vec<(Range<usize>, Flag)>,
    constructs: Vec<Construct>,
    code_blocks: Vec<CodeBlock>,
    quotes: Vec<Range<usize>>,
    hangs: Vec<usize>,
    /// Text and code events in order, which scanned markers never cover.
    texts: Vec<Range<usize>>,
    /// Inline constructs open around the current event, outermost first.
    open: Vec<usize>,
    /// Links and images open around the current event.
    spans: Vec<Span>,
    /// The code block or table being read, with its text or cell ranges.
    code: Option<(CodeBlock, Vec<Range<usize>>)>,
    cells: Option<(Range<usize>, Vec<Range<usize>>)>,
    quote_depth: usize,
    /// The start of a list item whose first inline content is to come.
    item: Option<usize>,
}

impl Walk<'_> {
    fn event(&mut self, event: Event, range: Range<usize>) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => self.heading(level as u8, range),
            Event::Start(tag @ (Tag::Emphasis | Tag::Strong | Tag::Strikethrough)) => {
                self.content(&range);
                let (syntax, flag, width) = match tag {
                    Tag::Emphasis => (Syntax::Emphasis, Flag::Emphasis, 1),
                    Tag::Strong => (Syntax::Strong, Flag::Strong, 2),
                    _ => (
                        Syntax::Strikethrough,
                        Flag::Strikethrough,
                        run_of(&self.text[range.clone()], b'~'),
                    ),
                };
                self.toggles.push((range.clone(), flag));
                let index = self.inline(syntax, range, width);
                self.open.push(index);
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                self.open.pop();
            }
            Event::Start(Tag::Link { link_type, .. }) => {
                self.content(&range);
                self.span_start(range, link_type, false);
            }
            Event::Start(Tag::Image { link_type, .. }) => {
                self.content(&range);
                self.span_start(range, link_type, true);
            }
            Event::End(TagEnd::Link | TagEnd::Image) => self.span_end(),
            Event::Code(_) => {
                self.content(&range);
                self.texts.push(range.clone());
                self.toggles.push((range.clone(), Flag::Code));
                let width = run_of(&self.text[range.clone()], b'`');
                self.inline(Syntax::Code, range, width);
            }
            Event::Text(_) => {
                self.texts.push(range.clone());
                match &mut self.code {
                    Some((_, texts)) => texts.push(range),
                    None => self.content(&range),
                }
            }
            Event::InlineHtml(_) | Event::InlineMath(_) | Event::FootnoteReference(_) => {
                self.content(&range);
            }
            Event::Start(Tag::CodeBlock(kind)) => self.code_start(kind, range),
            Event::End(TagEnd::CodeBlock) => self.code_end(),
            Event::Start(Tag::Table(_)) => {
                let end = range.start + line_trim(&self.text[range.clone()]);
                self.toggles.push((range.start..end, Flag::Table));
                self.cells = Some((range.start..end, Vec::new()));
            }
            Event::Start(Tag::TableCell) => {
                if let Some((_, cells)) = &mut self.cells {
                    cells.push(range);
                }
            }
            Event::End(TagEnd::Table) => {
                // Pipes and the delimiter row: everything outside cells.
                if let Some((table, cells)) = self.cells.take() {
                    for gap in gaps(table, &cells) {
                        self.toggles.push((gap, Flag::Marker));
                    }
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                if self.quote_depth == 0 {
                    let end = range.start + line_trim(&self.text[range.clone()]);
                    self.quotes.push(range.start..end);
                }
                self.quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => self.quote_depth -= 1,
            Event::Start(Tag::Item) => {
                // The bullet, or the number and its `.` or `)`.
                let item = &self.text[range.start..];
                let digits = item.bytes().take_while(u8::is_ascii_digit).count();
                let width = if digits > 0 { digits + 1 } else { 1 };
                let marker = range.start..(range.start + width).min(range.end);
                self.toggles.push((marker, Flag::Marker));
                self.item = Some(range.start);
            }
            Event::End(TagEnd::Item) => self.item = None,
            Event::TaskListMarker(checked) => {
                self.toggles.push((range.clone(), Flag::Marker));
                if checked {
                    let rest = &self.text[range.end..];
                    let line = &rest[..rest.find(['\n', '\r']).unwrap_or(rest.len())];
                    let start = range.end + line.len() - line.trim_start().len();
                    self.toggles
                        .push((start..range.end + line.len(), Flag::Done));
                }
            }
            Event::Rule => {
                let end = range.start + line_trim(&self.text[range.clone()]);
                self.toggles.push((range.start..end, Flag::Marker));
            }
            _ => {}
        }
    }

    /// Inline content starting at `range`: where a list item's wrapped rows
    /// hang, when it is on the item's first line.
    fn content(&mut self, range: &Range<usize>) {
        if let Some(item) = self.item.take()
            && !self.text[item..range.start].contains(['\n', '\r'])
        {
            self.hangs.push(range.start);
        }
    }

    /// Notes how far the events inside the innermost link or image reach.
    fn inside_span(&mut self, range: &Range<usize>) {
        if let Some(span) = self.spans.last_mut()
            && span.range != *range
            && span.range.start <= range.start
            && range.end <= span.range.end
        {
            span.inner_end = Some(span.inner_end.unwrap_or(0).max(range.end));
        }
    }

    fn heading(&mut self, level: u8, range: Range<usize>) {
        let end = range.start + line_trim(&self.text[range.clone()]);
        self.toggles.push((range.start..end, Flag::Heading(level)));
        if self.text[range.start..].starts_with('#') {
            let markers = atx_markers(self.text, range.start..end);
            for marker in &markers {
                self.toggles.push((marker.clone(), Flag::Marker));
            }
            // A heading with no text keeps its hashes, or the line would
            // vanish (REFERENCE-001 section 3).
            if markers[0].end < markers[1].start {
                self.constructs.push(Construct {
                    syntax: Syntax::Heading(level),
                    range: range.start..end,
                    markers,
                    group: self.constructs.len(),
                });
            }
        } else {
            // Setext: the underline is syntax; collapsing it is live
            // preview's (REFERENCE-001 section 3).
            let underline = self.text[..end].rfind(['\n', '\r']).map_or(end, |i| i + 1);
            self.toggles.push((underline..end, Flag::Marker));
        }
    }

    /// Adds an inline construct with `width`-byte markers at both ends and
    /// returns its index. Inside a table its markers are only dimmed:
    /// hiding them would pull the columns out of line until tables are
    /// drawn as grids (REFERENCE-001 section 10).
    fn inline(&mut self, syntax: Syntax, range: Range<usize>, width: usize) -> usize {
        let markers = [
            range.start..range.start + width,
            range.end - width..range.end,
        ];
        for marker in &markers {
            self.toggles.push((marker.clone(), Flag::Marker));
        }
        let index = self.constructs.len();
        if self.cells.is_none() {
            self.constructs.push(Construct {
                syntax,
                range,
                markers,
                group: self.open.first().copied().unwrap_or(index),
            });
        }
        index
    }

    fn span_start(&mut self, range: Range<usize>, link_type: LinkType, image: bool) {
        let auto = matches!(link_type, LinkType::Autolink | LinkType::Email);
        let angle = auto && self.text[range.start..].starts_with('<');
        let bare = auto && !angle;
        let construct = (!image && !bare && self.cells.is_none()).then(|| {
            let index = self.constructs.len();
            self.constructs.push(Construct {
                syntax: Syntax::Link,
                range: range.clone(),
                markers: [range.start..range.start, range.end..range.end],
                group: self.open.first().copied().unwrap_or(index),
            });
            self.open.push(index);
            index
        });
        self.spans.push(Span {
            construct,
            range,
            image,
            angle,
            bare,
            inner_end: None,
        });
    }

    /// A link's or image's markers: `[` (`![`) and everything after its
    /// text, or `<` and `>`. Without text it is all syntax and never hides
    /// (REFERENCE-001 section 5).
    fn span_end(&mut self) {
        let Some(span) = self.spans.pop() else {
            return;
        };
        if span.construct.is_some() {
            self.open.pop();
        }
        if let Some(parent) = self.spans.last_mut() {
            parent.inner_end = Some(parent.inner_end.unwrap_or(0).max(span.range.end));
        }
        let Range { start, end } = span.range;
        if span.bare {
            self.toggles.push((start..end, Flag::Link));
            return;
        }
        let Some(inner) = span.inner_end else {
            self.toggles.push((start..end, Flag::Marker));
            return;
        };
        let opening = start..start + if span.image { 2 } else { 1 };
        let closing = if span.angle { end - 1..end } else { inner..end };
        self.toggles.push((opening.clone(), Flag::Marker));
        self.toggles.push((closing.clone(), Flag::Marker));
        if !span.image {
            self.toggles.push((opening.end..closing.start, Flag::Link));
        }
        if let Some(index) = span.construct {
            self.constructs[index].markers = [opening, closing];
        }
    }

    fn code_start(&mut self, kind: CodeBlockKind, range: Range<usize>) {
        let end = range.start + line_trim(&self.text[range.clone()]);
        let (language, indent) = match kind {
            CodeBlockKind::Fenced(info) => {
                let language = info.split_whitespace().next().unwrap_or("");
                (Some(language.to_owned()), 0)
            }
            // An indented block's range starts after its first line's
            // indentation, which is the block's too.
            CodeBlockKind::Indented => {
                let before = &self.text[..range.start];
                (
                    None,
                    before.len() - before.trim_end_matches([' ', '\t']).len(),
                )
            }
        };
        let start = range.start - indent;
        self.toggles.push((start..end, Flag::CodeBlock));
        let block = CodeBlock {
            range: start..end,
            language,
            lines: Vec::new(),
        };
        self.code = Some((block, Vec::new()));
    }

    fn code_end(&mut self) {
        if let Some((mut block, texts)) = self.code.take() {
            // Around a fenced block's code: fences and the prefixes of the
            // container it sits in.
            if block.language.is_some() {
                for gap in gaps(block.range.clone(), &texts) {
                    self.toggles.push((gap, Flag::Marker));
                }
            }
            block.lines = texts
                .iter()
                .flat_map(|t| lines_of(self.text, t.clone()))
                .collect();
            self.code_blocks.push(block);
        }
    }

    /// Each line of a quote: its `>` markers (nested ones too, never inside
    /// text) dimmed, and its wrapped rows hanging after them.
    fn quote_markers(&mut self, quote: Range<usize>) {
        let bytes = self.text.as_bytes();
        let mut line = quote.start;
        loop {
            let mut at = line;
            let mut hang = None;
            loop {
                while matches!(bytes.get(at), Some(b' ' | b'\t')) {
                    at += 1;
                }
                if bytes.get(at) != Some(&b'>') || self.in_text(at) {
                    break;
                }
                self.toggles.push((at..at + 1, Flag::Marker));
                at += 1;
                if bytes.get(at) == Some(&b' ') {
                    at += 1;
                }
                hang = Some(at);
            }
            self.hangs.extend(hang);
            match self.text[line..quote.end].find('\n') {
                Some(i) => line += i + 1,
                None => break,
            }
        }
    }

    fn in_text(&self, at: usize) -> bool {
        let i = self.texts.partition_point(|t| t.end <= at);
        self.texts.get(i).is_some_and(|t| t.start <= at)
    }

    fn finish(mut self) -> Styled {
        for quote in self.quotes.clone() {
            self.quote_markers(quote);
        }
        self.hangs.sort_unstable();
        self.hangs.dedup();
        Styled {
            runs: sweep(self.toggles),
            constructs: self.constructs,
            code_blocks: self.code_blocks,
            quotes: self.quotes,
            hangs: self.hangs,
        }
    }
}

/// The opening `#` run with the spaces after it, and the optional closing
/// sequence with the spaces around it (CommonMark 4.2), in an ATX heading
/// without its line ending.
fn atx_markers(text: &str, heading: Range<usize>) -> [Range<usize>; 2] {
    let line = &text[heading.clone()];
    let hashes = run_of(line, b'#');
    let open = hashes + line[hashes..].len() - line[hashes..].trim_start_matches([' ', '\t']).len();
    let rest = line[open..].trim_end_matches([' ', '\t']);
    let before = rest.trim_end_matches('#');
    let close = if before.len() == rest.len() {
        line.len()
    } else if before.is_empty() {
        open
    } else if before.ends_with([' ', '\t']) {
        open + before.trim_end_matches([' ', '\t']).len()
    } else {
        line.len()
    };
    [
        heading.start..heading.start + open,
        heading.start + close..heading.end,
    ]
}

/// The stretches of `outer` outside the sorted ranges `inner`.
fn gaps(outer: Range<usize>, inner: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut gaps = Vec::new();
    let mut at = outer.start;
    for range in inner {
        if at < range.start {
            gaps.push(at..range.start.min(outer.end));
        }
        at = at.max(range.end);
    }
    if at < outer.end {
        gaps.push(at..outer.end);
    }
    gaps
}

/// The lines of `range` in `text`, each without its line ending.
fn lines_of(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut start = range.start;
    for (i, byte) in text[range.clone()].bytes().enumerate() {
        if byte == b'\n' {
            let end = range.start + i;
            let end = if text[start..end].ends_with('\r') {
                end - 1
            } else {
                end
            };
            lines.push(start..end);
            start = range.start + i + 1;
        }
    }
    if start < range.end {
        lines.push(start..range.start + line_trim(&text[range.clone()]));
    }
    lines
}

/// How many `byte`s `text` starts with.
fn run_of(text: &str, byte: u8) -> usize {
    text.bytes().take_while(|&b| b == byte).count()
}

/// The length of `text` without a trailing line ending.
fn line_trim(text: &str) -> usize {
    text.trim_end_matches(['\n', '\r']).len()
}
