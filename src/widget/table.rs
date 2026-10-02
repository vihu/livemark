//! Tables drawn as grids while the selection is outside them (REFERENCE-001
//! section 10). Each row's source stays laid out but transparent (a
//! concealed mark); its cells are shaped on their own in the prose font,
//! the header bold, into columns as wide as their widest cell, shrunk
//! together when the table is wider than the text, and placed by the
//! delimiter row's alignment. The delimiter row's line is shaped short and
//! drawn as the rule under the header.
use std::ops::Range;
use std::sync::Arc;

use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::graphics::text::{Raw, cosmic_text};
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Border, Color, Point, Rectangle, Size, Vector};

use super::shape::{Colors, shape};
use crate::layout::{Affinity, Line};
use crate::style::{Align, Style, Styled, Table};

/// Space between a cell's border and its text.
const PAD: f32 = 8.0;

/// The narrowest a column shrinks to.
const MIN_COLUMN: f32 = 32.0;

/// A table laid out as a grid.
pub struct Grid {
    /// Each column's left edge, from the row's start, and width.
    pub columns: Vec<(f32, f32)>,
    /// Each row's cells, the header row first.
    pub cells: Vec<Vec<Cell>>,
    pub align: Vec<Align>,
}

/// A cell shaped on its own.
pub struct Cell {
    pub buffer: Arc<cosmic_text::Buffer>,
    /// Its text's width and height.
    pub size: Size,
    /// Its source projected, to map clicks back, and where that source
    /// started: a grid is shared by every table with its text, so offsets
    /// are taken relative to this.
    pub line: Line,
    pub start: usize,
}

impl Grid {
    /// `table` of `text` laid out to fit `width`.
    pub fn new(text: &str, styled: &Styled, table: &Table, colors: Colors, width: f32) -> Self {
        let cells: Vec<Vec<Cell>> = table
            .rows
            .iter()
            .enumerate()
            .map(|(row, (range, cells))| {
                (0..table.align.len())
                    .map(|column| {
                        let cell = cell_range(range, cells, column);
                        let runs: Vec<_> = styled
                            .runs()
                            .iter()
                            .filter(|(r, _)| r.start < cell.end && cell.start < r.end)
                            .map(|(r, style)| {
                                let style = Style {
                                    table: false,
                                    strong: style.strong || row == 0,
                                    ..*style
                                };
                                (r.clone(), style)
                            })
                            .collect();
                        let line = Line::new(text, cell, &table.markers, &runs);
                        let cached = shape(&line, (0, false, colors, false), None, f32::MAX, &[]);
                        let width = cached
                            .buffer
                            .layout_runs()
                            .map(|run| run.line_w)
                            .fold(0.0, f32::max);
                        Cell {
                            size: Size::new(width, cached.height),
                            buffer: cached.buffer,
                            start: line.to_source(0, Affinity::Before),
                            line,
                        }
                    })
                    .collect()
            })
            .collect();
        let mut widths: Vec<f32> = (0..table.align.len())
            .map(|c| {
                let widest = cells
                    .iter()
                    .map(|row| row[c].size.width)
                    .fold(0.0, f32::max);
                (widest + 2.0 * PAD).max(MIN_COLUMN)
            })
            .collect();
        let total: f32 = widths.iter().sum();
        if total > width {
            for w in &mut widths {
                *w = (*w * width / total).max(MIN_COLUMN);
            }
        }
        let mut x = 0.0;
        let columns = widths
            .into_iter()
            .map(|w| {
                x += w;
                (x - w, w)
            })
            .collect();
        Self {
            columns,
            cells,
            align: table.align.clone(),
        }
    }

    /// The whole grid's width.
    pub fn width(&self) -> f32 {
        self.columns.last().map_or(0.0, |(x, w)| x + w)
    }

    /// Where cell `row`, `column`'s text starts, from the row's start.
    fn text_x(&self, row: usize, column: usize) -> f32 {
        let (x, width) = self.columns[column];
        let room = (width - 2.0 * PAD - self.cells[row][column].size.width).max(0.0);
        x + PAD
            + match self.align[column] {
                Align::Right => room,
                Align::Center => room / 2.0,
                Align::None | Align::Left => 0.0,
            }
    }

    /// The source offset under `x` (from the row's start) in row `row` of
    /// `table`, a table with this grid's text.
    pub fn hit(&self, table: &Table, row: usize, x: f32) -> usize {
        let column = self
            .columns
            .iter()
            .position(|(left, width)| x < left + width)
            .unwrap_or(self.columns.len() - 1);
        let cell = &self.cells[row][column];
        let local = x - self.text_x(row, column);
        let display = cell
            .buffer
            .hit(local, cell.size.height / 2.0)
            .map_or(0, |cursor| cursor.index);
        let display = if local >= cell.size.width {
            cell.line.text.len()
        } else {
            display
        };
        let (range, cells) = &table.rows[row];
        let start = cell_range(range, cells, column).start;
        cell.line.to_source(display, Affinity::After) - cell.start + start
    }

    /// Draws row `row` with its top left at `at`, `height` tall, clipped
    /// to `clip`.
    pub fn draw_row(
        &self,
        renderer: &mut iced::Renderer,
        row: usize,
        at: Point,
        height: f32,
        clip: Rectangle,
        (text, line, header): (Color, Color, Color),
    ) {
        for (column, &(x, width)) in self.columns.iter().enumerate() {
            let bounds = Rectangle::new(at + Vector::new(x, 0.0), Size::new(width, height));
            if row == 0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        ..renderer::Quad::default()
                    },
                    header,
                );
            }
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        color: line,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..renderer::Quad::default()
                },
                Color::TRANSPARENT,
            );
            let cell = &self.cells[row][column];
            let Some(clip) = bounds.shrink(1.0).intersection(&clip) else {
                continue;
            };
            let y = (height - cell.size.height) / 2.0;
            renderer.fill_raw(Raw {
                buffer: Arc::downgrade(&cell.buffer),
                position: at + Vector::new(self.text_x(row, column), y),
                color: text,
                clip_bounds: clip,
            });
        }
    }
}

/// Cell `column` of the row at `range`: an empty range at the row's end
/// when the row has fewer cells.
fn cell_range(range: &Range<usize>, cells: &[Range<usize>], column: usize) -> Range<usize> {
    cells.get(column).cloned().unwrap_or(range.end..range.end)
}
