//! Pictures for images (REFERENCE-001 section 5): decoded from bytes the
//! host supplies (`Editor::set_image`) and drawn under the line of an
//! image whose markdown is hidden, fitted to the text's width.
use iced::widget::image::Handle;
use iced::{Point, Rectangle, Size};

/// A decoded picture and its size in pixels.
#[derive(Clone)]
pub struct Picture {
    pub handle: Handle,
    pub size: Size,
}

/// The tallest a picture is drawn at 100% zoom; larger ones scale down.
pub const MAX_HEIGHT: f32 = 480.0;

/// Space above each picture.
pub const GAP: f32 = 6.0;

/// Decodes an encoded image (PNG, JPEG, GIF, WebP), as roughdraft does;
/// `None` for bytes that are not one.
pub fn decode(bytes: &[u8]) -> Option<Picture> {
    let rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(Picture {
        handle: Handle::from_rgba(width, height, rgba.into_raw()),
        size: Size::new(width as f32, height as f32),
    })
}

/// Where `pictures` go one under another from `top`: each at most `width`
/// wide and `max_height` tall, scaled down only, keeping its shape. The
/// rectangles and where the last one ends.
pub fn stack(
    pictures: &[&Picture],
    top: f32,
    width: f32,
    max_height: f32,
) -> (Vec<Rectangle>, f32) {
    let mut y = top;
    let placed = pictures
        .iter()
        .map(|picture| {
            let Size {
                width: w,
                height: h,
            } = picture.size;
            let scale = (width / w).min(max_height / h).min(1.0);
            y += GAP;
            let rect = Rectangle::new(Point::new(0.0, y), Size::new(w * scale, h * scale));
            y += rect.height;
            rect
        })
        .collect();
    (placed, y)
}
