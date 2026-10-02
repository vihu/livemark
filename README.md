# livemark

[![CI](https://github.com/vihu/livemark/actions/workflows/ci.yml/badge.svg)](https://github.com/vihu/livemark/actions/workflows/ci.yml)
[![Release](https://github.com/vihu/livemark/actions/workflows/release.yml/badge.svg)](https://github.com/vihu/livemark/actions/workflows/release.yml)

A live-preview markdown editor for [iced]: you type markdown and it renders
in place as you type, like Obsidian's Live Preview. The markdown text is the
document. Nothing else is stored, so other tools edit the same file.

Status: 0.1.0, not on crates.io (changes in `CHANGELOG.md`). Live preview of headings,
emphasis, links, escapes, lists (bullets as dots), task lists (clickable
checkboxes), quotes, rules, code (syntax colors in fenced blocks, fences
hidden) and tables (grids until the caret enters); list-aware editing;
formatting keys; line commands; find and replace; a source mode; zoom;
YAML front matter and `==highlight==`; images drawn under their line from
bytes the host supplies; a toolbar and a Split mode with the rendered note beside the markdown, the
two scrolled together. Targets Linux (Wayland) and macOS. It is both a library for
iced apps and a standalone editor (the `livemark` app).

## The app

Download it from [Releases](https://github.com/vihu/livemark/releases):

- Linux (x86_64 and arm64), either of:
  - `livemark-<version>-<arch>.flatpak`: run
    `flatpak install --user livemark-<version>-<arch>.flatpak` (it fetches
    the runtime from Flathub), then start livemark from the app menu. It
    can read and write in your home folder, where a note's pictures are.
  - `livemark-<version>-<arch>.AppImage`: make it executable
    (`chmod +x`) and run it. Needs glibc 2.35 or newer (Ubuntu 22.04,
    Debian 12, Fedora 36 and later).
- macOS 11 or newer (Apple silicon and Intel):
  `livemark-<version>-macos-universal.zip`. Unzip it and move
  `livemark.app` to Applications. The app is not notarized, so macOS blocks
  the first launch: allow it in System Settings > Privacy & Security > Open
  Anyway, or run `xattr -dr com.apple.quarantine /Applications/livemark.app`.

Or run it from source:

```text
cargo run --release -p livemark-app -- [file.md] [--dark|--light]
```

Light or dark follows the system unless a flag says. Ctrl+N starts a new
note, Ctrl+O opens, Ctrl+S saves (asking where for a new note),
Ctrl+Shift+S saves as; Ctrl+= and Ctrl+- (or Ctrl with the mouse wheel)
make the text bigger or smaller (50% to 300%), Ctrl+0 resets it. A `*` in the
title marks unsaved changes; closing or opening another file then asks.
When the window comes back into focus and the file changed on disk (git,
another editor), it is loaded again; with unsaved changes the app asks
which to keep. Images are read from files next to the note (relative
paths); the app makes no network requests, so web images stay as markdown.
A picture pasted with Ctrl+V is saved as `assets/<note>-<n>.png` next to the
note and linked at the caret (a new note is saved first); a picture file
dropped on the window is linked by its path, a dropped `.md` file opens.
The toolbar's File menu has New, Open, Save, Save as, the recent notes and
the theme (System, Light, Dark). The zoom, the window's size, the theme and
the ten most recent files are kept in `~/.config/livemark/settings` (macOS:
`~/Library/Application Support/livemark/settings`), a `key = value` file.

Keys in the editor (Cmd instead of Ctrl on macOS):

| Keys | Does |
| --- | --- |
| Ctrl+B, Ctrl+I, Ctrl+E | Bold, italic, inline code: on the selection or the word, off inside one |
| Ctrl+K | A link: `[text]()` around the selection, `[](url)` around a URL |
| Ctrl+Shift+E | Live preview, the markdown as written, or Split: the markdown beside the rendered note, scrolled together (a click there puts the caret at that place) |
| Ctrl+F, Ctrl+H | Find, find and replace; Enter, F3 or Ctrl+G for the next match, with Shift the previous; Tab to the next field; Escape closes |
| Enter | Continues a list item (next number, unchecked box) or quote; on an empty item, one level less |
| Shift+Enter | A new line indented to the item's text, without a marker |
| Tab, Shift+Tab | Nests a list item (with its children) under the one before, or moves it back out |
| Backspace | After a list marker or `>`, removes that markup first |
| Alt+Up, Alt+Down | Moves the selected lines up or down; with Shift, copies them |
| Ctrl+Shift+K | Deletes the selected lines |
| Ctrl+Enter | A blank line below, indented like this one |
| Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z | Undo, redo (typing undoes in bursts) |
| Ctrl+C, Ctrl+X | With nothing selected, copy or cut the whole line (pasted back as a line) |
| Home, End | The start or end of the row, then of the line; Home skips list and quote markup |
| Double, triple click | Selects a word, a line; dragging extends by words or lines |
| Ctrl+click on a link, Alt+Enter in one | Opens it (web and mail links) |
| Click on a checkbox | Checks or clears the task |

Markers (`**`, `#`, `[`, `](url)`) stay hidden until the caret touches
them; bullets show as dots, task boxes as checkboxes, rules as lines, code
fences as a language label, and tables as grids until the caret enters. The scroll bar on the right drags, and a click on its track jumps.

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
    // Ctrl/Cmd+click on a link, or Alt+Enter in it: the host opens it.
    if let Some(link) = message.link() {
        println!("open {link}");
    }
    // The task writes the clipboard after a copy or cut.
    editor.update(message)
}

fn view(editor: &Editor) -> Element<'_, Message> {
    editor.view()
}
```

`Editor::text` is the markdown to save, byte for byte as loaded plus the
edits; `Editor::version` changes on every edit and comes back with undo, so
compare it with the value from the last save to know when to autosave.
`Editor::toolbar` is a row of buttons (bold, italic, code, link, heading,
list, task, quote, the mode) to show where the host likes; each runs its
key's command. A paste with no text but a picture comes to the host as
`Message::pasted_image` (PNG bytes); keep it and answer with
`Editor::insert_text`. `Editor::image_urls` lists where the note's images point; hand each
picture's bytes to `Editor::set_image` (PNG, JPEG, GIF, WebP) and it is drawn
under its line while its markdown hides. `Editor::set_zoom` scales the
text.

[iced]: https://github.com/iced-rs/iced

## Licence

MIT, see `LICENSE`.
