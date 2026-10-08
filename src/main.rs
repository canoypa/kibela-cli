mod api;
mod comments;
mod config;
mod note;
mod store;

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, builder::PossibleValuesParser};
use serde_json::json;

use config::Config;
use note::NoteRef;

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
    "path",
    "title",
    "titleHtml",
    "url",
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
    },
    /// Print all comments and inline comments of a note, with replies, as JSON
    ///
    /// Prints {"comments":{"totalCount":N,"nodes":[...]},"inlineComments":{...}}.
    /// Replies are in each comment's "replies". An inline comment's
    /// noteTextSelection.startLineInMarkdown is 0-based: it is line N+1 of the body
    /// printed by `kibela get`.
    Comments {
        /// Note number or URL (https://<team>.kibe.la/notes/<number>)
        #[arg(value_parser = note::parse)]
        note: NoteRef,
        /// Team to read from
        #[arg(long, value_parser = config::parse_team)]
        team: Option<String>,
    },
    /// Print the Markdown body of a note
    #[command(after_help = fields_help(NOTE_FIELDS))]
    Get {
        /// Note number or URL (https://<team>.kibe.la/notes/<number>)
        #[arg(value_parser = note::parse)]
        note: NoteRef,
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
    /// Manage API tokens
    #[command(subcommand)]
    Token(TokenCommand),
    /// Manage registered teams
    #[command(subcommand)]
    Team(TeamCommand),
}

#[derive(Subcommand)]
enum TokenCommand {
    /// Save a token for a team, read from standard input
    Set {
        #[arg(value_parser = config::parse_team)]
        team: String,
    },
    /// Delete the token of a team
    Delete {
        #[arg(value_parser = config::parse_team)]
        team: String,
    },
}

#[derive(Subcommand)]
enum TeamCommand {
    /// List registered teams; the default team is marked with `*`
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
        } => search(&query.join(" "), limit, team.as_deref(), json.as_deref()),
        Command::Get { note, team, json } => get(note, team.as_deref(), json.as_deref()),
        Command::Comments { note, team } => comments(note, team.as_deref()),
        Command::Token(TokenCommand::Set { team }) => token_set(&team),
        Command::Token(TokenCommand::Delete { team }) => token_delete(&team),
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

fn read_token() -> Result<String, String> {
    let token = if io::stdin().is_terminal() {
        rpassword::prompt_password("Token: ").map_err(|e| e.to_string())?
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
            format!("the token for {team} is invalid. Run `kibela token set {team}`")
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
        .ok_or_else(|| format!("no token is saved for {team}. Run `kibela token set {team}`"))?;
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

fn selection(fields: &[String]) -> String {
    let fields: Vec<&str> = fields
        .iter()
        .map(|field| match field.as_str() {
            "author" => "author { account realName }",
            field => field,
        })
        .collect();
    fields.join(" ")
}

fn search(
    text: &str,
    limit: u32,
    team_flag: Option<&str>,
    json: Option<&[String]>,
) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, None)?;
    let client = open_client(&team)?;
    let fields = json.map_or_else(|| "title url contentSummaryHtml".to_string(), selection);
    let query = format!(
        "query($query: String!, $first: Int!) {{ search(query: $query, first: $first) {{ totalCount nodes {{ {fields} }} }} }}"
    );
    let data = client
        .query(&query, json!({ "query": text, "first": limit }))
        .map_err(|e| describe(e, &team))?;
    output(&format!("{}\n", data["search"]))
}

fn get(note: NoteRef, team_flag: Option<&str>, json: Option<&[String]>) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, note.team.as_deref())?;
    let client = open_client(&team)?;
    let fields = json.map_or_else(|| "content".to_string(), selection);
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
        None => output(found["content"].as_str().unwrap_or_default()),
    }
}

fn comments(note: NoteRef, team_flag: Option<&str>) -> Result<(), String> {
    let team = Config::load()?.resolve_team(team_flag, note.team.as_deref())?;
    let client = open_client(&team)?;
    match comments::fetch(&client, &note.path) {
        Ok(Some(all)) => output(&format!("{all}\n")),
        Ok(None) => Err(format!("note {} was not found in {team}", note.path)),
        Err(e) => Err(describe(e, &team)),
    }
}

fn token_set(team: &str) -> Result<(), String> {
    let token = read_token()?;
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

    output(&format!("Saved token for {team} ({account})\n"))
}

fn token_delete(team: &str) -> Result<(), String> {
    let mut config = Config::load()?;
    if !config.has_team(team) {
        return Err(format!("team {team} is not registered"));
    }
    store::delete(team).map_err(|e| format!("cannot delete the token: {e}"))?;
    config.teams.retain(|t| t != team);
    if config.default_team.as_deref() == Some(team) {
        config.default_team = None;
    }
    config.save()?;
    output(&format!("Deleted token for {team}\n"))
}

fn team_list() -> Result<(), String> {
    let config = Config::load()?;
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
            "team {team} is not registered. Run `kibela token set {team}`"
        ));
    }
    config.default_team = Some(team.to_string());
    config.save()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
