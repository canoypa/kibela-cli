# kibela-cli

Read [Kibela](https://kibe.la/) notes from the command line. `kibela` returns only the parts you ask for — a note's Markdown body, search matches with excerpts, or its comments — so AI agents can pull Kibela content into their context without the rest of the page.

## Installation

```sh
brew install canoypa/tap/kibela
```

Or build from source:

```sh
cargo install --locked --git https://github.com/canoypa/kibela-cli
```

## Setup

Add your team (`<team>` in `https://<team>.kibe.la`) with an access token created at `https://<team>.kibe.la/settings/access_tokens`. The token is read from standard input, or prompted for without echo in a terminal:

```sh
kibela team add <team>
```

The token is checked against the API and stored in the OS keychain.

Commands pick the team in this order: `--team` or the team in a note URL, then the default team set with `kibela team use <team>`, then the only added team.

## Commands

```sh
kibela search <query>... [-L <limit>] [--json <fields>] [filters]   # matches with excerpts, as JSON
kibela note view <note> [--json <fields>]                           # Markdown body of a note
kibela note comments <note>                                         # comments and inline comments, as JSON
kibela folder view <folder> [-L <limit>] [--json <fields>]          # a folder with its notes and subfolders, as JSON
kibela group view <group> [-L <limit>] [--json <fields>]            # a group with its top-level folders and notes, as JSON
kibela group list                                                   # groups, as JSON
kibela team add <team>
kibela team remove <team>
kibela team list
kibela team use <team>
```

`<note>` is a note number, a path (`/notes/<number>`), or a note URL. `<folder>` is a folder number, a path (`/folders/<number>`), or a folder URL. `<group>` is a group name, number, path (`/groups/<number>`), or URL. Commands that call the API accept `--team <team>`. `search` filters by `--sort`, `--updated`, `--resource`, `--archived`, `--coediting`, `--group`, `--folder`, `--user`, and `--liker`. `kibela <command> [<subcommand>] --help` describes each command's output and options and lists the fields `--json` accepts.

```sh
kibela search design review -L 5
kibela search release --sort recent --updated within-1-month --group Engineering
kibela note view https://example.kibe.la/notes/123 > note.md
kibela note view 123 --json title,url,author
kibela note comments 123
kibela folder view https://example.kibe.la/folders/45
```

## Output

Results go to standard output; errors and reports of what a command did, such as `team add`, go to standard error. The exit code is 0 on success, 1 on failure, and 2 on invalid arguments.

## Agent skill

[`skills/kibela-cli/SKILL.md`](skills/kibela-cli/SKILL.md) describes `kibela` for AI agents.

## License

[MIT](LICENSE)
