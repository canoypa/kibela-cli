mod api;
mod comments;
mod config;
mod location;
mod lookup;
mod store;

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, Subcommand, ValueEnum, builder::PossibleValuesParser};
use serde_json::{Value, json};

use config::Config;
use location::Location;

#[derive(Parser)]
#[command(
    name = "kibela",
    version,
    about = "Read Kibela notes from the command line"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

const NOTE_FIELDS: &[&str] = &[
    "author",
    "canBeArchived",
    "canBeCommented",
    "canBeDestroyed",
    "canBeLiked",
    "canBeUpdated",
    "coediting",
    "commentsCount",
    "content",
    "contentHtml",
    "contentSummaryHtml",
    "contentTocHtml",
    "contentUpdatedAt",
    "createdAt",
    "databaseId",
    "editPath",
    "folderName",
    "hasCollabHistory",
    "id",
    "isArchived",
    "isLikedByCurrentUser",
    "path",
    "publishedAt",
    "title",
    "updatedAt",
    "url",
];

const SEARCH_FIELDS: &[&str] = &[
    "author",
    "contentSummaryHtml",
    "contentUpdatedAt",
    "folders",
    "groups",
    "path",
    "title",
    "titleHtml",
    "url",
];

const AUTHOR: (&str, &str) = ("author", "author { account realName }");

const NOTE_EXPANSIONS: &[(&str, &str)] = &[AUTHOR];

const SEARCH_EXPANSIONS: &[(&str, &str)] = &[
    AUTHOR,
    ("folders", "folders { fullName fixedPath }"),
    ("groups", "groups { name }"),
];

const FOLDER_EXPANSIONS: &[(&str, &str)] = &[
    ("group", "group { name }"),
    (
        "notes",
        "notes(first: $first) { totalCount nodes { title url } }",
    ),
    (
        "folders",
        "folders(first: $first) { totalCount nodes { name fixedPath } }",
    ),
];

const FOLDER_FIELDS: &[&str] = &[
    "activeChildrenCount",
    "aliveNotesCount",
    "alivePinnedNotesCount",
    "archivedAt",
    "canBeManaged",
    "createdAt",
    "fixedPath",
    "folders",
    "fullName",
    "group",
    "id",
    "lastModifiedAt",
    "name",
    "newNotePath",
    "notes",
    "path",
    "updatedAt",
];

const GROUP_EXPANSIONS: &[(&str, &str)] = &[
    (
        "notes",
        "notes(first: $first) { totalCount nodes { title url } }",
    ),
    // Without parentFolderId the API returns every folder in the group, nested ones included.
    (
        "folders",
        "folders(first: $first, parentFolderId: null) { totalCount nodes { fullName fixedPath } }",
    ),
];

const GROUP_FIELDS: &[&str] = &[
    "allowNoteExternalSharing",
    "archivedAt",
    "canBeJoinedBySelf",
    "canBeManaged",
    "coverImageKey",
    "createdAt",
    "description",
    "feedUpdatedAt",
    "folders",
    "id",
    "isArchived",
    "isDefault",
    "isJoined",
    "isNotePublicationApprovalRequired",
    "isPrivate",
    "isVisibilityChangeable",
    "name",
    "notes",
    "path",
    "updatedAt",
];

/// The folder fields without its connections, which would multiply `-L` into each folder.
const FOLDER_LIST_FIELDS: &[&str] = &[
    "activeChildrenCount",
    "aliveNotesCount",
    "alivePinnedNotesCount",
    "archivedAt",
    "canBeManaged",
    "createdAt",
    "fixedPath",
    "fullName",
    "group",
    "id",
    "lastModifiedAt",
    "name",
    "newNotePath",
    "path",
    "updatedAt",
];

const FOLDER_SEARCH_FIELDS: &[&str] = &["fixedPath", "folder", "group", "name"];

const FOLDER_SEARCH_EXPANSIONS: &[(&str, &str)] = &[
    ("folder", "folder { fullName fixedPath }"),
    ("group", "group { name }"),
];

fn fields_help(fields: &[&str]) -> String {
    let mut help = String::from("JSON FIELDS");
    let mut line = String::new();
    for (i, field) in fields.iter().enumerate() {
        let item = if i + 1 < fields.len() {
            format!("{field}, ")
        } else {
            field.to_string()
        };
        if line.len() + item.len() > 78 {
            help += &format!("\n  {}", line.trim_end());
            line.clear();
        }
        line += &item;
    }
    help + &format!("\n  {line}")
}

