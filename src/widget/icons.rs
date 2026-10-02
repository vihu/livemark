//! The toolbar's icons (PLAN-003) as canvas outlines in a 20 px box, as
//! roughdraft draws its own: no image files, crisp at any zoom of the
//! screen, in the color of the text around them.
use std::f32::consts::FRAC_PI_2;

use iced::widget::canvas::{self, Frame, LineCap, LineJoin, Path, Stroke, path::Arc};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};

use super::Message;

/// Icon box in pixels.
const SIZE: f32 = 20.0;

/// What an icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyph {
    Bold,
    Italic,
    Code,
    Link,
    Heading,
    List,
    Task,
    Quote,
    /// Live preview: an eye.
    Live,
    /// The markdown as written: the markdown mark, M and an arrow down.
    Markdown,
    /// Side by side: two columns.
    Split,
}

/// An icon in a quieter shade of the text's color, or in the text's own
/// when `on`.
pub(super) fn icon<'a>(glyph: Glyph, on: bool) -> Element<'a, Message> {
    canvas::Canvas::new(Icon { glyph, on })
        .width(Length::Fixed(SIZE))
        .height(Length::Fixed(SIZE))
        .into()
}

struct Icon {
    glyph: Glyph,
    on: bool,
}

impl canvas::Program<Message> for Icon {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let palette = theme.palette();
        // The mode shown in the text's color, the rest quieter.
        let color = if self.on {
            palette.background.base.text
        } else {
            palette.background.base.text.scale_alpha(0.62)
        };
        let mut frame = Frame::new(renderer, bounds.size());
        let stroke = |width: f32| {
            Stroke::default()
                .with_color(color)
                .with_width(width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round)
        };
        let line = |points: &[(f32, f32)]| {
            Path::new(|path| {
                path.move_to(Point::new(points[0].0, points[0].1));
                for &(x, y) in &points[1..] {
                    path.line_to(Point::new(x, y));
                }
            })
        };
        // A half circle on the right of `center`, top to bottom.
        let bowl = |path: &mut canvas::path::Builder, center: (f32, f32), radius: f32| {
            path.arc(Arc {
                center: Point::new(center.0, center.1),
                radius,
                start_angle: (-FRAC_PI_2).into(),
                end_angle: FRAC_PI_2.into(),
            });
        };
        let rounded = |x: f32, y: f32, w: f32, h: f32, r: f32| {
            Path::rounded_rectangle(Point::new(x, y), Size::new(w, h), r.into())
        };
        let dot = |x: f32, y: f32, r: f32| Path::circle(Point::new(x, y), r);
        let thin = stroke(1.5);
        match self.glyph {
            Glyph::Bold => {
                let b = Path::new(|path| {
                    path.move_to(Point::new(10.5, 3.5));
                    path.line_to(Point::new(5.5, 3.5));
                    path.line_to(Point::new(5.5, 16.5));
                    path.line_to(Point::new(11.5, 16.5));
                    path.move_to(Point::new(5.5, 9.75));
                    path.line_to(Point::new(11.5, 9.75));
                    bowl(path, (10.5, 6.625), 3.125);
                    bowl(path, (11.5, 13.125), 3.375);
                });
                frame.stroke(&b, stroke(2.2));
            }
            Glyph::Italic => {
                frame.stroke(&line(&[(9.0, 3.5), (15.0, 3.5)]), thin);
                frame.stroke(&line(&[(5.0, 16.5), (11.0, 16.5)]), thin);
                frame.stroke(&line(&[(12.0, 3.5), (8.0, 16.5)]), thin);
            }
            Glyph::Code => {
                frame.stroke(&line(&[(7.0, 5.5), (2.5, 10.0), (7.0, 14.5)]), thin);
                frame.stroke(&line(&[(13.0, 5.5), (17.5, 10.0), (13.0, 14.5)]), thin);
            }
            Glyph::Link => {
                // Two half rings and the bar between them.
                let left = Path::new(|path| {
                    path.arc(Arc {
                        center: Point::new(6.5, 10.0),
                        radius: 4.0,
                        start_angle: FRAC_PI_2.into(),
                        end_angle: (3.0 * FRAC_PI_2).into(),
                    });
                    path.line_to(Point::new(8.0, 6.0));
                    path.move_to(Point::new(6.5, 14.0));
                    path.line_to(Point::new(8.0, 14.0));
                });
                let right = Path::new(|path| {
                    bowl(path, (13.5, 10.0), 4.0);
                    path.move_to(Point::new(13.5, 6.0));
                    path.line_to(Point::new(12.0, 6.0));
                    path.move_to(Point::new(13.5, 14.0));
                    path.line_to(Point::new(12.0, 14.0));
                });
                frame.stroke(&left, thin);
                frame.stroke(&right, thin);
                frame.stroke(&line(&[(7.0, 10.0), (13.0, 10.0)]), thin);
            }
            Glyph::Heading => {
                frame.stroke(&line(&[(5.0, 3.5), (5.0, 16.5)]), stroke(1.8));
                frame.stroke(&line(&[(15.0, 3.5), (15.0, 16.5)]), stroke(1.8));
                frame.stroke(&line(&[(5.0, 10.0), (15.0, 10.0)]), stroke(1.8));
            }
            Glyph::List => {
                for y in [4.5, 10.0, 15.5] {
                    frame.fill(&dot(3.75, y, 1.25), color);
                    frame.stroke(&line(&[(7.5, y), (17.5, y)]), thin);
                }
            }
            Glyph::Task => {
                frame.stroke(&rounded(2.5, 2.5, 6.0, 6.0, 1.5), thin);
                frame.stroke(&line(&[(4.0, 5.6), (5.3, 6.9), (7.6, 4.2)]), thin);
                frame.stroke(&line(&[(11.5, 5.5), (17.5, 5.5)]), thin);
                frame.stroke(&rounded(2.5, 11.5, 6.0, 6.0, 1.5), thin);
                frame.stroke(&line(&[(11.5, 14.5), (17.5, 14.5)]), thin);
            }
            Glyph::Quote => {
                frame.stroke(&line(&[(4.0, 4.0), (4.0, 16.0)]), stroke(2.5));
                for y in [5.5, 10.0, 14.5] {
                    frame.stroke(&line(&[(8.5, y), (17.0, y)]), thin);
                }
            }
            Glyph::Live => {
                let eye = Path::new(|path| {
                    path.move_to(Point::new(1.5, 10.0));
                    path.quadratic_curve_to(Point::new(10.0, 1.0), Point::new(18.5, 10.0));
                    path.quadratic_curve_to(Point::new(10.0, 19.0), Point::new(1.5, 10.0));
                });
                frame.stroke(&eye, thin);
                frame.stroke(&dot(10.0, 10.0, 2.5), thin);
            }
            Glyph::Markdown => {
                frame.stroke(&rounded(1.5, 4.5, 17.0, 11.0, 2.0), thin);
                let m = [
                    (4.5, 12.5),
                    (4.5, 7.5),
                    (7.0, 10.0),
                    (9.5, 7.5),
                    (9.5, 12.5),
                ];
                frame.stroke(&line(&m), thin);
                frame.stroke(&line(&[(14.0, 7.5), (14.0, 12.5)]), thin);
                frame.stroke(&line(&[(12.0, 10.5), (14.0, 12.5), (16.0, 10.5)]), thin);
            }
            Glyph::Split => {
                frame.stroke(&rounded(2.0, 3.5, 16.0, 13.0, 2.0), thin);
                frame.stroke(&line(&[(10.0, 3.5), (10.0, 16.5)]), thin);
            }
        }
        vec![frame.into_geometry()]
    }
}
