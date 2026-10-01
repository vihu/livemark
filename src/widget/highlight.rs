//! Syntax colors for fenced code (REFERENCE-001 section 9): iced's own
//! highlighter (syntect through two-face) run over a block's lines, kept by
//! the block's text and language, and colored by the theme.
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

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

/// A block's token classes, line by line, in offsets from each line's
/// start.
type Classes = Arc<Vec<Vec<(Range<usize>, Code)>>>;

/// Token classes of code blocks by block text and language.
#[derive(Default)]
pub struct Highlights {
    blocks: HashMap<u64, Classes>,
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
        let classes = self.block(text, block, language);
        classes[index]
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

    /// The token classes of `block`'s lines, parsed once per text.
    fn block(&mut self, text: &str, block: &CodeBlock, language: &str) -> Classes {
        let mut hasher = DefaultHasher::new();
        language.hash(&mut hasher);
        for line in &block.lines {
            text[line.clone()].hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some(classes) = self.blocks.get(&key) {
            return classes.clone();
        }
        let mut parser = Parser::new(&Settings {
            token: language.to_owned(),
        });
        let classes: Arc<Vec<Vec<_>>> = Arc::new(
            block
                .lines
                .iter()
                .map(|line| parser.parse_line(&text[line.clone()]).collect())
                .collect(),
        );
        // Every edit inside a block makes a new entry; old ones go at once.
        if self.blocks.len() >= 64 {
            self.blocks.clear();
        }
        self.blocks.insert(key, classes.clone());
        classes
    }
}

#[cfg(test)]
mod tests {
    use iced::{Code, Theme};

    use super::Highlights;
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
}