#[derive(Subcommand)]
enum Command {
    /// Search notes; prints matches with excerpts as JSON
    ///
    /// Prints {"totalCount":N,"nodes":[...]} with the top matches. When totalCount is
    /// larger than the number of nodes, raise --limit or narrow the query.
    /// In contentSummaryHtml, <em class="searchHighlight"> marks the matched words.
    #[command(after_help = fields_help(SEARCH_FIELDS))]
    Search {
        /// Words to search for
        #[arg(required = true)]
        query: Vec<String>,
        /// Maximum number of results
        #[arg(
            short = 'L',
            long,
            default_value_t = 10,
            value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX))
        )]
        limit: u32,
        /// Team to search
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields [default: title,url,contentSummaryHtml]
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(SEARCH_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
        #[command(flatten)]
        filters: Filters,
    },
    /// Read notes
    #[command(subcommand)]
    Note(NoteCommand),
    /// Read folders
    #[command(subcommand)]
    Folder(FolderCommand),
    /// Read groups
    #[command(subcommand)]
    Group(GroupCommand),
    /// Manage teams and their API tokens
    #[command(subcommand)]
    Team(TeamCommand),
}

#[derive(Subcommand)]
enum NoteCommand {
    /// List notes as JSON
    ///
    /// Prints {"totalCount":N,"nodes":[...]}. Without --folder or --group, lists the notes of
    /// the whole team. When totalCount is larger than the number of nodes, raise --limit.
    #[command(after_help = fields_help(NOTE_FIELDS))]
    List {
        /// Only notes in this folder (as `kibela folder view` takes it)
        #[arg(long, value_name = "FOLDER", value_parser = location::folder, conflicts_with = "group")]
        folder: Option<Location>,
        /// Only notes in this group (as `kibela group view` takes it)
        #[arg(long, value_name = "GROUP", value_parser = location::group)]
        group: Option<location::Group>,
        /// With --group, only notes that are in no folder
        #[arg(long, requires = "group")]
        not_in_folder: bool,
        /// Field to order by
        #[arg(long, value_enum, default_value_t = NoteSort::ContentUpdatedAt)]
        sort: NoteSort,
        /// Direction of the order
        #[arg(long, value_enum, default_value_t = Direction::Desc)]
        order: Direction,
        /// Which notes to list by whether they are archived
        #[arg(long, value_enum, default_value_t = State::Active)]
        state: State,
        /// Maximum number of notes
        #[arg(
            short = 'L',
            long,
            default_value_t = 10,
            value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX))
        )]
        limit: u32,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields [default: title,url]
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(NOTE_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
    },
    /// Print the Markdown body of a note
    #[command(after_help = fields_help(NOTE_FIELDS))]
    View {
        /// Note number, path (/notes/<number>), or URL (https://<team>.kibe.la/notes/<number>)
        #[arg(value_parser = location::note)]
        note: Location,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(NOTE_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
    },
    /// Print all comments and inline comments of a note, with replies, as JSON
    ///
    /// Prints {"comments":{"totalCount":N,"nodes":[...]},"inlineComments":{...}}.
    /// Replies are in each comment's "replies". An inline comment's
    /// noteTextSelection.startLineInMarkdown is 0-based: it is line N+1 of the body
    /// printed by `kibela note view`.
    Comments {
        /// Note number, path (/notes/<number>), or URL (https://<team>.kibe.la/notes/<number>)
        #[arg(value_parser = location::note)]
        note: Location,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
enum FolderCommand {
    /// Search folders by name; prints matches as JSON
    ///
    /// Prints {"totalCount":N,"nodes":[...]} with the top matches. A folder's fixedPath goes to
    /// `kibela folder view`. When totalCount is larger than the number of nodes, raise --limit.
    /// In name, the full name of the folder, <em class="searchHighlight"> marks the matched words.
    #[command(after_help = fields_help(FOLDER_SEARCH_FIELDS))]
    Search {
        /// Words to search for
        #[arg(required = true)]
        query: Vec<String>,
        /// Maximum number of results
        #[arg(
            short = 'L',
            long,
            default_value_t = 10,
            value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX))
        )]
        limit: u32,
        /// Team to search
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields [default: folder,group]
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(FOLDER_SEARCH_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
    },
    /// List folders as JSON
    ///
    /// Prints {"totalCount":N,"nodes":[...]}. Lists every folder, nested ones included, unless
    /// --root or --parent narrows it. A folder's fixedPath (/folders/<number>) can be passed to
    /// `kibela folder view`. When totalCount is larger than the number of nodes, raise --limit.
    #[command(after_help = fields_help(FOLDER_LIST_FIELDS))]
    List {
        /// Only folders in this group (as `kibela group view` takes it)
        #[arg(long, value_name = "GROUP", value_parser = location::group)]
        group: Option<location::Group>,
        /// Only top-level folders
        #[arg(long, conflicts_with = "parent")]
        root: bool,
        /// Only the subfolders of this folder (as `kibela folder view` takes it)
        #[arg(long, value_name = "FOLDER", value_parser = location::folder)]
        parent: Option<Location>,
        /// With --group, also list the folders below the ones listed
        #[arg(long, requires = "group")]
        with_children: bool,
        /// Which folders to list by whether they are archived
        #[arg(long, value_enum, default_value_t = State::Active)]
        state: State,
        /// Maximum number of folders
        #[arg(
            short = 'L',
            long,
            default_value_t = 10,
            value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX))
        )]
        limit: u32,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields [default: fullName,fixedPath]
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(FOLDER_LIST_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
    },
    /// Print a folder with its notes and subfolders as JSON
    ///
    /// Prints {"fullName":...,"notes":{"totalCount":N,"nodes":[...]},"folders":{...}}.
    /// A subfolder's fixedPath (/folders/<number>) can be passed back to `kibela folder view`.
    /// When a totalCount is larger than the number of nodes, raise --limit.
    #[command(after_help = fields_help(FOLDER_FIELDS))]
    View {
        /// Folder number, path (/folders/<number>), or URL (https://<team>.kibe.la/folders/<number>)
        #[arg(value_parser = location::folder)]
        folder: Location,
        /// Maximum number of notes and of subfolders
        #[arg(
            short = 'L',
            long,
            default_value_t = 10,
            value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX))
        )]
        limit: u32,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields [default: fullName,notes,folders]
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(FOLDER_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
    },
}

