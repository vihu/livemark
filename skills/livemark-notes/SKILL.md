---
name: livemark-notes
description: Finds, writes and edits notes in the user's livemark vault (a folder of plain markdown notes kept in their own git repository) with the livemark CLI, which lists the newest notes, searches them by words and tags, writes new ones named by date and title with front matter for the title, tags and the agent that wrote it, and copies pictures in next to a note. Use when the user asks to look something up in their notes or vault, asks what they noted about something, asks to note something down, keep a finding, decision, link, picture or to-do for later, or asks to change, add to or tick off something in a note.
license: MIT
compatibility: Requires the livemark CLI with the note, tags, list, search and picture commands on PATH, and a vault (one the user opened in the livemark app, or a folder passed with --vault). Works offline.
---

# livemark notes

The user keeps notes as markdown files in a folder (a vault) that they
sync with their own git. The livemark CLI finds notes there and writes new
ones in the vault's convention: `YYYY-MM-DD-title-in-lowercase.md`, front
matter with the title, tags, date and who wrote it. Never create note files
by hand. Existing notes are plain markdown: edit them with your own file
editing tool.

Run `livemark --help` first. If it is not found, or it has no `list`,
`search` and `picture` commands, stop and ask the user to install or update livemark
(https://github.com/vihu/livemark).

## When

- Finding: only when the user asks to look something up in their notes
  ("what did I note about the deploy key", "my notes from this week").
  Do not read the vault on your own initiative: it holds personal notes.
- Writing: only when the user asks for a note: "note this down", "keep
  this for later", "save it to my notes". Never more than the user asked
  for: one note per thing to keep.
- Editing: only when the user asks to change a note ("add this to my
  deploy note", "tick off the runner task"), and only that note.

## Finding notes

1. `livemark search <words>` finds the notes with every word, in any
   case, anywhere in the note (title, front matter, body). A `#tag` among
   the words, or `--tag work`, keeps notes with that tag. It prints one
   line per matching line, at most five a note, in `grep -n` form, the
   text cut to the match with `…`:

   ```text
   /home/me/notes/2026-10-02-deploy-key-rotated.md:7:Rotated the staging deploy key after the CI runner…
   ```

2. `livemark list` prints the notes, newest first, one
   `date<TAB>path<TAB>title<TAB>#tags` line each. The date is the day
   the file last changed; with `--sort created` it is the note's own
   date and the order follows it (undated notes last). `--tag` narrows as
   in search; given more than once, every tag must match.
3. Both print at most 20 notes, newest changed first (`--sort created`
   works on search too). When more matched, stderr says
   `livemark: 20 of 57 notes shown; --limit for more`: add words or
   `--tag` first, raise `--limit` only when you need them all.
4. Nothing printed means nothing matched: try fewer or other words.
5. Read the notes you need from their paths with your file reading tool.
   Answer from what they say, and give the user the paths you used.

## Writing a note

1. Search first, with the key words of what to keep. If a note on the
   same thing exists, still write the new one (the user asked), link the
   older note in its body as `[its title](its path relative to the
   vault)`, and tell the user both paths.
2. `livemark tags` lists the tags in use, most used first (`tag count` a
   line). Pick one to three of them that fit. Make a new tag only when
   none fits: lowercase, one word or words joined by hyphens.
3. Choose a title: short and specific, in sentence case, naming what the
   note is about ("Deploy key rotated on staging", not "Note" or
   "Update"). It becomes the file's name, so keep it under about eight
   words.
4. Write the body in markdown. Lead with the point, then what the user
   will need to act on it later: commands, paths, links, versions, dates.
   Use `- [ ]` for follow-ups. No heading repeating the title: the title
   is in the front matter.
5. Write it, naming yourself with `--by` (`claude-code`, `pi`):

   ```bash
   livemark note "Deploy key rotated on staging" --tags work,ops --by claude-code <<'EOF'
   Rotated the staging deploy key after the CI runner change.

   - New key fingerprint: SHA256:... (in the team vault, not here)
   - [ ] Remove the old key from the runner image
   EOF
   ```

   It prints the new note's path. Tell the user that path.

## Editing a note

1. Find the note (`livemark search` or `list`) and read all of it.
2. Change only what the user asked, with your own file editing tool.
   Leave everything else as it is: other lines, spacing, line endings.
3. In the front matter, `title:` and `tags:` may change; keep the tags
   list in the form it has (`[work, ops]`). Leave `created:` and `by:`
   as they are.
4. Tell the user the path of the note you changed.

The file name stays when the title changes. Never rename, move or delete
a note file: other notes link to it by its path. Ask the user to rename
or delete it in the livemark app, which updates the links.

## Adding a picture

1. Have the picture as a file: a screenshot, a chart you rendered. PNG,
   JPEG, GIF or WebP, at most 32 MB.
2. Write the note first (or find it): the command needs its path.
3. Copy the picture in next to the note:

   ```bash
   livemark picture /home/me/notes/2026-10-06-flame-graph-after-the-fix.md /tmp/flame.png
   ```

   It prints one markdown line per picture, its path from the note:

   ```text
   ![](assets/2026-10-06-flame-graph-after-the-fix-1.png)
   ```

4. Put that line in the note where the picture belongs, with your file
   editing tool, and describe the picture in the brackets:
   `![Flame graph after the fix](assets/...)`.

Never link a picture where it was (`/tmp/flame.png`): the note must work
from the vault alone. Never add a picture that shows a secret.

## Errors

A non-zero exit means nothing was written (2 when the arguments did not
parse, 1 otherwise); read the message on stderr:

- `no vault`: ask the user which folder holds their notes (or to open it
  once in the livemark app: vault menu, Switch vault, Open vault), then
  pass `--vault <dir>`.
- Anything else: fix the arguments as the message says and run it again.
  A second note with the same title on the same day is fine: it gets
  `-2`, never overwriting the first.

## Never

- Put secrets in a note: passwords, tokens, API keys, private keys, the
  contents of `.env` files or anything that looks like a credential. Say
  where the secret lives instead. Notes end up in a git repository.
- Rename, move or delete the user's notes, or edit a note the user did
  not ask about.
- Run git in the vault. The user commits, signs and pushes; the app shows
  them what changed.

## What a note looks like

```markdown
---
title: Deploy key rotated on staging
tags: [work, ops]
created: 2026-10-02
by: claude-code
---

Rotated the staging deploy key after the CI runner change.
```

The livemark app lists it in the vault's sidebar under its tags, and the
user finds it with Ctrl+P by title or Ctrl+Shift+F by its words.
