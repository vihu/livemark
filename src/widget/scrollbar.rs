//! The scroll bar in the right padding: a thumb as long as the share of
//! lines in view, placed by the first line drawn. It counts lines, not
//! pixels: lines off screen have no height until they are shaped.
use iced::advanced::renderer::{self, Renderer as _};
use iced::{Point, Rectangle, Size, Theme, border};

use super::{Editor, Pane};

/// The thumb's width.
pub(super) const WIDTH: f32 = 6.0;

/// The shortest thumb, so a long document still has one to grab.
const MIN_THUMB: f32 = 24.0;

/// The track along the right edge of the widget's `bounds`.
pub(super) fn track(bounds: Rectangle) -> Rectangle {
    Rectangle::new(
        Point::new(bounds.x + bounds.width - WIDTH - 4.0, bounds.y + 4.0),
        Size::new(WIDTH, (bounds.height - 8.0).max(0.0)),
    )
}

impl Editor {
    /// The thumb of `pane` in `track`; none when the whole document is in
    /// view.
    pub(super) fn thumb(&self, pane: Pane, track: Rectangle) -> Option<Rectangle> {
        let (start, visible, count) = self.with_pane(pane, |lines, source| {
            let count = source.doc.line_count();
            let start = lines.position(source, (lines.anchor, lines.offset));
            (start, lines.visible, count)
        });
        if count <= 1 || (start == 0.0 && visible >= count) {
            return None;
        }
        let share = (visible as f32 / count as f32).min(1.0);
        let height = (share * track.height).clamp(MIN_THUMB.min(track.height), track.height);
        let t = start / travel_lines(count, visible);
        let y = track.y + t.clamp(0.0, 1.0) * (track.height - height);
        Some(Rectangle::new(
            Point::new(track.x, y),
            Size::new(track.width, height),
        ))
    }

    /// Draws the thumb of `pane`, faint, in the widget's `bounds`.
    pub(super) fn draw_scrollbar(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        pane: Pane,
        bounds: Rectangle,
    ) {
        if let Some(thumb) = self.thumb(pane, track(bounds)) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: thumb,
                    border: border::rounded(WIDTH / 2.0),
                    ..renderer::Quad::default()
                },
                theme.palette().background.strong.color,
            );
        }
    }

    /// Scrolls `pane` so the thumb's top is at `t` of the way down its
    /// travel.
    pub(super) fn scroll_to(&mut self, pane: Pane, t: f32) {
        self.with_pane(pane, |lines, source| {
            let count = source.doc.line_count();
            // At the bottom, the end exactly: the travel counts lines.
            if t >= 1.0 {
                (lines.anchor, lines.offset) = lines.end(source);
                return;
            }
            let position = t.clamp(0.0, 1.0) * travel_lines(count, lines.visible);
            lines.anchor = (position.floor() as usize).min(count - 1);
            let height = lines.shaped(source, lines.anchor).height;
            lines.offset = position.fract() * height;
            lines.stop_at_end(source);
        });
    }
}

/// How many lines the view's top travels over, by line counts: up to the
/// last screen of lines (the frame before last drew `visible`).
fn travel_lines(count: usize, visible: usize) -> f32 {
    count.saturating_sub(visible).max(1) as f32
}

/// The thumb's travel for a top at `y`, from 0 to 1.
pub(super) fn travel(track: Rectangle, thumb: Rectangle, y: f32) -> f32 {
    let room = (track.height - thumb.height).max(1.0);
    ((y - track.y) / room).clamp(0.0, 1.0)
}