#[derive(Args)]
struct Filters {
    /// Order of the results
    #[arg(long, value_enum, default_value_t = Sort::Relevant)]
    sort: Sort,
    /// Only results updated within this period
    #[arg(long, value_enum)]
    updated: Option<Updated>,
    /// Kind of results; repeat for more
    #[arg(long = "resource", value_enum)]
    resources: Vec<Resource>,
    /// Only archived notes
    #[arg(long)]
    archived: bool,
    /// Only co-edited notes
    #[arg(long)]
    coediting: bool,
    /// Only results in this group (name, number, path, or URL); repeat for more
    #[arg(long = "group", value_name = "GROUP", value_parser = location::group)]
    groups: Vec<location::Group>,
    /// Only results in this folder (as `kibela folder view` takes it); repeat for more
    #[arg(long = "folder", value_name = "FOLDER", value_parser = location::folder)]
    folders: Vec<Location>,
    /// Only results written by this account; repeat for more
    #[arg(long = "user", value_name = "ACCOUNT")]
    users: Vec<String>,
    /// Only results liked by this account; repeat for more
    #[arg(long = "liker", value_name = "ACCOUNT")]
    likers: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum NoteSort {
    ContentUpdatedAt,
    Title,
    PublishedAt,
}

#[derive(Clone, Copy, ValueEnum)]
enum State {
    Active,
    Archived,
    All,
}

impl State {
    /// The API's `active` argument; null selects both.
    fn active(self) -> Value {
        match self {
            State::Active => json!(true),
            State::Archived => json!(false),
            State::All => Value::Null,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Direction {
    Asc,
    Desc,
}

#[derive(Clone, Copy, ValueEnum)]
enum Sort {
    Relevant,
    Recent,
}

#[derive(Clone, Copy, ValueEnum)]
enum Updated {
    #[value(name = "within-3-days")]
    Within3Days,
    #[value(name = "within-1-week")]
    Within1Week,
    #[value(name = "within-1-month")]
    Within1Month,
    #[value(name = "within-6-months")]
    Within6Months,
    #[value(name = "within-1-year")]
    Within1Year,
}

#[derive(Clone, Copy, ValueEnum)]
enum Resource {
    Note,
    Comment,
    Attachment,
}

fn api_enum(value: impl ValueEnum) -> String {
    let name = value.to_possible_value().expect("no value is skipped");
    name.get_name().replace('-', "_").to_uppercase()
}

#[derive(Subcommand)]
enum GroupCommand {
    /// Print a group with its top-level folders and its notes as JSON
    ///
    /// Prints {"name":...,"description":...,"folders":{"totalCount":N,"nodes":[...]},"notes":{...}}.
    /// A folder's fixedPath (/folders/<number>) can be passed to `kibela folder view`.
    /// When a totalCount is larger than the number of nodes, raise --limit.
    #[command(after_help = fields_help(GROUP_FIELDS))]
    View {
        /// Group name, number, path (/groups/<number>), or URL (https://<team>.kibe.la/groups/<number>)
        #[arg(value_parser = location::group)]
        group: location::Group,
        /// Maximum number of folders and of notes
        #[arg(
            short = 'L',
            long,
            default_value_t = 10,
            value_parser = clap::value_parser!(u32).range(1..=i64::from(i32::MAX))
        )]
        limit: u32,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
        /// Output JSON with the specified fields [default: name,description,folders,notes]
        #[arg(
            long,
            value_name = "FIELDS",
            value_delimiter = ',',
            value_parser = PossibleValuesParser::new(GROUP_FIELDS),
            hide_possible_values = true
        )]
        json: Option<Vec<String>>,
    },
    /// List groups, archived ones included, as JSON; a name or path goes to `group view` and --group
    List {
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
enum TeamCommand {
    /// Add a team with an access token, read from standard input
    ///
    /// Kibela's API needs an access token for each team. Create one at
    /// https://<team>.kibe.la/settings/access_tokens and give it on standard input; in a
    /// terminal it is prompted for without echo. The token is checked against the API and
    /// stored in the OS keychain. Adding a team again replaces its token.
    #[command(
        after_help = "EXAMPLES\n  kibela team add example\n  pbpaste | kibela team add example"
    )]
    Add {
        /// Team name, as in https://<team>.kibe.la
        #[arg(value_parser = config::parse_team)]
        team: String,
    },
    /// Remove a team and its token
    Remove {
        #[arg(value_parser = config::parse_team)]
        team: String,
    },
    /// List added teams; the default team is marked with `*`
    List,
    /// Set the default team
    Use {
        #[arg(value_parser = config::parse_team)]
        team: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Search {
            query,
            limit,
            team,
            json,
            filters,
        } => search(
            &query.join(" "),
            limit,
            team.as_deref(),
            json.as_deref(),
            &filters,
        ),
        Command::Note(NoteCommand::View { note, team, json }) => {
            note_view(note, team.as_deref(), json.as_deref())
        }
        Command::Note(NoteCommand::List {
            folder,
            group,
            not_in_folder,
            sort,
            order,
            state,
            limit,
            team,
            json,
        }) => note_list(
            NoteFilters {
                folder,
                group,
                not_in_folder,
                sort,
                order,
                state,
            },
            limit,
            team.as_deref(),
            json.as_deref(),
        ),
        Command::Note(NoteCommand::Comments { note, team }) => note_comments(note, team.as_deref()),
        Command::Folder(FolderCommand::View {
            folder: target,
            limit,
            team,
            json,
        }) => folder_view(target, limit, team.as_deref(), json.as_deref()),
        Command::Folder(FolderCommand::Search {
            query,
            limit,
            team,
            json,
        }) => folder_search(&query.join(" "), limit, team.as_deref(), json.as_deref()),
        Command::Folder(FolderCommand::List {
            group,
            root,
            parent,
            with_children,
            state,
            limit,
            team,
            json,
        }) => folder_list(
            FolderFilters {
                group,
                root,
                parent,
                with_children,
                state,
            },
            limit,
            team.as_deref(),
            json.as_deref(),
        ),
        Command::Group(GroupCommand::View {
            group,
            limit,
            team,
            json,
        }) => group_view(group, limit, team.as_deref(), json.as_deref()),
        Command::Group(GroupCommand::List { team }) => group_list(team.as_deref()),
        Command::Team(TeamCommand::Add { team }) => team_add(&team),
        Command::Team(TeamCommand::Remove { team }) => team_remove(&team),
        Command::Team(TeamCommand::List) => team_list(),
        Command::Team(TeamCommand::Use { team }) => team_use(&team),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(io::stderr(), "error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn read_token(team: &str) -> Result<String, String> {
    let token = if io::stdin().is_terminal() {
        let _ = writeln!(
            io::stderr(),
            "kibela reads {team} with an access token. Create one at \
             https://{team}.kibe.la/settings/access_tokens and paste it here."
        );
        rpassword::prompt_password("Access token: ").map_err(|e| e.to_string())?
    } else {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| e.to_string())?;
        input
    };
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("no token was given".into());
    }
    Ok(token)
}

