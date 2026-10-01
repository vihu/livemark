//! Random editing sessions on `Doc`: typing, deleting, pasting, wrapping,
//! caret moves, undo and redo in any order must never panic, must leave the
//! text equal to the same edits on a plain `String` (PLAN-001 contract 2),
//! and must keep the line starts and `version` honest. Seeded, so a failure
//! replays from the seed and step in its message.
use std::collections::HashMap;
use std::time::Duration;

use common::Rng;
use livemark::doc::{Change, Doc, Kind, Selection};

mod common;

const SEEDS: u64 = 12;
const STEPS: usize = 400;

/// `LIVEMARK_FUZZ_SEEDS=<n>` runs more sessions, e.g. with `--release`;
/// `LIVEMARK_FUZZ_ONLY=<seed>` replays one.
#[test]
fn random_sessions_match_a_plain_string() {
    for seed in common::seeds(SEEDS) {
        session(seed);
    }
}

/// Text that stresses offsets and line starts: multi-byte characters and
/// every line ending, alone and in pairs.
const SNIPPETS: &[&str] = &[
    "a", "é", "😀", " ", "\t", "\n", "\r\n", "\r", "**", "`", "# ", "- [ ] ", "> ", "\n\n",
];

fn session(seed: u64) {
    let start = match seed % 3 {
        0 => String::new(),
        1 => fixture("coverage.md"),
        _ => fixture("line-endings.md"),
    };
    let mut doc = Doc::new(start.clone());
    let mut oracle = start.clone();
    // Every version seen and the text it stood for.
    let mut seen = HashMap::from([(doc.version(), start.clone())]);
    let mut rng = Rng(seed);
    let mut now = Duration::ZERO;
    for step in 0..STEPS {
        let at = format!("seed {seed} step {step}");
        now += Duration::from_millis(rng.below(800));
        match rng.below(10) {
            0..=3 => {
                let range = doc.selection().range();
                let text = *rng.pick(SNIPPETS);
                let after = range.start + text.len();
                let changes = vec![Change {
                    range,
                    text: text.into(),
                }];
                edit(
                    &mut doc,
                    &mut oracle,
                    changes,
                    Selection::caret(after),
                    Kind::Type,
                    now,
                );
            }
            4 => {
                // Backspace over up to three characters.
                let caret = doc.selection().head;
                let from = doc.text()[..caret]
                    .char_indices()
                    .rev()
                    .nth(rng.below(3) as usize)
                    .map_or(0, |(i, _)| i);
                let changes = vec![Change::delete(from..caret)];
                edit(
                    &mut doc,
                    &mut oracle,
                    changes,
                    Selection::caret(from),
                    Kind::Delete,
                    now,
                );
            }
            5 => {
                let range = doc.selection().range();
                let text: String = (0..rng.below(6)).map(|_| *rng.pick(SNIPPETS)).collect();
                let after = range.start + text.len();
                let changes = vec![Change { range, text }];
                edit(
                    &mut doc,
                    &mut oracle,
                    changes,
                    Selection::caret(after),
                    Kind::Other,
                    now,
                );
            }
            6 => {
                // Wrap the selection, like a formatting toggle.
                let range = doc.selection().range();
                let changes = vec![
                    Change::insert(range.start, "**"),
                    Change::insert(range.end, "**"),
                ];
                let selection = Selection {
                    anchor: range.start + 2,
                    head: range.end + 2,
                };
                edit(&mut doc, &mut oracle, changes, selection, Kind::Other, now);
            }
            7 => {
                let selection = Selection {
                    anchor: rng.boundary(doc.text()),
                    head: rng.boundary(doc.text()),
                };
                doc.set_selection(selection);
                // An end inside a CRLF moves before its `\r`.
                let fit = |end: usize| {
                    let inside =
                        doc.text()[..end].ends_with('\r') && doc.text()[end..].starts_with('\n');
                    if inside { end - 1 } else { end }
                };
                let expected = Selection {
                    anchor: fit(selection.anchor),
                    head: fit(selection.head),
                };
                assert_eq!(doc.selection(), expected, "{at}");
            }
            _ => {
                for _ in 0..1 + rng.below(3) {
                    if rng.below(2) == 0 {
                        doc.undo();
                    } else {
                        doc.redo();
                    }
                }
                oracle = doc.text().to_owned();
            }
        }
        check(&doc, &oracle, &mut seen, &at);
    }
    // Everything undone is the loaded text; everything redone, the newest.
    while doc.redo() {}
    let last = doc.text().to_owned();
    while doc.undo() {}
    assert_eq!(doc.text(), start, "seed {seed}: undo all");
    while doc.redo() {}
    assert_eq!(doc.text(), last, "seed {seed}: redo all");
}

fn edit(
    doc: &mut Doc,
    oracle: &mut String,
    changes: Vec<Change>,
    selection: Selection,
    kind: Kind,
    now: Duration,
) {
    for change in changes.iter().rev() {
        oracle.replace_range(change.range.clone(), &change.text);
    }
    doc.apply(changes, selection, kind, now);
}

fn check(doc: &Doc, oracle: &str, seen: &mut HashMap<u64, String>, at: &str) {
    assert_eq!(
        doc.text(),
        oracle,
        "{at}: text differs from the plain string"
    );
    let text = doc.text();
    let selection = doc.selection();
    for end in [selection.anchor, selection.head] {
        assert!(text.is_char_boundary(end), "{at}: selection {selection:?}");
    }
    // Line starts as a full scan finds them.
    let mut starts = vec![0];
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' || (b == b'\r' && bytes.get(i + 1) != Some(&b'\n')) {
            starts.push(i + 1);
        }
    }
    let ours: Vec<usize> = (0..doc.line_count())
        .map(|i| doc.line_range(i).start)
        .collect();
    assert_eq!(ours, starts, "{at}: line starts");
    for (i, &start) in starts.iter().enumerate() {
        assert_eq!(doc.line_at(start), i, "{at}: line_at({start})");
    }
    // A version names one text, whenever it comes back.
    let known = seen.entry(doc.version()).or_insert_with(|| text.to_owned());
    assert_eq!(
        known,
        text,
        "{at}: version {} came back with other text",
        doc.version()
    );
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/docs/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
