//! Parse events to styled runs, and the markers live preview hides unless
//! the selection touches their construct (REFERENCE-001 sections 2 to 4).
//! Markers are measured from the parser's own ranges, so the view never
//! disagrees with the parser (PLAN-001 contract 3).
use std::ops::Range;

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::parse;

/// How a stretch of source is drawn, in theme-agnostic terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

/// A document's styling, worked out once per edit.
#[derive(Debug, Default)]
pub struct Styled {
    runs: Vec<(Range<usize>, Style)>,
    constructs: Vec<Construct>,
}

impl Styled {
    /// Styles `text`.
    pub fn new(text: &str) -> Self {
        let mut toggles = Vec::new();
        let mut constructs = Vec::new();
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
                    inline(&mut constructs, &mut toggles, &open, syntax, range, width);
                    open.push(index);
                }
                Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough) => {
                    open.pop();
                }
                Event::Code(_) => {
                    toggles.push((range.clone(), Flag::Code));
                    let width = run_of(&text[range.clone()], b'`');
                    inline(
                        &mut constructs,
                        &mut toggles,
                        &open,
                        Syntax::Code,
                        range,
                        width,
                    );
                }
                _ => {}
            }
        }
        Self {
            runs: sweep(toggles),
            constructs,
        }
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

/// Adds an inline construct with `width`-byte markers at both ends.
fn inline(
    constructs: &mut Vec<Construct>,
    toggles: &mut Vec<(Range<usize>, Flag)>,
    open: &[usize],
    syntax: Syntax,
    range: Range<usize>,
    width: usize,
) {
    let markers = [
        range.start..range.start + width,
        range.end - width..range.end,
    ];
    for marker in &markers {
        toggles.push((marker.clone(), Flag::Marker));
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
    let mut counts = [0i32; 5];
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
            marker: counts[4] > 0,
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
                Flag::Marker => counts[4] += delta,
            }
            i += 1;
        }
        from = at;
    }
    runs
}

#[cfg(test)]
mod tests;
