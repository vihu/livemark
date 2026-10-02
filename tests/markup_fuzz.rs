//! Enter, Shift+Enter, Backspace, Tab and Shift+Tab around list and quote
//! markup, the formatting keys and the line commands, at random carets and
//! selections in
//! random markdown: never a panic, and every edit is one `Doc` accepts
//! (sorted, on character boundaries), so the source stays valid UTF-8
//! text. Seeded, like `doc_fuzz.rs`.
use std::time::Duration;

use common::Rng;
use livemark::doc::{Doc, Selection};
use livemark::edit::format::{self, Format};
use livemark::edit::{self, lines, markup};
use livemark::style::Styled;

mod common;

const SEEDS: u64 = 300;

/// Markdown in pieces: list and quote markup, task boxes, indentation,
/// multi-byte text, fences and every line ending.
const PIECES: &[&str] = &[
    "- ", "* ", "+ ", "1. ", "10) ", "> ", "  ", "    ", "\t", "[ ] ", "[x] ", "a", "word", "é",
    "😀", "\n", "\r\n", "\n\n", "```\n", "-", ">", "**", "*", "`", "_", "[", "](u)",
];

#[test]
fn markup_commands_never_break_the_text() {
    for seed in common::seeds(SEEDS) {
        let mut rng = Rng(seed);
        let text: String = (0..rng.below(60)).map(|_| *rng.pick(PIECES)).collect();
        let mut doc = Doc::new(text);
        for step in 0..30 {
            let caret = rng.boundary(doc.text());
            let anchor = if rng.below(3) == 0 {
                rng.boundary(doc.text())
            } else {
                caret
            };
            doc.set_selection(Selection {
                anchor,
                head: caret,
            });
            let before = doc.text().to_owned();
            let now = Duration::from_secs(step);
            match rng.below(13) {
                6 => {
                    let styled = Styled::new(doc.text());
                    let format = *rng.pick(&[Format::Bold, Format::Italic, Format::Code]);
                    format::toggle(&mut doc, &styled, format, now);
                }
                7 => format::link(&mut doc, now),
                0 => edit::enter(&mut doc, now),
                1 => markup::soft_break(&mut doc, now),
                2 => edit::backspace(&mut doc, now),
                3 => markup::indent(&mut doc, false, now),
                4 => markup::indent(&mut doc, true, now),
                10 => lines::move_lines(&mut doc, rng.below(2) == 0, now),
                11 => lines::copy_lines(&mut doc, rng.below(2) == 0, now),
                12 if rng.below(2) == 0 => lines::delete_lines(&mut doc, now),
                12 => lines::blank_line(&mut doc, now),
                _ => {
                    // Named, so the oldest supported Rust and clippy agree.
                    let piece = *rng.pick::<&str>(PIECES);
                    edit::type_text(&mut doc, piece, now);
                }
            }
            let selection = doc.selection();
            assert!(
                doc.text().is_char_boundary(selection.head),
                "seed {seed} step {step}: caret {selection:?} in {before:?}"
            );
        }
        while doc.undo() {}
    }
}
