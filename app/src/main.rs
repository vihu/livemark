//! livemark: a desktop editor for markdown files.
//!
//! ```text
//! livemark [file.md] [--dark]
//! cargo run --release -p livemark-app -- [file.md] [--dark]
//! ```
//!
//! Opens the file in livemark's live preview editor. Saving lands with L1
//! (PLAN-001); until then edits stay in the window.
use iced::{Element, Task, Theme};
use livemark::widget::{Editor, Message};

pub fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let text = match args.iter().find(|a| !a.starts_with("--")) {
        Some(path) => std::fs::read_to_string(path).unwrap_or_else(|error| {
            eprintln!("{path}: {error}");
            std::process::exit(1);
        }),
        None => String::new(),
    };
    let theme = if args.iter().any(|a| a == "--dark") {
        Theme::Dark
    } else {
        Theme::Light
    };
    iced::application(
        move || (Editor::new(text.clone()), Editor::focus()),
        update,
        view,
    )
    .title("livemark")
    .theme(move |_: &Editor| theme.clone())
    .run()
}

fn update(editor: &mut Editor, message: Message) -> Task<Message> {
    editor.update(message);
    Task::none()
}

fn view(editor: &Editor) -> Element<'_, Message> {
    editor.view()
}