fn describe(error: api::Error, team: &str) -> String {
    match error {
        api::Error::Unauthorized => {
            format!("the token for {team} is invalid. Run `kibela team add {team}`")
        }
        api::Error::TeamNotFound => format!("team {team} was not found. Check the team name"),
        api::Error::NotFound => "not found".into(),
        api::Error::TeamBudgetExhausted { wait } => format!(
            "the hourly API budget of {team} is used up. Retry after {}",
            format_utc(SystemTime::now() + wait)
        ),
        api::Error::Other(message) => message,
    }
}

fn open_client(team: &str) -> Result<api::Client, String> {
    let token = store::get(team)
        .map_err(|e| format!("cannot read the token: {e}"))?
        .ok_or_else(|| format!("no token is saved for {team}. Run `kibela team add {team}`"))?;
    api::Client::new(team, &token).map_err(|e| describe(e, team))
}

fn format_utc(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Reports what a command did on stderr, keeping stdout for results.
fn report(message: &str) -> Result<(), String> {
    let _ = writeln!(io::stderr(), "{message}");
    Ok(())
}

fn output(text: &str) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    match stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
    {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e.to_string()),
        _ => Ok(()),
    }
}

/// The GraphQL selection for `--json` fields; object fields expand to the subfields we return.
fn selection<S: AsRef<str>>(fields: &[S], expansions: &[(&str, &str)]) -> String {
    let fields: Vec<&str> = fields
        .iter()
        .map(|field| {
            let field = field.as_ref();
            expansions
                .iter()
                .find(|(name, _)| *name == field)
                .map_or(field, |(_, expanded)| expanded)
        })
        .collect();
    fields.join(" ")
}

