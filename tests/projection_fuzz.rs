//! Random markdown and random selections through styling and the display
//! projection (PLAN-001 contract 4, REFERENCE-001 section 2): live preview
//! hides only marker text, never what the selection touches and never
//! around a selection end, and every source position of every line is
//! drawn somewhere and maps back. Seeded, like `doc_fuzz.rs`.
use common::Rng;
use livemark::doc::Doc;
use livemark::layout::{Affinity, Line};
use livemark::style::Styled;

mod common;

const SEEDS: u64 = 200;

/// Markdown in small pieces: delimiters, escapes, entities, multi-byte
/// text and every line ending.
const PIECES: &[&str] = &[
    "# ", "## ", "###### ", "#", " #", " ##", "**", "*", "_", "__", "~~", "~", "`", "``", "\\",
    "word", "x", " ", "\t", "é", "😀", "&amp;", "\n", "\r\n", "\r", "===", "---", "> ", "- ",
    "1. ", "[a](b)", "www.", "https://", "@", ".com", "(", ")",
];

#[test]
fn random_documents_project_and_map_back() {
    for seed in common::seeds(SEEDS) {
        let mut rng = Rng(seed);
        let text: String = (0..rng.below(80)).map(|_| *rng.pick(PIECES)).collect();
        let doc = Doc::new(text);
        let styled = Styled::new(doc.text());
        for round in 0..8 {
            let (a, b) = (rng.boundary(doc.text()), rng.boundary(doc.text()));
            let selection = a.min(b)..a.max(b);
            let at = format!("seed {seed} round {round} selection {selection:?}");
            check(&doc, &styled, selection, &at);
        }
    }
}

#[test]
fn the_coverage_document_projects_and_maps_back() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/coverage.md"
    ))
    .unwrap();
    let doc = Doc::new(text);
    let styled = Styled::new(doc.text());
    let mut rng = Rng(7);
    for round in 0..50 {
        let at = rng.boundary(doc.text());
        check(&doc, &styled, at..at, &format!("caret {at} round {round}"));
    }
}

/// The user's own notes, exported into a directory named by
/// `LIVEMARK_EXTRA_FIXTURES` (never committed; skipped when unset): each
/// `.md` file under it loads byte for byte, styles, and projects and maps
/// back at random carets and selections.
#[test]
fn extra_fixtures_project_and_map_back() {
    let Ok(dir) = std::env::var("LIVEMARK_EXTRA_FIXTURES") else {
        return;
    };
    let mut files = Vec::new();
    markdown_files(std::path::Path::new(&dir), &mut files);
    assert!(!files.is_empty(), "no .md files under {dir}");
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        let Ok(text) = String::from_utf8(bytes.clone()) else {
            eprintln!(
                "not UTF-8, skipped (the app refuses it too): {}",
                path.display()
            );
            continue;
        };
        let doc = Doc::new(text);
        assert_eq!(doc.text().as_bytes(), bytes, "{}", path.display());
        let styled = Styled::new(doc.text());
        let mut rng = Rng(bytes.len() as u64);
        for round in 0..20 {
            let (a, b) = (rng.boundary(doc.text()), rng.boundary(doc.text()));
            let selection = if round % 2 == 0 {
                a..a
            } else {
                a.min(b)..a.max(b)
            };
            let at = format!("{} round {round} selection {selection:?}", path.display());
            check(&doc, &styled, selection, &at);
        }
    }
    eprintln!("{} files checked", files.len());
}

fn markdown_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "md") {
            files.push(path);
        }
    }
}

fn check(doc: &Doc, styled: &Styled, selection: std::ops::Range<usize>, at: &str) {
    let text = doc.text();
    let hidden = styled.hidden(selection.clone());
    for pair in hidden.windows(2) {
        assert!(pair[0].end < pair[1].start, "{at}: not sorted and merged");
    }
    // Only marker text is ever hidden.
    for h in &hidden {
        assert!(text.is_char_boundary(h.start) && text.is_char_boundary(h.end));
        let first = styled.runs().partition_point(|(r, _)| r.end <= h.start);
        let mut covered = h.start;
        for (r, style) in &styled.runs()[first..] {
            if r.start > covered || covered >= h.end {
                break;
            }
            assert!(style.marker, "{at}: hidden {h:?} is not marker text");
            covered = r.end;
        }
        assert!(covered >= h.end, "{at}: hidden {h:?} is not marker text");
    }
    // A selection end is never inside hidden text.
    for end in [selection.start, selection.end] {
        assert!(
            hidden.iter().all(|h| end <= h.start || h.end <= end),
            "{at}: {end} inside {hidden:?}"
        );
    }
    // What the selection touches shows its markers.
    for c in styled.constructs() {
        let group = &styled.constructs()[c.group].range;
        if selection.start <= group.end && group.start <= selection.end {
            for m in &c.markers {
                assert!(
                    hidden.iter().all(|h| h.end <= m.start || m.end <= h.start),
                    "{at}: touched {c:?} has a hidden marker"
                );
            }
        }
    }
    // Every line: the drawn text is the source minus hidden text, and every
    // position maps across and back.
    for i in 0..doc.line_count() {
        let range = doc.line_range(i);
        let line = Line::new(text, range.clone(), &hidden, styled.runs());
        let visible: String = (range.start..range.end)
            .filter(|&p| text.is_char_boundary(p))
            .filter(|&p| hidden.iter().all(|h| p < h.start || h.end <= p))
            .map(|p| text[p..].chars().next().unwrap())
            .collect();
        assert_eq!(line.text, visible, "{at}: line {i}");
        let inside = |p: usize| hidden.iter().any(|h| h.start < p && p < h.end);
        let mut last = 0;
        for s in (range.start..=range.end).filter(|&s| text.is_char_boundary(s)) {
            let d = line.to_display(s);
            assert!(d >= last, "{at}: line {i} maps {s} backwards");
            last = d;
            if !inside(s) {
                let back = [Affinity::Before, Affinity::After].map(|a| line.to_source(d, a));
                assert!(back.contains(&s), "{at}: line {i}: {s} -> {d} -> {back:?}");
            }
        }
        for d in (0..=line.text.len()).filter(|&d| line.text.is_char_boundary(d)) {
            let before = line.to_source(d, Affinity::Before);
            let after = line.to_source(d, Affinity::After);
            assert!(before <= after, "{at}: line {i} display {d}");
            for s in [before, after] {
                assert!(
                    range.contains(&s) || s == range.end,
                    "{at}: {s} off line {i}"
                );
                assert!(
                    !inside(s),
                    "{at}: line {i} display {d} -> {s} inside hidden"
                );
                assert_eq!(line.to_display(s), d, "{at}: line {i} display {d} -> {s}");
            }
        }
        for (r, _) in &line.runs {
            assert!(r.end <= line.text.len() && line.text.is_char_boundary(r.start));
        }
    }
}
