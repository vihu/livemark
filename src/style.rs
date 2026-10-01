//! Parse events to styled runs, and the markers live preview hides unless
//! the selection touches their construct (REFERENCE-001 sections 2 to 4).
//! Markers are measured from the parser's own ranges, so the view never
//! disagrees with the parser (PLAN-001 contract 3).
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};

use crate::parse;

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
    /// Markdown syntax (`#`, `**`, backticks, a setext underline): dimmed.
    pub marker: bool,
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

/// A document's styling, worked out once per edit.
#[derive(Debug, Default)]
pub struct Styled {
    runs: Vec<(Range<usize>, Style)>,
    constructs: Vec<Construct>,
    code_blocks: Vec<CodeBlock>,
}

impl Styled {
    /// Styles `text`.
    pub fn new(text: &str) -> Self {
        let mut toggles = Vec::new();
        let mut constructs = Vec::new();
        let mut code_blocks: Vec<CodeBlock> = Vec::new();
        // The code block or table being read, with its text or cell ranges.
        let mut code: Option<(CodeBlock, Vec<Range<usize>>)> = None;
        let mut cells: Option<(Range<usize>, Vec<Range<usize>>)> = None;
        // Inline constructs open around the current event, outermost first.
        let mut open: Vec<usize> = Vec::new();
        for (event, range) in parse::events(text) {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    let level = level as u8;
                    let end = range.start + line_trim(&text[range.clone()]);
                    toggles.push((range.start..end, Flag::Heading(level)));
                    if text[range.start..].starts_with('#') {
                        let markers = atx_markers(text, range.start..end);
                        for marker in &markers {
                            toggles.push((marker.clone(), Flag::Marker));
                        }
                        // A heading with no text keeps its hashes, or the
                        // line would vanish (REFERENCE-001 section 3).
                        if markers[0].end < markers[1].start {
                            constructs.push(Construct {
                                syntax: Syntax::Heading(level),
                                range: range.start..end,
                                markers,
                                group: constructs.len(),
                            });
                        }
                    } else {
                        // Setext: the underline is syntax; collapsing it is
                        // live preview's (REFERENCE-001 section 3).
                        let underline = text[..end].rfind(['\n', '\r']).map_or(end, |i| i + 1);
                        toggles.push((underline..end, Flag::Marker));
                    }
                }
                Event::Start(tag @ (Tag::Emphasis | Tag::Strong | Tag::Strikethrough)) => {
                    let (syntax, flag, width) = match tag {
                        Tag::Emphasis => (Syntax::Emphasis, Flag::Emphasis, 1),
                        Tag::Strong => (Syntax::Strong, Flag::Strong, 2),
                        _ => (
                            Syntax::Strikethrough,
                            Flag::Strikethrough,
                            run_of(&text[range.clone()], b'~'),
                        ),
                    };
                    toggles.push((range.clone(), flag));
                    let index = constructs.len();
                    let hide = cells.is_none();
                    inline(
                        &mut constructs,
                        &mut toggles,
                        &open,
                        syntax,
                        range,
                        width,
                        hide,
                    );
                    open.push(index);
                }
                Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                    open.pop();
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let end = range.start + line_trim(&text[range.clone()]);
                    let (language, indent) = match kind {
                        CodeBlockKind::Fenced(info) => {
                            let language = info.split_whitespace().next().unwrap_or("");
                            (Some(language.to_owned()), 0)
                        }
                        // An indented block's range starts after its first
                        // line's indentation, which is the block's too.
                        CodeBlockKind::Indented => {
                            let before = &text[..range.start];
                            (
                                None,
                                before.len() - before.trim_end_matches([' ', '\t']).len(),
                            )
                        }
                    };
                    let range = range.start - indent..range.end;
                    toggles.push((range.start..end, Flag::CodeBlock));
                    let block = CodeBlock {
                        range: range.start..end,
                        language,
                        lines: Vec::new(),
                    };
                    code = Some((block, Vec::new()));
                }
                Event::Text(_) if code.is_some() => {
                    if let Some((_, texts)) = &mut code {
                        texts.push(range);
                    }
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some((mut block, texts)) = code.take() {
                        // Around a fenced block's code: fences and the
                        // prefixes of the container it sits in.
                        if block.language.is_some() {
                            for gap in gaps(block.range.clone(), &texts) {
                                toggles.push((gap, Flag::Marker));
                            }
                        }
                        block.lines = texts
                            .iter()
                            .flat_map(|t| lines_of(text, t.clone()))
                            .collect();
                        code_blocks.push(block);
                    }
                }
                Event::Start(Tag::Table(_)) => {
                    let end = range.start + line_trim(&text[range.clone()]);
                    toggles.push((range.start..end, Flag::Table));
                    cells = Some((range.start..end, Vec::new()));
                }
                Event::Start(Tag::TableCell) => {
                    if let Some((_, ranges)) = &mut cells {
                        ranges.push(range);
                    }
                }
                Event::End(TagEnd::Table) => {
                    // Pipes and the delimiter row: everything outside cells.
                    if let Some((table, ranges)) = cells.take() {
                        for gap in gaps(table, &ranges) {
                            toggles.push((gap, Flag::Marker));
                        }
                    }
                }
                Event::Code(_) => {
                    toggles.push((range.clone(), Flag::Code));
                    let width = run_of(&text[range.clone()], b'`');
                    let hide = cells.is_none();
                    inline(
                        &mut constructs,
                        &mut toggles,
                        &open,
                        Syntax::Code,
                        range,
                        width,
                        hide,
                    );
                }
                _ => {}
            }
        }
        Self {
            runs: sweep(toggles),
            constructs,
            code_blocks,
        }
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

    /// Styled runs, in order, not overlapping; text outside them is plain
    /// body text.
    pub fn runs(&self) -> &[(Range<usize>, Style)] {
        &self.runs
    }

    /// The constructs with markers, in document order.
    pub fn constructs(&self) -> &[Construct] {
        &self.constructs
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

/// Adds an inline construct with `width`-byte markers at both ends; with
/// `hide` unset only dims them. Inside a table they stay, or hiding them
/// would pull the columns out of line until tables are drawn as grids
/// (REFERENCE-001 section 10).
fn inline(
    constructs: &mut Vec<Construct>,
    toggles: &mut Vec<(Range<usize>, Flag)>,
    open: &[usize],
    syntax: Syntax,
    range: Range<usize>,
    width: usize,
    hide: bool,
) {
    let markers = [
        range.start..range.start + width,
        range.end - width..range.end,
    ];
    for marker in &markers {
        toggles.push((marker.clone(), Flag::Marker));
    }
    if !hide {
        return;
    }
    constructs.push(Construct {
        syntax,
        range,
        markers,
        group: open.first().copied().unwrap_or(constructs.len()),
    });
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

#[derive(Clone, Copy, Debug)]
enum Flag {
    Heading(u8),
    Strong,
    Emphasis,
    Strikethrough,
    Code,
    CodeBlock,
    Table,
    Marker,
}

/// Flattens nested styled ranges into runs of one style each.
fn sweep(toggles: Vec<(Range<usize>, Flag)>) -> Vec<(Range<usize>, Style)> {
    let mut edges: Vec<(usize, Flag, i32)> = toggles
        .into_iter()
        .filter(|(range, _)| !range.is_empty())
        .flat_map(|(range, flag)| [(range.start, flag, 1), (range.end, flag, -1)])
        .collect();
    edges.sort_by_key(|&(at, _, _)| at);
    let mut runs = Vec::new();
    let mut counts = [0i32; 7];
    let mut heading = 0;
    let mut from = 0;
    let mut i = 0;
    while i < edges.len() {
        let at = edges[i].0;
        let style = Style {
            heading,
            strong: counts[0] > 0,
            emphasis: counts[1] > 0,
            strikethrough: counts[2] > 0,
            code: counts[3] > 0,
            code_block: counts[4] > 0,
            table: counts[5] > 0,
            marker: counts[6] > 0,
        };
        if at > from && style != Style::default() {
            runs.push((from..at, style));
        }
        while i < edges.len() && edges[i].0 == at {
            let (_, flag, delta) = edges[i];
            match flag {
                Flag::Heading(level) => heading = if delta > 0 { level } else { 0 },
                Flag::Strong => counts[0] += delta,
                Flag::Emphasis => counts[1] += delta,
                Flag::Strikethrough => counts[2] += delta,
                Flag::Code => counts[3] += delta,
                Flag::CodeBlock => counts[4] += delta,
                Flag::Table => counts[5] += delta,
                Flag::Marker => counts[6] += delta,
            }
            i += 1;
        }
        from = at;
    }
    runs
}

#[cfg(test)]
mod tests;