fn single_url_team<'a>(
    teams: impl IntoIterator<Item = Option<&'a str>>,
) -> Result<Option<&'a str>, String> {
    let mut teams = teams.into_iter().flatten();
    let first = teams.next();
    match teams.find(|team| Some(*team) != first) {
        Some(other) => Err(format!(
            "the URLs point to different teams ({} and {other})",
            first.unwrap_or_default()
        )),
        None => Ok(first),
    }
}

fn search(
    text: &str,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
    filters: &Filters,
) -> Result<(), String> {
    let url_team = single_url_team(
        (filters.folders.iter().map(|f| f.team.as_deref()))
            .chain(filters.groups.iter().map(|g| g.team.as_deref())),
    )?;
    let team = Config::load()?.resolve_team(team_flag, url_team)?;
    let client = open_client(&team)?;
    let api_error = |e| describe(e, &team);

    let mut variables = json!({
        "query": text,
        "first": limit,
        "sortBy": api_enum(filters.sort),
        "isArchived": filters.archived,
    });
    if let Some(updated) = filters.updated {
        variables["updated"] = json!(api_enum(updated));
    }
    if !filters.resources.is_empty() {
        let kinds: Vec<String> = filters.resources.iter().map(|&r| api_enum(r)).collect();
        variables["resources"] = json!(kinds);
    }
    if filters.coediting {
        variables["coediting"] = json!(true);
    }
    if !filters.groups.is_empty() {
        let groups = lookup::groups(&client).map_err(api_error)?;
        let ids = filters
            .groups
            .iter()
            .map(|group| lookup::group_id(&groups, &group.key))
            .collect::<Result<Vec<_>, _>>()?;
        variables["groupIds"] = json!(ids);
    }
    if !filters.folders.is_empty() {
        let mut ids = Vec::new();
        for folder in &filters.folders {
            let id = lookup::folder_id(&client, &folder.path).map_err(api_error)?;
            ids.push(id.ok_or_else(|| format!("folder {} was not found in {team}", folder.path))?);
        }
        variables["folderIds"] = json!(ids);
    }
    for (accounts, key) in [(&filters.users, "userIds"), (&filters.likers, "likerIds")] {
        if accounts.is_empty() {
            continue;
        }
        let mut ids = Vec::new();
        for account in accounts {
            let id = lookup::user_id(&client, account).map_err(api_error)?;
            ids.push(id.ok_or_else(|| format!("user {account} was not found in {team}"))?);
        }
        variables[key] = json!(ids);
    }

    let fields = json.map_or_else(
        || "title url contentSummaryHtml".to_string(),
        |fields| selection(fields, SEARCH_EXPANSIONS),
    );
    let query = format!(
        "query($query: String!, $first: Int!, $sortBy: SearchSortKind, $isArchived: Boolean, \
           $updated: SearchDate, $resources: [SearchResourceKind!], $coediting: Boolean, \
           $groupIds: [ID!], $folderIds: [ID!], $userIds: [ID!], $likerIds: [ID!]) {{ \
           search(query: $query, first: $first, sortBy: $sortBy, isArchived: $isArchived, \
             updated: $updated, resources: $resources, coediting: $coediting, groupIds: $groupIds, \
             folderIds: $folderIds, userIds: $userIds, likerIds: $likerIds) {{ \
             totalCount nodes {{ {fields} }} }} }}"
    );
    let data = client.query(&query, variables).map_err(api_error)?;
    output(&format!("{}\n", data["search"]))
}

fn note_view(
    note: Location,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, note.team.as_deref())?;
    let client = open_client(&team)?;
    let fields = json.map_or_else(
        || "content".to_string(),
        |fields| selection(fields, NOTE_EXPANSIONS),
    );
    let query = format!("query($path: String!) {{ noteFromPath(path: $path) {{ {fields} }} }}");
    let not_found = || format!("note {} was not found in {team}", note.path);
    let data = client
        .query(&query, json!({ "path": note.path }))
        .map_err(|e| match e {
            api::Error::NotFound => not_found(),
            e => describe(e, &team),
        })?;
    let found = &data["noteFromPath"];
    if found.is_null() {
        return Err(not_found());
    }
    match json {
        Some(_) => output(&format!("{found}\n")),
        None => {
            let content = found["content"].as_str().unwrap_or_default();
            if content.is_empty() || content.ends_with('\n') {
                output(content)
            } else {
                output(&format!("{content}\n"))
            }
        }
    }
}

