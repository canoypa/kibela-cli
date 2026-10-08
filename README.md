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

Save a Kibela access token for your team (`<team>` in `https://<team>.kibe.la`). The token is read from standard input, or prompted for without echo in a terminal:

```sh
kibela token set <team>
```

The token is checked against the API and stored in the OS keychain.

Commands pick the team in this order: `--team` or the team in a note URL, then the default team set with `kibela team use <team>`, then the only registered team.

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

`<note>` is a note number or a note URL. `<folder>` is a folder number, a path (`/folders/<number>`), or a folder URL. Commands that call the API accept `--team <team>`. `kibela <command> --help` describes each command's output and lists the fields `--json` accepts.

```sh
kibela search design review -L 5
kibela get https://example.kibe.la/notes/123 > note.md
kibela get 123 --json title,url,author
kibela comments 123
kibela folder https://example.kibe.la/folders/45
```

## Output

Results go to standard output and errors to standard error. The exit code is 0 on success, 1 on failure, and 2 on invalid arguments.

## Agent skill

[`skills/kibela-cli/SKILL.md`](skills/kibela-cli/SKILL.md) describes `kibela` for AI agents.

## License

[MIT](LICENSE)
