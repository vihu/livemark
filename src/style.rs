//! Parse events to styled runs, and the markers live preview hides unless
//! the selection touches their construct (REFERENCE-001 sections 2 to 11).
//! Markers are measured from the parser's own ranges, so the view never
//! disagrees with the parser (PLAN-001 contract 3). The walk over the
//! events is `walk.rs`.
mod sweep;
mod walk;

use std::ops::Range;

/// How a stretch of source is drawn, in theme-agnostic terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    /// 0 for body text, 1 to 6 inside a heading of that level.
    pub heading: u8,
    /// Inside `**strong**`.
    pub strong: bool,
    /// Inside `*emphasis*`.
    pub emphasis: bool,
    /// Inside `~~strikethrough~~`.
    pub strikethrough: bool,
    /// Inside `` `code` ``.
    pub code: bool,
    /// Inside a fenced or indented code block, fences included.
    pub code_block: bool,
    /// Inside a GFM table: drawn in the code font, so columns line up
    /// while the source shows (REFERENCE-001 section 10).
    pub table: bool,
    /// The text of a link (REFERENCE-001 section 5).
    pub link: bool,
    /// The text of a checked task (REFERENCE-001 section 8): muted, struck.
    pub done: bool,
    /// Markdown syntax (`#`, `**`, backticks, `>`, list markers, a setext
    /// underline): dimmed.
    pub marker: bool,
    /// In the code font without being code: a task box, so `[ ]` and
    /// `[x]` are as wide and checking one moves nothing.
    pub mono: bool,
}

/// The syntax a [`Construct`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    /// An ATX heading (`## Title`) of this level.
    Heading(u8),
    /// `*emphasis*` or `_emphasis_`.
    Emphasis,
    /// `**strong**` or `__strong__`.
    Strong,
    /// `~~strikethrough~~` or `~strikethrough~`.
    Strikethrough,
    /// `` `code` ``.
    Code,
    /// `[text](url)`, `[text][label]`, `[label]`, or `<url>`.
    Link,
}

/// A parse node whose markers live preview can hide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Construct {
    /// What it is.
    pub syntax: Syntax,
    /// Its source, markers included, line ending excluded: a selection
    /// touching this reveals it.
    pub range: Range<usize>,
    /// Opening and closing markers (either may be empty).
    pub markers: [Range<usize>; 2],
    /// The construct whose touch reveals this one: the outermost inline
    /// span it sits in (REFERENCE-001 section 4), else itself.
    pub group: usize,
}

/// A fenced or indented code block (REFERENCE-001 section 9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlock {
    /// The block, fences included, without the line ending after it.
    pub range: Range<usize>,
    /// The info string's first word (`rust` for ```` ```rust title ````),
    /// empty without one; `None` for an indented block.
    pub language: Option<String>,
    /// The code line by line, without line endings and without the prefix
    /// of a container (`> `) it sits in.
    pub lines: Vec<Range<usize>>,
}

/// Markup live preview draws as something else while the selection does
/// not touch it (inclusive, REFERENCE-001 section 2): the source stays in
/// place, so nothing moves when it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The source drawn over.
    pub range: Range<usize>,
    /// What a selection touches to show the source: the mark itself, the
    /// line of a rule, the block of a fence, the heading of an underline.
    pub touch: Range<usize>,
    /// What it is drawn as.
    pub kind: MarkKind,
}

/// What a [`Mark`] is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// `-`, `+` or `*` before a list item: a dot (section 7).
    Bullet,
    /// `[ ]`, or `[x]` with `true`: a checkbox (section 8).
    Task(bool),
    /// A quote's `>`: a bar (section 6).
    Quote,
    /// A thematic break's line: a horizontal line (section 11).
    Rule,
    /// A code fence: nothing, or the info string as a label at the right
    /// (section 9).
    Fence,
    /// A setext heading's underline: nothing (section 3).
    Underline,
}

/// A document's styling, worked out once per edit.
#[derive(Debug, Default)]
pub struct Styled {
    runs: Vec<(Range<usize>, Style)>,
    constructs: Vec<Construct>,
    code_blocks: Vec<CodeBlock>,
    /// Outermost block quotes, without their last line ending.
    quotes: Vec<Range<usize>>,
    /// Where wrapped rows of a line line up: after a list marker (and task
    /// box) or a quote's `>` prefix (REFERENCE-001 sections 6, 7). Sorted.
    hangs: Vec<usize>,
    /// Sorted by start.
    marks: Vec<Mark>,
}

impl Styled {
    /// Styles `text`.
    pub fn new(text: &str) -> Self {
        walk::walk(text)
    }

    /// Styled runs, in order, not overlapping; text outside them is plain
    /// body text.
    pub fn runs(&self) -> &[(Range<usize>, Style)] {
        &self.runs
    }

    /// The constructs with markers, in document order.
    pub fn constructs(&self) -> &[Construct] {
        &self.constructs
    }

    /// The code blocks, in document order.
    pub fn code_blocks(&self) -> &[CodeBlock] {
        &self.code_blocks
    }

    /// The code block the source line at `line` is part of, fences
    /// included.
    pub fn code_block_at(&self, line: Range<usize>) -> Option<&CodeBlock> {
        let i = self
            .code_blocks
            .partition_point(|b| b.range.end < line.start);
        self.code_blocks
            .get(i)
            .filter(|b| b.range.start <= line.end && line.start <= b.range.end)
    }

    /// Whether the source line at `line` is part of a block quote, lazy
    /// continuation lines included.
    pub fn in_quote(&self, line: Range<usize>) -> bool {
        let i = self.quotes.partition_point(|q| q.end < line.start);
        self.quotes
            .get(i)
            .is_some_and(|q| q.start <= line.end && line.start <= q.end)
    }

    /// The source offset in the line at `line` that its wrapped rows line
    /// up under, if it starts a list item or a quoted line.
    pub fn hang_at(&self, line: Range<usize>) -> Option<usize> {
        let end = self.hangs.partition_point(|&h| h <= line.end);
        self.hangs[..end]
            .last()
            .copied()
            .filter(|&h| h >= line.start)
    }

    /// The marks drawn as something else with `selection` in place: those
    /// it does not touch.
    pub fn concealed(&self, selection: Range<usize>) -> Vec<Mark> {
        self.marks
            .iter()
            .filter(|m| !(selection.start <= m.touch.end && m.touch.start <= selection.end))
            .cloned()
            .collect()
    }

    /// The markers live preview hides with `selection` in place: those of
    /// every construct whose group the selection does not touch (touching
    /// is inclusive at both ends, REFERENCE-001 section 2). Sorted, with
    /// neighbours merged.
    pub fn hidden(&self, selection: Range<usize>) -> Vec<Range<usize>> {
        let touches = |r: &Range<usize>| selection.start <= r.end && r.start <= selection.end;
        let mut hidden: Vec<Range<usize>> = self
            .constructs
            .iter()
            .filter(|c| !touches(&self.constructs[c.group].range))
            .flat_map(|c| c.markers.clone())
            .filter(|m| !m.is_empty())
            .collect();
        hidden.sort_by_key(|m| m.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(hidden.len());
        for m in hidden {
            match merged.last_mut() {
                Some(last) if m.start <= last.end => last.end = last.end.max(m.end),
                _ => merged.push(m),
            }
        }
        merged
    }
}

#[cfg(test)]
mod tests;
