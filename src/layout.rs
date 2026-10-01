//! The display projection: a source line drawn without its hidden ranges,
//! with offsets mapped both ways, so every visible position is a source
//! position and back (PLAN-001 contract 4).
use std::ops::Range;

use crate::style::Style;

/// Which source offset a display offset stands for where hidden text sits
/// between two glyphs: right after the glyph before it, or right before the
/// glyph after it (REFERENCE-001 section 14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    /// The offset right after the glyph before (the hidden text follows).
    Before,
    /// The offset right before the glyph after (the hidden text precedes).
    After,
}

/// A source line as it is drawn.
#[derive(Debug)]
pub struct Line {
    /// What is drawn: the line without its hidden ranges and line ending.
    pub text: String,
    /// Styled runs in display offsets; the rest is body text.
    pub runs: Vec<(Range<usize>, Style)>,
    /// The visible stretches of source between hidden ranges, in order and
    /// back to back in display offsets; empty ones keep the line's ends
    /// reachable when a hidden range starts or ends there.
    pieces: Vec<Piece>,
}

#[derive(Clone, Copy, Debug)]
struct Piece {
    source: usize,
    display: usize,
    len: usize,
}

impl Line {
    /// Projects the source line `line` (without its line ending) of `text`,
    /// leaving out `hidden` (sorted, merged ranges of the whole document)
    /// and styling it with `runs` (sorted, of the whole document).
    pub fn new(
        text: &str,
        line: Range<usize>,
        hidden: &[Range<usize>],
        runs: &[(Range<usize>, Style)],
    ) -> Self {
        let mut pieces = Vec::new();
        let mut display = 0;
        let mut from = line.start;
        let first = hidden.partition_point(|h| h.end <= line.start);
        for h in hidden[first..].iter().take_while(|h| h.start < line.end) {
            let (start, end) = (h.start.max(line.start), h.end.min(line.end));
            pieces.push(Piece {
                source: from,
                display,
                len: start - from,
            });
            display += start - from;
            from = end;
        }
        pieces.push(Piece {
            source: from,
            display,
            len: line.end - from,
        });
        let mut shown = String::with_capacity(display + line.end - from);
        for p in &pieces {
            shown.push_str(&text[p.source..p.source + p.len]);
        }
        let first = runs.partition_point(|(r, _)| r.end <= line.start);
        let mut styled = Vec::new();
        for (range, style) in runs[first..].iter().take_while(|(r, _)| r.start < line.end) {
            for p in &pieces {
                let start = range.start.max(p.source);
                let end = range.end.min(p.source + p.len);
                if start < end {
                    let at = p.display + start - p.source;
                    styled.push((at..at + end - start, *style));
                }
            }
        }
        Self {
            text: shown,
            runs: styled,
            pieces,
        }
    }

    /// Where source offset `source` (in this line, line ending excluded)
    /// is drawn. Offsets inside a hidden range are drawn where it was.
    pub fn to_display(&self, source: usize) -> usize {
        let i = self
            .pieces
            .partition_point(|p| p.source + p.len < source)
            .min(self.pieces.len() - 1);
        let p = self.pieces[i];
        p.display + source.saturating_sub(p.source).min(p.len)
    }

    /// The source offset drawn at `display` (0 to the drawn length): with
    /// [`Affinity::Before`] the first one there, with
    /// [`Affinity::After`] the last; they differ only where hidden text
    /// sits.
    pub fn to_source(&self, display: usize, affinity: Affinity) -> usize {
        let i = match affinity {
            Affinity::Before => self
                .pieces
                .partition_point(|p| p.display + p.len < display)
                .min(self.pieces.len() - 1),
            Affinity::After => self
                .pieces
                .partition_point(|p| p.display <= display)
                .saturating_sub(1),
        };
        let p = self.pieces[i];
        p.source + (display - p.display).min(p.len)
    }
}

#[cfg(test)]
mod tests;
