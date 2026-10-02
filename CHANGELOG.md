# Changelog

## Unreleased

- Vaults: File > Open vault makes a folder of notes a vault, kept in your
  own git repository (the app never runs git). A sidebar lists the notes
  and their tags (front matter and inline `#tags`), a click on a tag
  showing only its notes, and how many notes changed since your last
  commit.
- Ctrl+N in a vault names a new note and makes it as
  `2026-10-02-the-title.md` with front matter.
- Ctrl+P opens a note by its title; Ctrl+Shift+F searches every note.
- `#tags` are drawn as tags; Ctrl+click on one shows its notes.
- Typing `[[` picks a note and puts in a normal markdown link to it;
  links to notes open in the app, and the sidebar lists the notes linking
  to the open one.
- `livemark note` and `livemark tags` write and list notes from scripts
  and coding agents, and a skill teaches agents to use them.

## 0.1.0 (2026-10-02)

The first release: the library and the `livemark` desktop app.

- Live preview: headings, emphasis, links, escapes, lists, task lists with
  clickable boxes, quotes, rules, code with syntax colors, tables as grids,
  images, YAML front matter and `==highlight==`. Markers stay hidden until
  the caret touches them.
- Editing: Enter, Tab and Backspace that know lists and quotes; formatting
  keys; line commands; find and replace; undo in typing bursts. A file
  loaded and saved without edits stays byte for byte the same.
- A toolbar of icons, and three modes: Live preview, Markdown (as written)
  and Side by side (the rendered note beside the markdown, scrolled
  together, with a divider to drag); zoom from 50% to 300%.
- The app: new, open, save and recent notes from the File menu; light, dark
  or the system's theme; it asks before dropping unsaved changes and loads
  a file changed on disk again; pasted pictures are saved next to the note.
- Packages: AppImage and Flatpak for Linux (x86_64 and arm64), a universal
  zip for macOS.
