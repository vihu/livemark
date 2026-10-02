//! A markdown file drawn headlessly by the editor, for typography reviews.
//!
//! ```text
//! cargo run --release --example snapshot -- <file.md> <out.png> [--dark] [--source|--split] [--caret <offset>|<anchor>..<head>] [--size <w>x<h>] [--zoom <factor>] [--toolbar]
//! ```
//!
//! Writes `<out>-<renderer>.png` at 2x, 900 by 1100 points (or `--size`), with the caret
//! (and so the revealed markers) at `--caret` (default: the end).
use iced::{Event, Point, Theme, mouse};
use livemark::widget::Editor;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, out, ..] = args.as_slice() else {
        eprintln!(
            "usage: snapshot <file.md> <out.png> [--dark] [--source|--split] [--caret <offset>|<anchor>..<head>] [--size <w>x<h>] [--zoom <factor>] [--toolbar]"
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
    let mut editor = Editor::new(std::fs::read_to_string(file).expect("the markdown file"));
    // Pictures next to the file, as the app finds them.
    let folder = std::path::Path::new(file)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    for url in editor.image_urls() {
        if let Ok(bytes) = std::fs::read(folder.join(&url)) {
            editor.set_image(&url, &bytes);
        }
    }
    let end = editor.text().len();
    let (anchor, head) = caret.unwrap_or((end, end));
    editor.select(anchor, head);
    if args.iter().any(|a| a == "--source") {
        editor.set_mode(livemark::widget::Mode::Source);
    }
    if args.iter().any(|a| a == "--split") {
        editor.set_mode(livemark::widget::Mode::Split);
    }
    if let Some(zoom) = args
        .iter()
        .position(|a| a == "--zoom")
        .and_then(|i| args.get(i + 1)?.parse().ok())
    {
        editor.set_zoom(zoom);
    }
    let size = args
        .iter()
        .position(|a| a == "--size")
        .and_then(|i| {
            let (w, h) = args.get(i + 1)?.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((900.0, 1100.0));
    // With `--toolbar`, the toolbar above the text, as the app shows it.
    let view: iced::Element<'_, _> = if args.iter().any(|a| a == "--toolbar") {
        iced::widget::column![
            iced::widget::container(editor.toolbar()).padding([6, 12]),
            editor.view()
        ]
        .into()
    } else {
        editor.view()
    };
    let mut ui = iced_test::Simulator::with_size(iced::Settings::default(), size, view);
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
