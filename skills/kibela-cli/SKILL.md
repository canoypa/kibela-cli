---
name: kibela-cli
description: kibela is a CLI for reading Kibela notes (`https://<team>.kibe.la/...`). Use when searching Kibela, reading or saving a note's Markdown body, reading a note's comments and inline comments, or browsing a folder's notes.
---

# kibela-cli

`kibela` reads Kibela notes through the Kibela API.

## Setup

```sh
brew install canoypa/tap/kibela   # or: cargo install --locked --git https://github.com/canoypa/kibela-cli
kibela token set <team>   # reads the token from standard input
```

## Commands

```sh
kibela search <query>... [-L <limit>] [--json <fields>]             # matches with excerpts, as JSON
kibela get <note> [--json <fields>]                                 # Markdown body of a note
kibela comments <note>                                              # comments and inline comments, as JSON
kibela folder <folder> [-L <limit>] [--json <fields>]               # a folder with its notes and subfolders, as JSON
kibela token set <team>
kibela token delete <team>
kibela team list
kibela team use <team>
```

`<note>` is a note number or a note URL. `<folder>` is a folder number, a path (`/folders/<number>`), or a folder URL. Commands that call the API accept `--team <team>`.

`kibela <command> --help` describes each command's output and lists the fields `--json` accepts.
