//! The block-level parts of the walk: code blocks (their lines, fences and
//! container prefixes), quote markers, and tables as grid rows.
use std::ops::Range;

use pulldown_cmark::CodeBlockKind;

use super::{Walk, gaps, line_trim};
use crate::style::sweep::Flag;
use crate::style::{CodeBlock, Mark, MarkKind};

impl Walk<'_> {
    pub(super) fn code_start(&mut self, kind: CodeBlockKind, range: Range<usize>) {
        let end = range.start + line_trim(&self.text[range.clone()]);
        let (language, indent) = match kind {
            CodeBlockKind::Fenced(info) => {
                let language = info.split_whitespace().next().unwrap_or("");
                (Some(language.to_owned()), 0)
            }
            // An indented block's range starts after its first line's
            // indentation, which is the block's too.
            CodeBlockKind::Indented => {
                let before = &self.text[..range.start];
                (
                    None,
                    before.len() - before.trim_end_matches([' ', '\t']).len(),
                )
            }
        };
        let start = range.start - indent;
        self.toggles.push((start..end, Flag::CodeBlock));
        let block = CodeBlock {
            range: start..end,
            language,
            lines: Vec::new(),
        };
        self.code = Some((block, Vec::new()));
    }

    pub(super) fn code_end(&mut self) {
        if let Some((mut block, texts)) = self.code.take() {
            // Around a fenced block's code: fences and the prefixes of the
            // container it sits in.
            if block.language.is_some() {
                for gap in gaps(block.range.clone(), &texts) {
                    self.toggles.push((gap.clone(), Flag::Marker));
                    // The fences among them: a run of ``` or ~~~ after
                    // any container prefix, to the line's end.
                    for line in lines_of(self.text, gap) {
                        let text = &self.text[line.clone()];
                        let fence = text.trim_start_matches(['>', ' ', '\t']);
                        if fence.starts_with("```") || fence.starts_with("~~~") {
                            self.marks.push(Mark {
                                range: line.end - fence.len()..line.end,
                                touch: block.range.clone(),
                                kind: MarkKind::Fence,
                            });
                        }
                    }
                }
            }
            // In a container, pulldown-cmark ends a CRLF line with a text
            // event of its own after skipping the `\r`: join them, or the
            // ending would make a line.
            let mut joined: Vec<Range<usize>> = Vec::with_capacity(texts.len());
            for text in texts {
                match joined.last_mut() {
                    Some(last)
                        if text.start == last.end + 1
                            && self.text[last.end..].starts_with("\r\n") =>
                    {
                        last.end = text.end;
                    }
                    _ => joined.push(text),
                }
            }
            block.lines = joined
                .into_iter()
                .flat_map(|t| lines_of(self.text, t))
                .collect();
            self.code_blocks.push(block);
        }
    }

    /// Each line of a quote: its `>` markers (nested ones too, never inside
    /// text) dimmed, and its wrapped rows hanging after them.
    pub(super) fn quote_markers(&mut self, quote: Range<usize>) {
        let bytes = self.text.as_bytes();
        let mut line = quote.start;
        // The last line with a `>`, which a lazy line after it continues.
        let mut quoted: Option<Range<usize>> = None;
        loop {
            let mut at = line;
            let mut hang = None;
            loop {
                while matches!(bytes.get(at), Some(b' ' | b'\t')) {
                    at += 1;
                }
                if bytes.get(at) != Some(&b'>') || self.in_text(at) {
                    break;
                }
                self.toggles.push((at..at + 1, Flag::Marker));
                self.mark(at..at + 1, MarkKind::Quote);
                at += 1;
                if bytes.get(at) == Some(&b' ') {
                    at += 1;
                }
                hang = Some(at);
            }
            self.hangs.extend(hang);
            let rest = &self.text[line..quote.end];
            let end = line + rest.find(['\n', '\r']).unwrap_or(rest.len());
            if hang.is_some() {
                quoted = Some(line..end);
            } else if let Some(quoted) = &quoted
                && !self.text[line..end].trim().is_empty()
            {
                self.lazies.push((line..end, quoted.clone()));
            }
            // The next line, after any line ending (a lone `\r` too).
            match self.text[line..quote.end].find(['\n', '\r']) {
                Some(i) if self.text[line + i..].starts_with("\r\n") => line += i + 2,
                Some(i) => line += i + 1,
                None => break,
            }
        }
    }

    /// The last table's rows as grid rows, and its delimiter row (the line
    /// after the header, after any container prefix) as the rule under
    /// the header; the whole table is what a selection touches.
    pub(super) fn table_marks(&mut self) {
        let index = self.tables.len() - 1;
        self.tables[index].markers.sort_by_key(|m| m.start);
        let table = &self.tables[index];
        let touch = table.range.clone();
        let mut marks: Vec<Mark> = table
            .rows
            .iter()
            .enumerate()
            .map(|(row, (range, _))| Mark {
                range: range.clone(),
                touch: touch.clone(),
                kind: MarkKind::TableRow(index, row),
            })
            .collect();
        if let Some((head, _)) = table.rows.first() {
            let rest = &self.text[head.end..touch.end];
            let after = match rest.find(['\n', '\r']) {
                Some(i) if rest[i..].starts_with("\r\n") => i + 2,
                Some(i) => i + 1,
                None => rest.len(),
            };
            let line = head.end + after;
            let next = &self.text[line..touch.end];
            let len = next.find(['\n', '\r']).unwrap_or(next.len());
            let rule = next[..len].trim_start_matches(['>', ' ', '\t']);
            let range = line + len - rule.len()..line + len;
            self.toggles.push((range.clone(), Flag::Table));
            marks.push(Mark {
                range,
                touch,
                kind: MarkKind::TableRule(index),
            });
        }
        self.marks.extend(marks);
    }
    /// A task box at `task`: the checkbox stands in for its item's bullet
    /// (REFERENCE-001 section 8). While the box is not touched the bullet,
    /// the spaces up to the box and its opening bracket are hidden and no
    /// dot is drawn: the checkbox, drawn over the rest of the box, sits in
    /// the bullet's place.
    /// Touching any of `- [ ]` shows it all.
    pub(super) fn task_box(&mut self, task: &Range<usize>) {
        let line = self.text[..task.start]
            .rfind(['\n', '\r'])
            .map_or(0, |i| i + 1);
        let bullet = self
            .marks
            .iter()
            .rposition(|m| {
                matches!(m.kind, MarkKind::Bullet)
                    && line <= m.range.start
                    && m.range.end <= task.start
            })
            .map(|i| self.marks.remove(i).range);
        let touch = bullet.as_ref().map_or(task.clone(), |b| b.start..task.end);
        if let Some(mark) = self.marks.last_mut().filter(|m| m.range == *task) {
            mark.touch = touch.clone();
        }
        if let Some(bullet) = bullet {
            self.toggles.push((bullet.start..task.start, Flag::Marker));
            self.task_bullets
                .push((bullet.start..task.start, touch.clone()));
        }
        // The opening bracket too; the closing one stays laid out, the
        // checkbox's room before the text.
        if task.len() == 3 {
            self.task_bullets.push((task.start..task.start + 1, touch));
        }
    }

    /// The line holding `at`, text directly in a list item: when it is
    /// not the item's first line, its text lines up with the item's, at
    /// `hang` (REFERENCE-001 section 7), after the markers of the quotes
    /// the item is in.
    pub(super) fn continuation_line(&mut self, at: usize, hang: usize) {
        // Each line is looked at once: a long line holds many events.
        if self
            .scanned
            .as_ref()
            .is_some_and(|l| l.start <= at && at <= l.end)
        {
            return;
        }
        let start = self.text[..at].rfind(['\n', '\r']).map_or(0, |i| i + 1);
        let rest = &self.text[start..];
        let end = start + rest.find(['\n', '\r']).unwrap_or(rest.len());
        self.scanned = Some(start..end);
        if !(start..=end).contains(&hang) {
            self.continuations.push((start..end, hang));
        }
    }
}

/// The lines of `range` in `text`, each without its line ending (`\n`,
/// `\r\n` or a lone `\r`, as in `Doc`).
fn lines_of(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let (mut start, mut at) = (range.start, range.start);
    while at < range.end {
        match bytes[at] {
            b'\n' => {
                lines.push(start..at);
                at += 1;
                start = at;
            }
            b'\r' => {
                lines.push(start..at);
                let crlf = at + 1 < range.end && bytes[at + 1] == b'\n';
                at += if crlf { 2 } else { 1 };
                start = at;
            }
            _ => at += 1,
        }
    }
    if start < range.end {
        lines.push(start..range.end);
    }
    lines
}
