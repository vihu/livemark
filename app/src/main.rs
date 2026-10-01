//! livemark: a desktop editor for markdown files.
//!
//! ```text
//! livemark [file.md]
//! cargo run --release -p livemark-app -- [file.md]
//! ```
//!
//! Until the live preview widget lands (PLAN-001 L0), the file opens in
//! iced's own `text_editor`, unsaved.
use iced::widget::text_editor;
use iced::widget::text_editor::{Action, Content};
use iced::{Element, Fill};

pub fn main() -> iced::Result {
    let text = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|error| {
            eprintln!("{path}: {error}");
            std::process::exit(1);
        }),
        None => String::new(),
    };
    iced::application(move || Content::with_text(&text), update, view)
        .title("livemark")
        .run()
}

fn update(content: &mut Content, action: Action) {
    content.perform(action);
}

fn view(content: &Content) -> Element<'_, Action> {
    text_editor(content)
        .on_action(|action| action)
        .height(Fill)
        .into()
}
