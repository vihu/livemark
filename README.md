# livemark

[![CI](https://github.com/vihu/livemark/actions/workflows/ci.yml/badge.svg)](https://github.com/vihu/livemark/actions/workflows/ci.yml)
[![Release](https://github.com/vihu/livemark/actions/workflows/release.yml/badge.svg)](https://github.com/vihu/livemark/actions/workflows/release.yml)

A live-preview markdown editor for [iced]: you type markdown and it renders
in place, like Obsidian's Live Preview. The markdown text is the document,
so git, other editors and agents edit the same file.

## Demo

## Design

- Source-first: nothing but the text is stored. A file loaded and saved
  without edits comes back byte for byte.
- CommonMark 0.31.2 plus GFM tables, task lists, strikethrough and
  autolinks, parsed by pulldown-cmark. The view never disagrees with the
  parser.
- Behaviour after Obsidian's Live Preview, with CodeMirror 6 and
  SilverBullet as references.
- An iced-free core (`doc`, `parse`, `style`, `layout`, `edit`), so editing
  is tested without a window.

Status: 0.2.0, not on crates.io (changes in `CHANGELOG.md`). Targets Linux
(Wayland) and macOS. It is both a library for iced apps and a standalone
editor (the `livemark` app).

## What it edits

| Area         | Supported                                                                                                                                   |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Live preview | Headings, emphasis, links, `==highlight==`, lists as dots, clickable task boxes, quotes, rules; markers hidden until the caret touches them |
| Blocks       | Fenced code with syntax colours, tables as grids until the caret enters, images under their line                                            |
| Front matter | Drawn as the title, date and tag chips (a cross removes, "+ tag" adds)                                                                      |
| Editing      | Lists and quotes continue on Enter, Tab nests; formatting keys; line commands; find and replace; undo in bursts                             |
| Modes        | Live preview, Markdown (as written), and Side by side (scrolled together)                                                                   |

## The app

Download it from [Releases](https://github.com/vihu/livemark/releases):

- Linux (x86_64 and arm64), either of:
  - `livemark-<version>-<arch>.flatpak`: run
    `flatpak install --user livemark-<version>-<arch>.flatpak`, then start
    livemark from the app menu.
  - `livemark-<version>-<arch>.AppImage`: `chmod +x` it and run it. Needs
    glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 and later).
- macOS 11 or newer: `livemark-<version>-macos-universal.zip`. Unzip it and
  move `livemark.app` to Applications. It is not notarized: allow the first
  launch in System Settings > Privacy & Security > Open Anyway, or run
  `xattr -dr com.apple.quarantine /Applications/livemark.app`.

Or run it from source:

```text
cargo run --release -p livemark-app -- [file.md] [--dark|--light]
```

## Notes from scripts and agents

Two commands that never open a window:

```text
livemark note "Flaky test cause" --tags work,ci --by claude-code < body.md
livemark tags
```

`note` writes a new note into the vault (the one the app last opened, or
`--vault <dir>`), prints its path, and never overwrites one. `tags` lists
the tags in use, `tag count` a line. `livemark --help` has the rest.

`skills/writing-livemark-notes/` is an [Agent Skill] that teaches a coding
agent when and how to write a note: only when asked, existing tags first,
no secrets. Link it where your agent looks for skills:

```text
ln -s "$PWD/skills/writing-livemark-notes" ~/.claude/skills/   # Claude Code
ln -s "$PWD/skills/writing-livemark-notes" ~/.agents/skills/   # pi, and others
```

The agent needs `livemark` on its `PATH`: the AppImage renamed to
`livemark`, `livemark.app/Contents/MacOS/livemark` on macOS, or
`cargo install --locked --git https://github.com/vihu/livemark livemark-app`.
With the Flatpak, a script named `livemark` that runs
`exec flatpak run io.github.vihu.livemark "$@"` does it.

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

- `Editor::text` is the markdown to save. `Editor::version` changes on
  every edit and comes back with undo: compare it with the last save's to
  know when to autosave.
- `Editor::toolbar` is the formatting and mode buttons;
  `toolbar_tools` and `toolbar_modes` are its halves, for a host that puts
  its own items between them.
- Pictures: `Editor::image_urls` lists where the note's images point; hand
  their bytes to `Editor::set_image`. A pasted picture comes back as
  `Message::pasted_image`.
- Completion: `Editor::completing` says what follows `[[` or `#`; answer
  with `Editor::set_choices`.
- `Message::tag` is a `#tag` the user asked to see; `Editor::set_footer`
  lists links under the note's last line.

## How it is checked

CI runs all of this on Linux and macOS for every push to `main` and every
pull request.

- The CommonMark and GFM spec examples: GFM 23 of 23, CommonMark 648 of
  652 (the rest differ on purpose: GFM autolinks and front matter).
- Seeded random sessions: `tests/doc_fuzz.rs` (the text always equals the
  same edits on a plain `String`), `tests/projection_fuzz.rs` (every
  position maps to the screen and back), `tests/markup_fuzz.rs` and
  `tests/widget_fuzz.rs` (the widget in iced's simulator, every round
  drawn).
- Widget tests through iced's simulator in `tests/editor_widget.rs`,
  `tests/side_by_side.rs` and `tests/completion.rs`, and the app's own
  tests in `app/src/`.
- `cargo run --release --example bench` measures frame times at 1,000 to
  20,000 lines.

## License

MIT, see `LICENSE`. Code ported from CodeMirror 6, SilverBullet and
ink-mde, and the spec examples in the tests, keep their notices in
`THIRD-PARTY-NOTICES.md`. The bundled fonts,
Atkinson Hyperlegible Next and JetBrains Mono, are under the SIL Open Font
License 1.1, with their licences under `assets/fonts/`.

[Agent Skill]: https://agentskills.io
[iced]: https://github.com/iced-rs/iced
