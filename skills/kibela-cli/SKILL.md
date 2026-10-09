---
name: kibela-cli
description: kibela is a CLI for reading Kibela notes (`https://<team>.kibe.la/...`). Use when searching Kibela (by group, folder, author, or date as well), reading or saving a note's Markdown body, reading a note's comments and inline comments, or browsing a folder's notes.
---

# kibela-cli

`kibela` reads Kibela notes through the Kibela API.

## Setup

```sh
brew install canoypa/tap/kibela   # or: cargo install --locked --git https://github.com/canoypa/kibela-cli
kibela team add <team>    # reads the token from standard input
```

## Commands

```sh
kibela search <query>... [-L <limit>] [--json <fields>] [filters]   # matches with excerpts, as JSON
kibela note view <note> [--json <fields>]                           # Markdown body of a note
kibela note comments <note>                                         # comments and inline comments, as JSON
kibela folder view <folder> [-L <limit>] [--json <fields>]          # a folder with its notes and subfolders, as JSON
kibela group list                                                   # groups, as JSON
kibela team add <team>
kibela team remove <team>
kibela team list
kibela team use <team>
```

`<note>` is a note number or a note URL. `<folder>` is a folder number, a path (`/folders/<number>`), or a folder URL. Commands that call the API accept `--team <team>`. `search` filters by `--sort`, `--updated`, `--resource`, `--archived`, `--coediting`, `--group`, `--folder`, `--user`, and `--liker`.

`kibela <command> [<subcommand>] --help` describes each command's output and options and lists the fields `--json` accepts.
