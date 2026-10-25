# Changelog

## Unreleased

- Save as no longer hands the new file to a note opened while its dialog
  was up, which could then be written over it; Ctrl+S writes at once.
- Undo after typing a `#tag` in the front matter's `tags` list no longer
  closes the app, and a `#` there no longer offers inline tags. Deleting
  or merging a tag a note lists twice, and renaming a note linked from a
  line with a reference definition after a link, no longer close it
  either.
- Typing in a vault with many tags is fast again: a keystroke at 1,000
  tags took 96 ms, now 0.6 ms. Autosave and coming back to the window
  read only what changed: 21 ms at 5,000 notes, now 3 ms. Search and `[[`
  completion are about twice as fast.
- Renames, tag edits and Move check that every file is still as read
  before writing any, and keep their Undo when they stop part way.
  Settings are written through a temporary file. Autosave says at once
  when the file changed on disk.
- A note's title comes from its first heading as the parser reads it,
  never a `#` comment in code, and a rename changes that heading at any
  level. Links to files with `#`, `?`, `%` or parentheses in their names
  work. A `.txt` file moved into the vault becomes a `.md` note in its
  list. Files that do not read are counted at the sidebar's foot.
- `livemark note` fails when its body is not UTF-8 instead of writing an
  empty note, and refuses `--tags` that would break the front matter;
  `livemark search --limit` past 200 prints them all; output into a
  closed pipe (`| head`) ends quietly.
- A crash leaves its report in `crash.log` next to `settings.toml`.
- For hosts: `widget::Selection`, `ZOOMS` and `SPLIT_RATIOS`;
  `set_zoom` ignores NaN; Shift is published only while the find bar is
  open.

## 0.3.3 (2026-10-15)

- Tables wrap long cells instead of cutting them off: a column of short
  words keeps them whole, long text wraps, and each row is as tall as
  its tallest cell.

## 0.3.2 (2026-10-13)

- The note's text is at most 120 columns of code wide, centred, so long
  code lines wrap where they can be read; Appearance steps it from 60 to
  240 or to the whole width (`line-width` in the settings).
  `Editor::set_line_width` for hosts.

## 0.3.1 (2026-10-06)

- `livemark picture <note> <file>...` copies pictures into `assets/`
  next to a note, never over a file, and prints the markdown line for
  each, so agents can put pictures in notes.
- The agent skill lets agents edit a note's text when asked, with their
  own editing tool (never renaming, moving or deleting a note), and add
  pictures with `livemark picture`: link or copy it again.

## 0.3.0 (2026-10-05)

- `livemark list` and `livemark search` let scripts and agents find
  notes: the newest first, by tag, or by their words, with `--sort` and
  `--limit`.
- The agent skill is now `skills/livemark-notes/` (was
  `writing-livemark-notes`) and covers finding notes too: link it again.

## 0.2.1 (2026-10-04)

- The vault menu shows the version at its foot, so a Flatpak or macOS
  build says which it is.

## 0.2.0 (2026-10-03)

Vaults: a folder of notes with search, tags, links between notes and
note management, plus a calmer window, themes and autosave.

- `livemark --help` lists every option, `--version` says the version,
  and a mistyped option is named with a hint.
- Show in folder selects the note in the file manager on Linux too, also
  from the Flatpak.
- Folders linked into a vault are read too, and a linked note's edits
  show up like any other's.
- Settings are kept in `settings.toml`. The old `settings` file is not
  read, so the theme, sizes, recent files and vaults start fresh once.
- A calmer window: the sidebar runs to the top with the vault menu in
  File's place; the note's bar has the formatting buttons without boxes,
  the search, whether the note is saved, and the mode as three icons.
  Ctrl+\ hides the sidebar, its edge drags to resize it; Ctrl+Q quits.
- Right-click menus float where you click, on notes, tags and the text
  (Cut, Copy, Paste, Select all), and close on any click elsewhere.
- Vaults: the vault menu's Switch vault lists the vaults opened, one click
  away (Open recent beside it, recent notes by title); a vault
  opens with the note last open in it and nothing of the last vault left
  over; a note of another vault opened from the recent files brings its
  vault; Close vault.
- Notes from the sidebar: a right press opens a note's menu: rename (F2
  for the open note; the file is renamed with the title and the links to
  it follow), duplicate, copy a link, show it in its folder, delete. Each
  can be undone.
- A note opened from outside the vault says so in a bar over it, with
  Move into vault and Copy into vault (front matter added when missing).
- Autosave, on by default: notes are written two seconds after typing
  stops, on leaving the window and before another note opens; off in
  Appearance.
- Appearance: Kanagawa Wave and Lotus, Solarized, Gruvbox and Catppuccin
  Mocha and Frappe themes besides Light and Dark, System following the
  system between a light and a dark pick; Ctrl+= and Ctrl+- scale the
  whole interface, the text's size set apart.
- Vaults: Open vault (in the vault menu) makes a folder of notes a vault, kept in your
  own git repository (the app never runs git). A sidebar lists the notes
  and their tags (front matter and inline `#tags`), a click on a tag
  showing only its notes, and how many notes changed since your last
  commit.
- Ctrl+N in a vault names a new note and makes it as
  `2026-10-02-the-title.md` with front matter.
- A title in any script gives a readable file name (`日本` makes
  `ri-ben`, not `note`).
- One search field in the toolbar (Ctrl+P) finds notes by title, lines in
  notes and tags; Ctrl+Enter lists every match in the sidebar.
- Search marks what matched in every result, and finds titles by their
  words in any order, forgiving a typo; letters scattered through a title
  no longer match.
- Tags in the sidebar are a flat list, most used first; each one's menu
  renames, merges or deletes it across the vault, with Undo. Manage tags
  opens the tag manager: sort, merge or delete several at once, and tags
  that look alike are suggested for merging.
- Front matter is drawn as the note's properties: its title, its date and
  its tags as chips with a cross; "+ tag" or Ctrl+T adds one, with the
  vault's tags offered.
- `#tags` are drawn as tags; Ctrl+click on one shows its notes.
- Typing `[[` picks a note and puts in a normal markdown link to it;
  links to notes open in the app, and "Linked from" at the end of a note
  lists the notes linking to it, with the line each link sits in.
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
