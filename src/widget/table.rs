//! Tables drawn as grids while the selection is outside them (REFERENCE-001
//! section 10). Each row's source stays laid out but transparent (a
//! concealed mark) and as tall as its tallest cell; its cells are shaped
//! on their own in the prose font, the header bold, wrapped to columns
//! sized by `fit`, and placed by the delimiter row's alignment. The
//! delimiter row's line is shaped short and drawn as the rule under the
//! header.
use std::ops::Range;
use std::sync::Arc;

use iced::advanced::graphics::text::Renderer as _;
use iced::advanced::graphics::text::{Raw, cosmic_text};
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Border, Color, Point, Rectangle, Size, Vector};

use super::shape::{Colors, shape};
use crate::layout::{Affinity, Line};
use crate::style::{Align, Style, Styled, Table};

/// Space between a cell's border and its text, at 100%.
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
    /// Each row's height: its tallest cell's.
    pub heights: Vec<f32>,
    /// Space between a cell's border and its text, with the zoom.
    pad: f32,
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
    pub fn new(
        text: &str,
        styled: &Styled,
        table: &Table,
        (colors, zoom): (Colors, f32),
        width: f32,
    ) -> Self {
        let pad = PAD * zoom;
        let shaped = |line: Line, room: f32| {
            let cached = shape(&line, (0, false, colors, false, zoom), None, room, &[]);
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
        };
        // Each cell on one row first, for the columns' widths.
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
                        shaped(Line::new(text, cell, &table.markers, &runs), f32::MAX)
                    })
                    .collect()
            })
            .collect();
        let widest = |measure: &dyn Fn(&Cell) -> f32| -> Vec<f32> {
            (0..table.align.len())
                .map(|c| {
                    let widest = cells.iter().map(|row| measure(&row[c])).fold(0.0, f32::max);
                    (widest + 2.0 * pad).max(MIN_COLUMN)
                })
                .collect()
        };
        let narrowest = widest(&|cell| widest_word(&cell.buffer));
        let natural = widest(&|cell| cell.size.width);
        let widths = fit(&narrowest, &natural, width);
        // Cells wider than their column wrap (half a pixel over is only
        // rounding, which would wrap a cell's last word).
        let cells: Vec<Vec<Cell>> = cells
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .zip(&widths)
                    .map(|(cell, w)| {
                        let room = w - 2.0 * pad;
                        if cell.size.width > room + 0.5 {
                            shaped(cell.line, room)
                        } else {
                            cell
                        }
                    })
                    .collect()
            })
            .collect();
        let heights = cells
            .iter()
            .map(|row| row.iter().map(|cell| cell.size.height).fold(0.0, f32::max))
            .collect();
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
            heights,
            pad,
        }
    }

    /// The whole grid's width.
    pub fn width(&self) -> f32 {
        self.columns.last().map_or(0.0, |(x, w)| x + w)
    }

    /// Where cell `row`, `column`'s text starts, from the row's start.
    fn text_x(&self, row: usize, column: usize) -> f32 {
        let (x, width) = self.columns[column];
        let room = (width - 2.0 * self.pad - self.cells[row][column].size.width).max(0.0);
        x + self.pad
            + match self.align[column] {
                Align::Right => room,
                Align::Center => room / 2.0,
                Align::None | Align::Left => 0.0,
            }
    }

    /// Where cell `row`, `column`'s text starts, from the row's top: the
    /// cell centred in its row.
    fn text_y(&self, row: usize, column: usize) -> f32 {
        (self.heights[row] - self.cells[row][column].size.height) / 2.0
    }

    /// Whether `x`, `y` (from the row's top left) is on the text of a cell
    /// of row `row`.
    pub fn covers(&self, row: usize, x: f32, y: f32) -> bool {
        (0..self.columns.len()).any(|column| {
            let at = Point::new(self.text_x(row, column), self.text_y(row, column));
            Rectangle::new(at, self.cells[row][column].size).contains(Point::new(x, y))
        })
    }

    /// The source offset under `x`, `y` (from the row's top left) in row
    /// `row` of `table`, a table with this grid's text: above or below a
    /// cell's text, its first or last row there.
    pub fn hit(&self, table: &Table, row: usize, x: f32, y: f32) -> usize {
        let column = self
            .columns
            .iter()
            .position(|(left, width)| x < left + width)
            .unwrap_or(self.columns.len() - 1);
        let cell = &self.cells[row][column];
        let y = (y - self.text_y(row, column)).clamp(0.0, (cell.size.height - 1.0).max(0.0));
        let display = cell
            .buffer
            .hit(x - self.text_x(row, column), y)
            .map_or(0, |cursor| cursor.index);
        let (range, cells) = &table.rows[row];
        let start = cell_range(range, cells, column).start;
        cell.line.to_source(display, Affinity::After) - cell.start + start
    }

    /// Draws row `row` with its top left at `at`, clipped to `clip`.
    pub fn draw_row(
        &self,
        renderer: &mut iced::Renderer,
        row: usize,
        at: Point,
        clip: Rectangle,
        (text, line, header): (Color, Color, Color),
    ) {
        for (column, &(x, width)) in self.columns.iter().enumerate() {
            let bounds = Rectangle::new(
                at + Vector::new(x, 0.0),
                Size::new(width, self.heights[row]),
            );
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
            renderer.fill_raw(Raw {
                buffer: Arc::downgrade(&cell.buffer),
                position: at + Vector::new(self.text_x(row, column), self.text_y(row, column)),
                color: text,
                clip_bounds: clip,
            });
        }
    }
}

/// Column widths from each column's `narrowest` (its widest word) and
/// `natural` (its widest cell on one row) widths, to fit `width`: the
/// natural widths when they fit; else each column its narrowest and a share
/// of the room left by how much wider it wants to be, so a column of short
/// words keeps them whole and long text wraps (as browsers lay out tables,
/// CSS 2.1 section 17.5.2.2); else the narrowest shrunk together, long
/// words broken (REFERENCE-001 section 10).
pub(super) fn fit(narrowest: &[f32], natural: &[f32], width: f32) -> Vec<f32> {
    let (least, most): (f32, f32) = (narrowest.iter().sum(), natural.iter().sum());
    if most <= width {
        natural.to_vec()
    } else if least <= width {
        let share = (width - least) / (most - least);
        narrowest
            .iter()
            .zip(natural)
            .map(|(low, high)| low + (high - low) * share)
            .collect()
    } else {
        narrowest
            .iter()
            .map(|w| (w * width / least).max(MIN_COLUMN))
            .collect()
    }
}

/// The widest word of `buffer`, shaped on one row: the narrowest its text
/// wraps to without breaking a word.
fn widest_word(buffer: &cosmic_text::Buffer) -> f32 {
    let mut widest: f32 = 0.0;
    for run in buffer.layout_runs() {
        let mut left = None;
        for glyph in run.glyphs {
            if run.text[glyph.start..glyph.end].trim().is_empty() {
                left = None;
            } else {
                let start = *left.get_or_insert(glyph.x);
                widest = widest.max(glyph.x + glyph.w - start);
            }
        }
    }
    widest
}

/// Cell `column` of the row at `range`: an empty range at the row's end
/// when the row has fewer cells.
fn cell_range(range: &Range<usize>, cells: &[Range<usize>], column: usize) -> Range<usize> {
    cells.get(column).cloned().unwrap_or(range.end..range.end)
}
