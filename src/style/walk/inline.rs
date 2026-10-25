//! The inline parts of the walk: headings and their `#` runs, emphasis,
//! code spans, links and images with their markers, the markers inside
//! table cells, and backslash escapes.
use std::ops::Range;

use pulldown_cmark::LinkType;

use super::{Span, Walk, line_trim, run_of};
use crate::style::sweep::Flag;
use crate::style::{Construct, Mark, MarkKind, Syntax};

impl Walk<'_> {
    pub(super) fn heading(&mut self, level: u8, range: Range<usize>) {
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
                    hides: true,
                });
            }
        } else {
            // Setext: the underline is syntax; collapsing it is live
            // preview's (REFERENCE-001 section 3).
            // The underline line, after any container prefix (`> `).
            let line = self.text[..end].rfind(['\n', '\r']).map_or(end, |i| i + 1);
            let underline = end
                - self.text[line..end]
                    .trim_start_matches(['>', ' ', '\t'])
                    .len();
            self.toggles.push((underline..end, Flag::Marker));
            self.marks.push(Mark {
                range: underline..end,
                touch: range.start..end,
                kind: MarkKind::Underline,
            });
        }
    }

    /// Adds an inline construct with `width`-byte markers at both ends and
    /// returns its index. Inside a table cell its markers are only dimmed
    /// in the source, which keeps its columns, and hidden by the grid
    /// (REFERENCE-001 section 10).
    pub(super) fn inline(&mut self, syntax: Syntax, range: Range<usize>, width: usize) -> usize {
        let markers = [
            range.start..range.start + width,
            range.end - width..range.end,
        ];
        for marker in &markers {
            self.toggles.push((marker.clone(), Flag::Marker));
        }
        let index = self.constructs.len();
        let hides = self.cells.is_none();
        if !hides {
            self.cell_markers(markers.clone());
        }
        self.constructs.push(Construct {
            syntax,
            range,
            markers,
            group: self.open.first().copied().unwrap_or(index),
            hides,
        });
        index
    }

    pub(super) fn span_start(&mut self, range: Range<usize>, link_type: LinkType, image: bool) {
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
                hides: true,
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
    pub(super) fn span_end(&mut self) {
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
        } else if self.cells.is_some() && !span.image {
            self.cell_markers([opening, closing]);
        }
    }

    /// Markers inside a table cell, for the grid to hide.
    pub(super) fn cell_markers(&mut self, markers: [Range<usize>; 2]) {
        if let Some(table) = self.tables.last_mut() {
            table
                .markers
                .extend(markers.into_iter().filter(|m| !m.is_empty()));
        }
    }

    /// A backslash escape before the text at `range`: the parser leaves the
    /// `\` out of every text event. Its backslash hides unless the escape
    /// itself is touched (REFERENCE-001 section 11).
    pub(super) fn escape(&mut self, range: &Range<usize>) {
        let at = range.start.wrapping_sub(1);
        let escape = self.code.is_none()
            && range.start > 0
            && self.text.as_bytes()[at] == b'\\'
            && !self.in_text(at);
        if !escape {
            return;
        }
        let end = range.start
            + self.text[range.start..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
        let marker = at..range.start;
        self.toggles.push((marker.clone(), Flag::Marker));
        let hides = self.cells.is_none();
        if !hides {
            self.cell_markers([marker.clone(), range.start..range.start]);
        }
        self.constructs.push(Construct {
            syntax: Syntax::Escape,
            range: at..end,
            markers: [marker, end..end],
            group: self.constructs.len(),
            hides,
        });
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
