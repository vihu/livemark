# Changelog

## 0.1.0 (unreleased)

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
