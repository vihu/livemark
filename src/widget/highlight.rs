//! Syntax colors for fenced code (REFERENCE-001 section 9): iced's own
//! highlighter (syntect through two-face) run over a block's lines and
//! colored by the theme. Lines are parsed only as far down as they are
//! drawn, and an edited block is parsed again from the snapshot (every 50
//! lines, iced's) before its first changed line: a line's state depends
//! only on the lines above it in the block (backlog 14).
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use iced::highlighter::{Parser, Settings};
use iced::{Code, Color, Theme, font};

use crate::layout::Line;
use crate::style::{CodeBlock, Styled};

/// A colored stretch of a drawn line, in display offsets.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub range: Range<usize>,
    pub color: Color,
    pub italic: bool,
}

/// How many blocks keep their parse; the least recently drawn go first.
const KEEP: usize = 32;

/// A block parsed from its first line down.
struct Parsed {
    language: String,
    /// A hash of each of the block's lines.
    lines: Vec<u64>,
    parser: Parser,
    /// Token classes of the lines parsed so far, in offsets from each
    /// line's start.
    classes: Vec<Vec<(Range<usize>, Code)>>,
}

/// Parsed code blocks, most recently drawn last.
#[derive(Default)]
pub struct Highlights {
    parsed: Vec<Parsed>,
}

impl Highlights {
    /// The token colors of the source line at `range`, drawn as `line`,
    /// when it is code in a block with a language.
    pub fn tokens(
        &mut self,
        text: &str,
        styled: &Styled,
        range: Range<usize>,
        line: &Line,
        theme: &Theme,
    ) -> Vec<Token> {
        let Some(block) = styled.code_block_at(range.clone()) else {
            return Vec::new();
        };
        let Some(language) = block.language.as_deref().filter(|l| !l.is_empty()) else {
            return Vec::new();
        };
        let Some(index) = block
            .lines
            .iter()
            .position(|l| range.start <= l.start && l.end <= range.end)
        else {
            return Vec::new();
        };
        let segment = block.lines[index].start;
        let parsed = self.parsed(text, block, language);
        while parsed.classes.len() <= index {
            let source = &text[block.lines[parsed.classes.len()].clone()];
            parsed
                .classes
                .push(parsed.parser.parse_line(source).collect());
        }
        parsed.classes[index]
            .iter()
            .filter_map(|(r, code)| {
                let style = code.highlight(theme);
                let start = line.to_display(segment + r.start);
                let end = line.to_display(segment + r.end);
                Some(Token {
                    range: start..end,
                    color: style.color?,
                    italic: style.style == Some(font::Style::Italic),
                })
            })
            .collect()
    }

    /// The parse of `block`: one made for the same lines, else the one in
    /// the same language sharing the most first lines with it, rewound to
    /// its snapshot before the first different line, else a new one.
    fn parsed(&mut self, text: &str, block: &CodeBlock, language: &str) -> &mut Parsed {
        let lines: Vec<u64> = block
            .lines
            .iter()
            .map(|line| {
                let mut hasher = DefaultHasher::new();
                text[line.clone()].hash(&mut hasher);
                hasher.finish()
            })
            .collect();
        let shared = |parsed: &Parsed| {
            parsed
                .lines
                .iter()
                .zip(&lines)
                .take_while(|(a, b)| a == b)
                .count()
        };
        let best = self
            .parsed
            .iter()
            .enumerate()
            .filter(|(_, p)| p.language == language)
            .map(|(i, p)| (i, shared(p)))
            .max_by_key(|&(_, n)| n)
            .filter(|&(i, n)| n > 0 || self.parsed[i].lines.is_empty());
        let mut parsed = match best {
            Some((i, n)) => {
                let mut parsed = self.parsed.remove(i);
                if n < parsed.classes.len() {
                    parsed.parser.change_line(n);
                    parsed.classes.truncate(parsed.parser.current_line());
                }
                parsed
            }
            None => {
                if self.parsed.len() >= KEEP {
                    self.parsed.remove(0);
                }
                Parsed {
                    language: language.to_owned(),
                    lines: Vec::new(),
                    parser: Parser::new(&Settings {
                        token: language.to_owned(),
                    }),
                    classes: Vec::new(),
                }
            }
        };
        parsed.lines = lines;
        self.parsed.push(parsed);
        self.parsed.last_mut().expect("just pushed")
    }
}

#[cfg(test)]
mod tests {
    use iced::{Code, Theme};

    use super::{Highlights, Token};
    use crate::layout::Line;
    use crate::style::Styled;

    #[test]
    fn rust_in_a_fence_takes_the_themes_token_colors() {
        let text = "```rust\nfn main() {}\n```\n```\nfn plain() {}\n```\n";
        let styled = Styled::new(text);
        let mut highlights = Highlights::default();
        let tokens = |highlights: &mut Highlights, range: std::ops::Range<usize>| {
            let line = Line::new(text, range.clone(), &[], styled.runs());
            highlights.tokens(text, &styled, range, &line, &Theme::Light)
        };
        let rust = tokens(&mut highlights, 8..20);
        let keyword = Code::Keyword.highlight(&Theme::Light).color.unwrap();
        assert_eq!(
            (rust[0].range.clone(), rust[0].color),
            (0..2, keyword),
            "fn"
        );
        assert!(tokens(&mut highlights, 0..7).is_empty(), "the fence");
        assert!(tokens(&mut highlights, 28..41).is_empty(), "no language");
    }

    /// The colors of every line of `text`, asked for in `order`.
    fn colors(highlights: &mut Highlights, text: &str, order: &[usize]) -> Vec<Vec<Token>> {
        let styled = Styled::new(text);
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let mut colors = vec![Vec::new(); starts.len() - 1];
        for &i in order.iter().filter(|&&i| i + 1 < starts.len()) {
            let range = starts[i]..starts[i + 1] - 1;
            let line = Line::new(text, range.clone(), &[], styled.runs());
            colors[i] = highlights.tokens(text, &styled, range, &line, &Theme::Light);
        }
        colors
    }

    #[test]
    fn an_edited_block_highlights_as_if_parsed_afresh() {
        let body: Vec<String> = (0..130)
            .map(|i| format!("    let x{i} = \"s{i}\"; // n{i}"))
            .collect();
        let block = |body: &[String]| format!("```rust\n{}\n```\n", body.join("\n"));
        let mut highlights = Highlights::default();
        let bottom: Vec<usize> = (100..132).collect();
        let all: Vec<usize> = (0..132).collect();
        // Only the bottom drawn, then edits that change how every line
        // below parses (a comment opened and closed), lines removed and
        // added, each compared with a parse from scratch.
        let mut body = body;
        let edits: [fn(&mut Vec<String>); 6] = [
            |b| b[70].insert_str(0, "/*"),
            |b| b[90].push_str("*/"),
            |b| b[70].replace_range(0..2, ""),
            |b| {
                b.drain(50..60);
            },
            |b| b.insert(3, "fn f() {}".into()),
            |b| b.push("}".into()),
        ];
        let _ = colors(&mut highlights, &block(&body), &bottom);
        for (n, edit) in edits.iter().enumerate() {
            edit(&mut body);
            let text = block(&body);
            let order = if n % 2 == 0 { &bottom } else { &all };
            let fresh = colors(&mut Highlights::default(), &text, order);
            assert_eq!(colors(&mut highlights, &text, order), fresh, "edit {n}");
        }
    }
}