struct NoteFilters {
    folder: Option<Location>,
    group: Option<location::Group>,
    not_in_folder: bool,
    sort: NoteSort,
    order: Direction,
    state: State,
}

fn note_list(
    filters: NoteFilters,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let url_team = filters.folder.as_ref().and_then(|f| f.team.as_deref());
    let url_team = url_team.or(filters.group.as_ref().and_then(|g| g.team.as_deref()));
    let team = Config::load()?.resolve_team(team_flag, url_team)?;
    let client = open_client(&team)?;
    let api_error = |e| describe(e, &team);

    let mut variables = json!({
        "first": limit,
        "orderBy": { "field": api_enum(filters.sort), "direction": api_enum(filters.order) },
        "active": filters.state.active(),
    });
    if filters.not_in_folder {
        variables["onlyNotAttachedFolder"] = json!(true);
    }
    if let Some(folder) = &filters.folder {
        let id = lookup::folder_id(&client, &folder.path).map_err(api_error)?;
        variables["folderId"] =
            id.ok_or_else(|| format!("folder {} was not found in {team}", folder.path))?;
    }
    if let Some(group) = &filters.group {
        let groups = lookup::groups(&client).map_err(api_error)?;
        variables["id"] = lookup::group_id(&groups, &group.key)?;
    }

    let fields = json.map_or_else(
        || "title url".to_string(),
        |fields| selection(fields, NOTE_EXPANSIONS),
    );
    let data = client
        .query(
            &note_list_query(filters.group.is_some(), &fields),
            variables,
        )
        .map_err(api_error)?;
    let notes = match filters.group {
        Some(_) => &data["group"]["notes"],
        None => &data["notes"],
    };
    output(&format!("{notes}\n"))
}

/// The notes of a group come from `Group.notes`, which takes no folder; the others from `notes`.
fn note_list_query(in_group: bool, fields: &str) -> String {
    let connection = format!("totalCount nodes {{ {fields} }}");
    if in_group {
        format!(
            "query($id: ID!, $first: Int!, $orderBy: NoteOrder, $active: Boolean, \
               $onlyNotAttachedFolder: Boolean) {{ group(id: $id) {{ \
               notes(first: $first, orderBy: $orderBy, active: $active, \
                 onlyNotAttachedFolder: $onlyNotAttachedFolder) {{ {connection} }} }} }}"
        )
    } else {
        format!(
            "query($first: Int!, $orderBy: NoteOrder, $active: Boolean, $folderId: ID) {{ \
               notes(first: $first, orderBy: $orderBy, active: $active, folderId: $folderId) \
               {{ {connection} }} }}"
        )
    }
}

fn note_comments(note: Location, team_flag: Option<&str>) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, note.team.as_deref())?;
    let client = open_client(&team)?;
    match comments::fetch(&client, &note.path) {
        Ok(Some(all)) => output(&format!("{all}\n")),
        Ok(None) => Err(format!("note {} was not found in {team}", note.path)),
        Err(e) => Err(describe(e, &team)),
    }
}

fn folder_search(
    text: &str,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, None)?;
    let client = open_client(&team)?;
    let fields = selection(
        json.unwrap_or(&["folder".into(), "group".into()]),
        FOLDER_SEARCH_EXPANSIONS,
    );
    let query = format!(
        "query($query: String!, $first: Int!) {{ searchFolder(query: $query, first: $first) {{ \
           totalCount nodes {{ {fields} }} }} }}"
    );
    let data = client
        .query(&query, json!({ "query": text, "first": limit }))
        .map_err(|e| describe(e, &team))?;
    output(&format!("{}\n", data["searchFolder"]))
}

struct FolderFilters {
    group: Option<location::Group>,
    root: bool,
    parent: Option<Location>,
    with_children: bool,
    state: State,
}

