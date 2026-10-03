---
name: writing-livemark-notes
description: Writes a note into the user's livemark vault (a folder of plain markdown notes kept in their own git repository) with the livemark CLI, which names the file by date and title and adds front matter for the title, tags and the agent that wrote it. Use when the user asks to note something down, keep a finding, decision, link or to-do for later, or put something in their notes or vault.
license: MIT
compatibility: Requires the livemark CLI with the note and tags commands on PATH, and a vault (one the user opened in the livemark app, or a folder passed with --vault). Works offline.
---

# Writing livemark notes

The user keeps notes as markdown files in a folder (a vault) that they
sync with their own git. `livemark note` writes a new one there in the
vault's convention: `YYYY-MM-DD-title-in-lowercase.md`, front matter with
the title, tags, date and who wrote it. Never write note files by hand.

Run `livemark --help` first. If it is not found, or it has no `note` and
`tags` commands, stop and ask the user to install livemark
(https://github.com/vihu/livemark).

## When

Only when the user asks for a note: "note this down", "keep this for
later", "save it to my notes". Do not write notes on your own initiative,
and never more than the user asked for: one note per thing to keep.

## The steps

1. `livemark tags` lists the tags in use, most used first (`tag count` a
   line). Pick one to three of them that fit. Make a new tag only when
   none fits: lowercase, one word or words joined by hyphens.
2. Choose a title: short and specific, in sentence case, naming what the
   note is about ("Deploy key rotated on staging", not "Note" or
   "Update"). It becomes the file's name, so keep it under about eight
   words.
3. Write the body in markdown. Lead with the point, then what the user
   will need to act on it later: commands, paths, links, versions, dates.
   Use `- [ ]` for follow-ups. No heading repeating the title: the title
   is in the front matter.
4. Write it, naming yourself with `--by` (`claude-code`, `pi`):

   ```bash
   livemark note "Deploy key rotated on staging" --tags work,ops --by claude-code <<'EOF'
   Rotated the staging deploy key after the CI runner change.

   - New key fingerprint: SHA256:... (in the team vault, not here)
   - [ ] Remove the old key from the runner image
   EOF
   ```

   It prints the new note's path. Tell the user that path.
5. A non-zero exit means nothing was written (2 when the arguments did
   not parse, 1 otherwise); read the message on stderr:
   - `no vault`: ask the user which folder holds their notes (or to open
     it once in the livemark app: vault menu, Switch vault, Open vault),
     then pass
     `--vault <dir>`.
   - Anything else: fix the arguments as the message says and run it
     again. A second note with the same title on the same day is fine: it
     gets `-2`, never overwriting the first.

## Never

- Put secrets in a note: passwords, tokens, API keys, private keys, the
  contents of `.env` files or anything that looks like a credential. Say
  where the secret lives instead. Notes end up in a git repository.
- Edit, rename or delete the user's existing notes. The CLI only adds.
- Run git in the vault. The user commits, signs and pushes; the app shows
  them what changed.

## What the file looks like

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
