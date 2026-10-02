//! The app's icons (PLAN-006) as canvas outlines drawn in a 20 px box and
//! scaled, as the library draws its toolbar's: no image files, crisp at
//! any scale, in a tone of the theme.
use std::f32::consts::{FRAC_PI_2, PI};

use iced::widget::canvas::{self, Frame, LineCap, LineJoin, Path, Stroke, path::Arc};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};

/// What an icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// The sidebar, shown or hidden.
    Panel,
    Chevron,
    Plus,
    Minus,
    Search,
    Check,
    Folder,
    Vault,
    Clock,
    Save,
    Quit,
    Bin,
    Link,
    Copy,
    Pen,
    Doc,
    Palette,
}

/// The color an icon is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// The text's color, quieter: most icons.
    Quiet,
    Danger,
}

/// `kind` drawn `size` pixels square in `tone`.
pub fn icon<'a, M: 'a>(kind: Icon, size: f32, tone: Tone) -> Element<'a, M> {
    canvas::Canvas::new(Glyph { kind, tone })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

struct Glyph {
    kind: Icon,
    tone: Tone,
}

impl<M> canvas::Program<M> for Glyph {
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
        let color = match self.tone {
            Tone::Quiet => palette.background.base.text.scale_alpha(0.62),
            Tone::Danger => palette.danger.base.color,
        };
        let mut frame = Frame::new(renderer, bounds.size());
        frame.scale(bounds.width / 20.0);
        let stroke = |width: f32| {
            Stroke::default()
                .with_color(color)
                .with_width(width)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round)
        };
        let thin = stroke(1.5);
        let line = |points: &[(f32, f32)]| {
            Path::new(|path| {
                path.move_to(Point::new(points[0].0, points[0].1));
                for &(x, y) in &points[1..] {
                    path.line_to(Point::new(x, y));
                }
            })
        };
        let rounded = |x: f32, y: f32, w: f32, h: f32, r: f32| {
            Path::rounded_rectangle(Point::new(x, y), Size::new(w, h), r.into())
        };
        let circle = |x: f32, y: f32, r: f32| Path::circle(Point::new(x, y), r);
        let arc = |x: f32, y: f32, r: f32, from: f32, to: f32| {
            Path::new(|path| {
                path.arc(Arc {
                    center: Point::new(x, y),
                    radius: r,
                    start_angle: from.into(),
                    end_angle: to.into(),
                });
            })
        };
        let mut s = |path: Path| frame.stroke(&path, thin);
        match self.kind {
            Icon::Panel => {
                s(rounded(2.5, 3.5, 15.0, 13.0, 2.0));
                s(line(&[(7.5, 3.5), (7.5, 16.5)]));
            }
            Icon::Chevron => {
                frame.stroke(&line(&[(6.0, 8.0), (10.0, 12.0), (14.0, 8.0)]), stroke(2.0))
            }
            Icon::Plus => {
                frame.stroke(&line(&[(10.0, 4.0), (10.0, 16.0)]), stroke(2.0));
                frame.stroke(&line(&[(4.0, 10.0), (16.0, 10.0)]), stroke(2.0));
            }
            Icon::Minus => frame.stroke(&line(&[(4.0, 10.0), (16.0, 10.0)]), stroke(2.0)),
            Icon::Search => {
                s(circle(8.5, 8.5, 5.0));
                s(line(&[(12.5, 12.5), (16.5, 16.5)]));
            }
            Icon::Check => {
                frame.stroke(&line(&[(4.5, 10.5), (8.0, 14.0), (15.5, 6.0)]), stroke(2.0))
            }
            Icon::Folder => {
                s(line(&[
                    (2.5, 7.0),
                    (2.5, 5.0),
                    (3.5, 4.0),
                    (7.5, 4.0),
                    (9.5, 6.0),
                ]));
                s(rounded(2.5, 6.0, 15.0, 10.5, 2.0));
            }
            Icon::Vault => {
                s(rounded(3.0, 3.0, 14.0, 14.0, 2.5));
                s(circle(10.0, 10.0, 3.0));
            }
            Icon::Clock => {
                s(circle(10.0, 10.0, 7.5));
                s(line(&[(10.0, 6.0), (10.0, 10.5), (13.0, 12.0)]));
            }
            Icon::Save => {
                s(rounded(3.5, 3.5, 13.0, 13.0, 1.5));
                s(line(&[(7.0, 3.5), (7.0, 7.5), (13.0, 7.5), (13.0, 3.5)]));
                s(line(&[
                    (6.5, 16.5),
                    (6.5, 12.0),
                    (13.5, 12.0),
                    (13.5, 16.5),
                ]));
            }
            Icon::Palette => {
                s(arc(10.0, 10.0, 7.5, PI * 0.25, PI * 2.0));
                s(line(&[(15.3, 15.3), (13.0, 13.0), (12.0, 13.0)]));
                for (x, y) in [(6.5, 9.0), (9.0, 6.0), (13.0, 6.5)] {
                    frame.fill(&circle(x, y, 1.0), color);
                }
            }
            Icon::Doc => {
                s(rounded(4.0, 2.5, 12.0, 15.0, 2.0));
                s(line(&[(7.0, 6.5), (13.0, 6.5)]));
                s(line(&[(7.0, 10.0), (13.0, 10.0)]));
                s(line(&[(7.0, 13.5), (10.0, 13.5)]));
            }
            Icon::Pen => s(line(&[
                (13.5, 3.5),
                (16.5, 6.5),
                (7.5, 15.5),
                (4.5, 15.5),
                (4.5, 12.5),
                (13.5, 3.5),
            ])),
            Icon::Copy => {
                s(rounded(6.5, 6.5, 10.0, 11.0, 2.0));
                s(line(&[(3.5, 13.5), (3.5, 3.5), (12.5, 3.5)]));
            }
            Icon::Link => {
                s(arc(6.5, 10.0, 4.0, FRAC_PI_2, 3.0 * FRAC_PI_2));
                s(arc(13.5, 10.0, 4.0, -FRAC_PI_2, FRAC_PI_2));
                s(line(&[(6.5, 6.0), (8.0, 6.0)]));
                s(line(&[(6.5, 14.0), (8.0, 14.0)]));
                s(line(&[(13.5, 6.0), (12.0, 6.0)]));
                s(line(&[(13.5, 14.0), (12.0, 14.0)]));
                s(line(&[(7.0, 10.0), (13.0, 10.0)]));
            }
            Icon::Bin => {
                s(line(&[(4.0, 6.0), (16.0, 6.0)]));
                s(line(&[(8.0, 6.0), (8.0, 4.0), (12.0, 4.0), (12.0, 6.0)]));
                s(line(&[(6.0, 6.0), (7.0, 16.0), (13.0, 16.0), (14.0, 6.0)]));
            }
            Icon::Quit => {
                s(line(&[(10.0, 3.0), (10.0, 9.0)]));
                s(arc(
                    10.0,
                    11.0,
                    6.0,
                    -FRAC_PI_2 + 0.75,
                    3.0 * FRAC_PI_2 - 0.75,
                ));
            }
        }
        vec![frame.into_geometry()]
    }
}