fn folder_list(
    filters: FolderFilters,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let url_team = single_url_team([
        filters.group.as_ref().and_then(|g| g.team.as_deref()),
        filters.parent.as_ref().and_then(|p| p.team.as_deref()),
    ])?;
    let team = Config::load()?.resolve_team(team_flag, url_team)?;
    let client = open_client(&team)?;
    let api_error = |e| describe(e, &team);

    let mut variables = json!({ "first": limit, "active": filters.state.active() });
    // Left out, parentFolderId lists every folder; null lists the top-level ones.
    if filters.root {
        variables["parentFolderId"] = Value::Null;
    }
    if let Some(parent) = &filters.parent {
        let id = lookup::folder_id(&client, &parent.path).map_err(api_error)?;
        variables["parentFolderId"] =
            id.ok_or_else(|| format!("folder {} was not found in {team}", parent.path))?;
    }
    if filters.with_children {
        variables["withChildren"] = json!(true);
    }
    if let Some(group) = &filters.group {
        let groups = lookup::groups(&client).map_err(api_error)?;
        variables["id"] = lookup::group_id(&groups, &group.key)?;
    }

    let fields = json.map_or_else(
        || "fullName fixedPath".to_string(),
        |fields| selection(fields, FOLDER_EXPANSIONS),
    );
    let data = client
        .query(
            &folder_list_query(filters.group.is_some(), &fields),
            variables,
        )
        .map_err(api_error)?;
    let folders = match filters.group {
        Some(_) => &data["group"]["folders"],
        None => &data["folders"],
    };
    output(&format!("{folders}\n"))
}

/// Only `Group.folders` takes withChildren.
fn folder_list_query(in_group: bool, fields: &str) -> String {
    let connection = format!("totalCount nodes {{ {fields} }}");
    if in_group {
        format!(
            "query($id: ID!, $first: Int!, $active: Boolean, $parentFolderId: ID, \
               $withChildren: Boolean) {{ group(id: $id) {{ folders(first: $first, \
                 active: $active, parentFolderId: $parentFolderId, withChildren: $withChildren) \
                 {{ {connection} }} }} }}"
        )
    } else {
        format!(
            "query($first: Int!, $active: Boolean, $parentFolderId: ID) {{ \
               folders(first: $first, active: $active, parentFolderId: $parentFolderId) \
               {{ {connection} }} }}"
        )
    }
}

fn folder_view(
    target: Location,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, target.team.as_deref())?;
    let client = open_client(&team)?;
    let fields: Vec<&str> = match json {
        Some(fields) => fields.iter().map(String::as_str).collect(),
        None => vec!["fullName", "notes", "folders"],
    };
    let (query, uses_first) = view_query(
        "$path: String!",
        "folderFromPath(path: $path)",
        &fields,
        FOLDER_EXPANSIONS,
    );
    let variables = if uses_first {
        json!({ "path": target.path, "first": limit })
    } else {
        json!({ "path": target.path })
    };
    let not_found = || format!("folder {} was not found in {team}", target.path);
    let data = client.query(&query, variables).map_err(|e| match e {
        api::Error::NotFound => not_found(),
        e => describe(e, &team),
    })?;
    let found = &data["folderFromPath"];
    if found.is_null() {
        return Err(not_found());
    }
    output(&format!("{found}\n"))
}

/// The query for the selected fields of `root`, and whether it takes `$first`. GraphQL rejects
/// a declared variable that the selection does not use.
fn view_query(
    params: &str,
    root: &str,
    fields: &[&str],
    expansions: &[(&str, &str)],
) -> (String, bool) {
    let selection = selection(fields, expansions);
    let uses_first = selection.contains("$first");
    let first = if uses_first { ", $first: Int!" } else { "" };
    let query = format!("query({params}{first}) {{ {root} {{ {selection} }} }}");
    (query, uses_first)
}

fn group_view(
    group: location::Group,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, group.team.as_deref())?;
    let client = open_client(&team)?;
    let groups = lookup::groups(&client).map_err(|e| describe(e, &team))?;
    let id = lookup::group_id(&groups, &group.key)?;
    let fields: Vec<&str> = match json {
        Some(fields) => fields.iter().map(String::as_str).collect(),
        None => vec!["name", "description", "folders", "notes"],
    };
    let (query, uses_first) = view_query("$id: ID!", "group(id: $id)", &fields, GROUP_EXPANSIONS);
    let variables = if uses_first {
        json!({ "id": id, "first": limit })
    } else {
        json!({ "id": id })
    };
    let data = client
        .query(&query, variables)
        .map_err(|e| describe(e, &team))?;
    output(&format!("{}\n", data["group"]))
}

fn group_list(team_flag: Option<&str>) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, None)?;
    let client = open_client(&team)?;
    let mut groups = lookup::groups(&client).map_err(|e| describe(e, &team))?;
    if let Some(nodes) = groups["nodes"].as_array_mut() {
        for node in nodes.iter_mut().filter_map(Value::as_object_mut) {
            node.shift_remove("id");
        }
    }
    output(&format!("{groups}\n"))
}

