//! Flattening styled ranges, which nest and overlap, into runs of one
//! style each.
use std::ops::Range;

use super::Style;

/// A style a stretch of source has, or a marker.
#[derive(Clone, Copy, Debug)]
pub(super) enum Flag {
    Heading(u8),
    Strong,
    Emphasis,
    Strikethrough,
    Code,
    CodeBlock,
    Table,
    Link,
    Done,
    Marker,
    Mono,
    Meta,
    Highlight,
}

/// Flattens nested styled ranges into runs of one style each.
pub(super) fn sweep(toggles: Vec<(Range<usize>, Flag)>) -> Vec<(Range<usize>, Style)> {
    let mut edges: Vec<(usize, Flag, i32)> = toggles
        .into_iter()
        .filter(|(range, _)| !range.is_empty())
        .flat_map(|(range, flag)| [(range.start, flag, 1), (range.end, flag, -1)])
        .collect();
    edges.sort_by_key(|&(at, _, _)| at);
    let mut runs = Vec::new();
    let mut counts = [0i32; 12];
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
            link: counts[6] > 0,
            done: counts[7] > 0,
            marker: counts[8] > 0,
            mono: counts[9] > 0,
            meta: counts[10] > 0,
            highlight: counts[11] > 0,
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
                Flag::Link => counts[6] += delta,
                Flag::Done => counts[7] += delta,
                Flag::Marker => counts[8] += delta,
                Flag::Mono => counts[9] += delta,
                Flag::Meta => counts[10] += delta,
                Flag::Highlight => counts[11] += delta,
            }
            i += 1;
        }
        from = at;
    }
    runs
}
