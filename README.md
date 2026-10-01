# livemark

A live-preview markdown editor for [iced]: you type markdown and it renders
in place as you type, like Obsidian's Live Preview. The markdown text is the
document. Nothing else is stored, so other tools edit the same file.

Status: L0 spike (live preview of headings and inline styles, typing, IME), not on crates.io. Targets Linux (Wayland) and macOS. It is
both a library for iced apps and a standalone editor (the `livemark` app).

## Embedding

Keep an `Editor` in your state, show its view, and pass its messages back:

```no_run
use iced::{Element, Task};
use livemark::widget::{Editor, Message};

fn main() -> iced::Result {
    let boot = || (Editor::new("# Notes\n\nType *here*.\n".into()), Editor::focus());
    iced::application(boot, update, view).run()
}

fn update(editor: &mut Editor, message: Message) -> Task<Message> {
    editor.update(message);
    Task::none()
}

fn view(editor: &Editor) -> Element<'_, Message> {
    editor.view()
}
```

`Editor::text` is the markdown to save, byte for byte as loaded plus the
edits; `Editor::version` changes on every edit and comes back with undo, so
compare it with the value from the last save to know when to autosave.

[iced]: https://github.com/iced-rs/iced

## Licence

MIT, see `LICENSE`.
