//! A markdown file drawn headlessly by the editor, for typography reviews.
//!
//! ```text
//! cargo run --release --example snapshot -- <file.md> <out.png> [--dark] [--caret <offset>]
//! ```
//!
//! Writes `<out>-<renderer>.png` at 2x, 900 by 1100 points, with the caret
//! (and so the revealed markers) at `--caret` (default: the end).
use iced::{Event, Point, Theme, mouse};
use livemark::widget::Editor;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, out, ..] = args.as_slice() else {
        eprintln!("usage: snapshot <file.md> <out.png> [--dark] [--caret <offset>]");
        std::process::exit(2);
    };
    let theme = if args.iter().any(|a| a == "--dark") {
        Theme::Dark
    } else {
        Theme::Light
    };
    let caret = args
        .iter()
        .position(|a| a == "--caret")
        .and_then(|i| args.get(i + 1)?.parse().ok());
    let mut editor = Editor::new(std::fs::read_to_string(file).expect("the markdown file"));
    if let Some(caret) = caret {
        editor.select(caret, caret);
    } else {
        let end = editor.text().len();
        editor.select(end, end);
    }
    let size = (900.0, 1100.0);
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
