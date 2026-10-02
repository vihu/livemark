//! A markdown file drawn headlessly by the editor, for typography reviews.
//!
//! ```text
//! cargo run --release --example snapshot -- <file.md> <out.png> [--dark] [--source] [--caret <offset>|<anchor>..<head>] [--font <dir>] [--size <w>x<h>]
//! ```
//!
//! Writes `<out>-<renderer>.png` at 2x, 900 by 1100 points (or `--size`), with the caret
//! (and so the revealed markers) at `--caret` (default: the end). `--font`
//! draws prose in the family of the font files in `<dir>`, to compare
//! typefaces.
use std::borrow::Cow;

use iced::advanced::graphics::text::font_system;
use iced::{Event, Point, Theme, mouse};
use livemark::widget::Editor;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, out, ..] = args.as_slice() else {
        eprintln!(
            "usage: snapshot <file.md> <out.png> [--dark] [--source] [--caret <offset>|<anchor>..<head>] [--font <dir>] [--size <w>x<h>]"
        );
        std::process::exit(2);
    };
    let theme = if args.iter().any(|a| a == "--dark") {
        Theme::Dark
    } else {
        Theme::Light
    };
    // `--caret 12` or a selection `--caret 4..30`.
    let caret: Option<(usize, usize)> = args.iter().position(|a| a == "--caret").and_then(|i| {
        let value = args.get(i + 1)?;
        let (a, b) = value.split_once("..").unwrap_or((value, value));
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    if let Some(dir) = args
        .iter()
        .position(|a| a == "--font")
        .and_then(|i| args.get(i + 1))
    {
        prose_font(dir);
    }
    let mut editor = Editor::new(std::fs::read_to_string(file).expect("the markdown file"));
    let end = editor.text().len();
    let (anchor, head) = caret.unwrap_or((end, end));
    editor.select(anchor, head);
    if args.iter().any(|a| a == "--source") {
        editor.set_mode(livemark::widget::Mode::Source);
    }
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| {
            let (w, h) = args.get(i + 1)?.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((900.0, 1100.0));
    let mut ui = iced_test::Simulator::with_size(iced::Settings::default(), size, editor.view());
    // A press outside the text focuses nothing and moves nothing; it only
    // lets the first frame settle the layout.
    ui.point_at(Point::new(-10.0, -10.0));
    ui.simulate([Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(-10.0, -10.0),
    })]);
    let snapshot = ui.snapshot(&theme).expect("a snapshot");
    let path = std::path::Path::new(out);
    let _ = std::fs::remove_file(path.with_file_name(format!(
        "{}-wgpu.png",
        path.file_stem().unwrap().to_string_lossy()
    )));
    snapshot.matches_image(path).expect("the PNG");
    println!("wrote {}-<renderer>.png", path.with_extension("").display());
}

/// Loads the `.ttf` and `.otf` files in `dir` and makes their family the
/// one prose is drawn in (the font system's sans-serif).
fn prose_font(dir: &str) {
    let mut system = font_system().write().expect("font system lock");
    let before: Vec<_> = system.raw().db().faces().map(|face| face.id).collect();
    for entry in std::fs::read_dir(dir).expect("the font directory") {
        let path = entry.expect("a directory entry").path();
        if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("ttf" | "otf")
        ) {
            system.load_font(Cow::Owned(std::fs::read(&path).expect("the font file")));
        }
    }
    let family = system
        .raw()
        .db()
        .faces()
        .find(|face| !before.contains(&face.id))
        .map(|face| face.families[0].0.clone())
        .expect("a font in the directory");
    println!("prose in {family}");
    system.raw().db_mut().set_sans_serif_family(family);
}