fn team_add(team: &str) -> Result<(), String> {
    let token = read_token(team)?;
    let client = api::Client::new(team, &token).map_err(|e| describe(e, team))?;
    let data = match client.query("query { currentUser { account realName } }", json!({})) {
        Ok(data) => data,
        Err(api::Error::Unauthorized) => {
            return Err(format!("the given token is invalid for {team}"));
        }
        Err(e) => return Err(describe(e, team)),
    };
    let account = data["currentUser"]["account"].as_str().unwrap_or_default();

    let mut config = Config::load()?;
    store::set(team, &token).map_err(|e| format!("cannot save the token: {e}"))?;
    if !config.has_team(team) {
        config.teams.push(team.to_string());
        config.save()?;
    }

    report(&format!("Added {team} ({account})"))
}

fn team_remove(team: &str) -> Result<(), String> {
    let mut config = Config::load()?;
    if !config.has_team(team) {
        return Err(format!("team {team} is not added"));
    }
    store::delete(team).map_err(|e| format!("cannot delete the token: {e}"))?;
    config.teams.retain(|t| t != team);
    if config.default_team.as_deref() == Some(team) {
        config.default_team = None;
    }
    config.save()?;
    report(&format!("Removed {team}"))
}

fn team_list() -> Result<(), String> {
    let config = Config::load()?;
    if config.teams.is_empty() {
        return report("no team is added. Run `kibela team add <team>`");
    }
    let mut list = String::new();
    for team in &config.teams {
        let mark = if config.default_team.as_deref() == Some(team.as_str()) {
            "*"
        } else {
            " "
        };
        list += &format!("{mark} {team}\n");
    }
    output(&list)
}

fn team_use(team: &str) -> Result<(), String> {
    let mut config = Config::load()?;
    if !config.has_team(team) {
        return Err(format!(
            "team {team} is not added. Run `kibela team add {team}`"
        ));
    }
    config.default_team = Some(team.to_string());
    config.save()?;
    report(&format!("Set {team} as the default team"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn filter_values_name_the_api_enums() {
        assert_eq!(api_enum(Sort::Relevant), "RELEVANT");
        assert_eq!(api_enum(Sort::Recent), "RECENT");
        assert_eq!(api_enum(Updated::Within3Days), "WITHIN_3_DAYS");
        assert_eq!(api_enum(Updated::Within6Months), "WITHIN_6_MONTHS");
        assert_eq!(api_enum(Resource::Attachment), "ATTACHMENT");
        assert_eq!(api_enum(NoteSort::ContentUpdatedAt), "CONTENT_UPDATED_AT");
        assert_eq!(api_enum(NoteSort::PublishedAt), "PUBLISHED_AT");
        assert_eq!(api_enum(Direction::Asc), "ASC");
    }

    #[test]
    fn each_command_expands_its_own_object_fields() {
        assert_eq!(
            selection(&["title", "folders", "groups"], SEARCH_EXPANSIONS),
            "title folders { fullName fixedPath } groups { name }"
        );
        assert_eq!(
            selection(&["author", "content"], NOTE_EXPANSIONS),
            "author { account realName } content"
        );
    }

    #[test]
    fn view_query_declares_first_only_for_connections() {
        let folder = |fields: &[&str]| {
            view_query(
                "$path: String!",
                "folderFromPath(path: $path)",
                fields,
                FOLDER_EXPANSIONS,
            )
        };
        let (query, uses_first) = folder(&["fullName", "notes", "folders"]);
        assert!(uses_first);
        assert_eq!(
            query,
            "query($path: String!, $first: Int!) { folderFromPath(path: $path) { fullName \
             notes(first: $first) { totalCount nodes { title url } } \
             folders(first: $first) { totalCount nodes { name fixedPath } } } }"
        );
        let (query, uses_first) = folder(&["name", "group"]);
        assert!(!uses_first);
        assert_eq!(
            query,
            "query($path: String!) { folderFromPath(path: $path) { name group { name } } }"
        );
    }

    #[test]
    fn group_folders_are_the_top_level_ones() {
        let (query, uses_first) =
            view_query("$id: ID!", "group(id: $id)", &["folders"], GROUP_EXPANSIONS);
        assert!(uses_first);
        assert_eq!(
            query,
            "query($id: ID!, $first: Int!) { group(id: $id) { \
             folders(first: $first, parentFolderId: null) { totalCount nodes { fullName fixedPath } } } }"
        );
    }

    #[test]
    fn urls_must_share_a_team() {
        assert_eq!(single_url_team([]), Ok(None));
        assert_eq!(single_url_team([None, Some("a"), Some("a")]), Ok(Some("a")));
        assert!(single_url_team([Some("a"), Some("b")]).is_err());
    }

    #[test]
    fn utc_dates() {
        assert_eq!(format_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            format_utc(UNIX_EPOCH + Duration::from_secs(951_782_400)),
            "2000-02-29T00:00:00Z"
        );
        assert_eq!(
            format_utc(UNIX_EPOCH + Duration::from_secs(1_791_331_199)),
            "2026-10-06T23:59:59Z"
        );
    }
}
