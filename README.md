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
YAML front matter (drawn as its title, date and tags, the tags as chips
with a cross and "+ tag", until the caret enters) and `==highlight==`; images drawn under their line from
bytes the host supplies; a toolbar; three modes, Live preview, Markdown (as written) and Side by
side (the rendered note beside the markdown, the two scrolled together). Targets Linux (Wayland) and macOS. It is both a library for
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

The theme follows the system unless a flag says or you pick one. Ctrl+N starts a new
note, Ctrl+O opens, Ctrl+S saves (asking where for a new note),
Ctrl+Shift+S saves as; Ctrl+= and Ctrl+- scale the whole window (50% to
200%), Ctrl+0 resets it; Ctrl with the mouse wheel over the note sizes its
text, also set in Appearance. A `*` in the
title marks unsaved changes; closing or opening another file then asks.
When the window comes back into focus and the file changed on disk (git,
another editor), it is loaded again; with unsaved changes the app asks
which to keep. Images are read from files next to the note (relative
paths); the app makes no network requests, so web images stay as markdown.
A picture pasted with Ctrl+V is saved as `assets/<note>-<n>.png` next to the
note and linked at the caret (a new note is saved first); a picture file
dropped on the window is linked by its path, a dropped `.md` file opens.
The vault menu (the name at the sidebar's top) > Open vault picks a folder of notes (a vault, kept in your own git
repository; the app never runs git): a sidebar lists its notes, most
recently changed first, and their tags (front matter `tags: [work,
travel]` and inline `#tags`) with counts, a click on one showing only its
notes. Nothing is stored but the notes: they are read into memory (5,000
notes in about 0.1 s) and read again when the window comes back. In a
vault Ctrl+N asks for a title and makes `2026-10-02-lisbon-hotels.md`
(the date, then the title in lowercase with hyphens; `-2` when taken) with
front matter for its title, tags and date; the file keeps its name when
the title changes. The search field in the toolbar's centre (Ctrl+P, or
Ctrl+Shift+F) finds notes by part of their title (most recent first among
equals), lines in the other notes with all the words (any case), and tags;
`#travel` narrows to a tag. Up and Down choose; Enter opens a note (a line
with its match selected) or shows a tag's notes; Escape closes; Ctrl+Enter
lists every matching note in the sidebar, each with its lines, until Clear.
Outside a vault it finds the recent files by name. Typing `[[` lists notes by title and puts in a
normal markdown link to the one chosen (`[Lisbon hotels](2026-10-02-lisbon-hotels.md)`,
so links work in the git web UI too); typing `#` and a letter lists the
vault's tags. Ctrl+click on a link to a note opens it here, and "Linked
from" at the end of the note lists the notes linking to it, each with the
line its link sits in, a click opening it. A right press on a note in the sidebar opens its menu: Open, Rename (F2 for
the open note: the title and the file's name change together, its date
kept, and the links in other notes follow), Duplicate, Copy link (a
markdown link to paste into another note), Show in folder, and Delete
(which says first how many notes link to it); each ends with Undo. Each tag in the sidebar has a
menu: Rename or merge (typing a tag that exists merges the two, and
says so first) and Delete from every note (an inline `#tag` keeps its
word), and Open in the tag manager. Manage tags, under the list, shows
every tag in place of the note: how many notes have it and when the newest
was made, sorted by any column, with Rename and Delete on each row; select
several to merge them into one of them or another tag, or to delete them;
tags that look alike (a plural, a dash, a slip of a letter) come up as a
suggestion to merge. All of these rewrite only the tag's own bytes, in
front matter and in the text, and end with one Undo. Its foot says how many
notes changed since your last commit (going by file times; the app never
runs git, so committing, signing and pushing stay yours).
The sidebar runs to the window's top: its head is the vault menu (New,
Open, Open vault, the recent files, Save, Save as, the theme, Quit with
Ctrl+Q), "+" for a new note, and the button that hides the sidebar
(Ctrl+\; then the menu sits in the note's bar); drag its edge to make it
wider or narrower (a double click puts it back). The note's bar has the
formatting buttons, the search field, whether the note is saved (Saved,
Unsaved with a dot, or Not in the vault for a file from elsewhere) and the
mode. Without a vault the sidebar offers to open one and lists the recent
files. Appearance (in the vault menu) picks the theme: System, Light,
Dark, Kanagawa Wave and Lotus, Solarized Light and Dark, Gruvbox Light and
Dark, Catppuccin Mocha and Frappe, each shown as a little window in its
colours; System follows the system between a light and a dark one you
pick. It also sets the interface's size and the text's. The sizes, the
window's size, the theme,
where the divider between the side by side panes was (drag it; a double click
puts it back in the middle) and the ten most recent files are kept in `~/.config/livemark/settings` (macOS:
`~/Library/Application Support/livemark/settings`), a `key = value` file.

Keys in the editor (Cmd instead of Ctrl on macOS):

| Keys | Does |
| --- | --- |
| Ctrl+B, Ctrl+I, Ctrl+E | Bold, italic, inline code: on the selection or the word, off inside one |
| Ctrl+K | A link: `[text]()` around the selection, `[](url)` around a URL |
| Ctrl+T | A new tag: the caret at the end of the front matter's `tags` list (made when missing), the host's tags offered |
| Ctrl+Shift+E | The next mode: Live preview, Markdown (as written), Side by side (the rendered note beside the markdown, scrolled together; a click there puts the caret at that place) |
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
| Ctrl+Home, Ctrl+End | The document's start and end (on macOS also Cmd+Up, Cmd+Down) |
| Home, End | The start or end of the row, then of the line; Home skips list and quote markup |
| Double, triple click | Selects a word, a line; dragging extends by words or lines |
| Ctrl+click on a link, Alt+Enter in one | Opens it (web and mail links) |
| Ctrl+click on a `#tag`, Alt+Enter in one | Shows the vault's notes with that tag |
| Ctrl+P (or Ctrl+Shift+F) | The app's search field: notes, lines and tags; Ctrl+Enter there lists every match in the sidebar |
| F2 | In the app, renames the open note |
| Click on a checkbox | Checks or clears the task |

Markers (`**`, `#`, `[`, `](url)`) stay hidden until the caret touches
them; bullets show as dots, task boxes as checkboxes, rules as lines, code
fences as a language label, and tables as grids until the caret enters. The scroll bar on the right drags, and a click on its track jumps.

### Notes from scripts and agents

The `livemark` binary also writes notes without opening a window, for
scripts and coding agents:

```text
livemark note "Flaky test cause" --tags work,ci --by claude-code < body.md
livemark tags
```

`note` writes `2026-10-02-flaky-test-cause.md` into the vault (the one the
app last opened, or `--vault <dir>`) with front matter for its title, tags,
date and who wrote it (`by:`), the body from stdin, and prints its path. It
never prompts and never overwrites a note (a second one that day gets
`-2`). `tags` lists the vault's tags, `tag count` a line, so an agent can
reuse `travel` rather than invent `trips`. `livemark --help` shows both.

`skills/writing-livemark-notes/` is an [Agent Skill] that teaches a coding
agent (Claude Code, pi, and others that read Agent Skills) when and how to
write a note with them: only when asked, existing tags first, a specific
title, no secrets, never touching other notes or git. Link the folder where
your agent looks for skills:

```text
ln -s "$PWD/skills/writing-livemark-notes" ~/.claude/skills/   # Claude Code
ln -s "$PWD/skills/writing-livemark-notes" ~/.agents/skills/   # pi, and others
```

The agent needs `livemark` on its `PATH`: the AppImage renamed to
`livemark`, `livemark.app/Contents/MacOS/livemark` on macOS, or
`cargo install --locked --git https://github.com/vihu/livemark livemark-app`.
With the Flatpak, a one-line script on the `PATH` named `livemark` that runs
`exec flatpak run io.github.vihu.livemark "$@"` does it (the Flatpak can
write in your home folder).

[Agent Skill]: https://agentskills.io

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
key's command. `Editor::toolbar_tools` and `Editor::toolbar_modes` are its
two halves, for a host that puts its own items between them. A paste with no text but a picture comes to the host as
`Message::pasted_image` (PNG bytes); keep it and answer with
`Editor::insert_text`. `Message::tag` is a `#tag` the user Ctrl+clicked (drawn as a pill), for the
host to show. `Editor::set_footer` lists links under the note's last line
(`FooterLink`: a label, a line beside it, a destination), each handed back as
`Message::link` on a click: the app's "Linked from". `Editor::completing` says what is typed after `[[` or a tag's `#`;
answer with `Editor::set_choices` and the editor lists them under the
caret. `Editor::image_urls` lists where the note's images point; hand each
picture's bytes to `Editor::set_image` (PNG, JPEG, GIF, WebP) and it is drawn
under its line while its markdown hides. `Editor::set_zoom` scales the
text; `Editor::split_ratio` and `set_split_ratio` are the markdown's share
of the width side by side (`Mode::Split`), for a host to keep.

[iced]: https://github.com/iced-rs/iced

## Licence

MIT, see `LICENSE`.
