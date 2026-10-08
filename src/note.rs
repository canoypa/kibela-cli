use crate::config;

#[derive(Clone, Debug, PartialEq)]
pub struct NoteRef {
    pub team: Option<String>,
    pub path: String,
}

pub fn parse(arg: &str) -> Result<NoteRef, String> {
    if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(NoteRef {
            team: None,
            path: format!("/notes/{arg}"),
        });
    }
    let invalid = || {
        "neither a note number nor a note URL (https://<team>.kibe.la/notes/<number>)".to_string()
    };
    let rest = arg.strip_prefix("https://").ok_or_else(invalid)?;
    let (host, path) = rest.split_once('/').ok_or_else(invalid)?;
    let team = host.strip_suffix(".kibe.la").ok_or_else(invalid)?;
    let team = config::parse_team(team).map_err(|_| invalid())?;
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    if path.is_empty() {
        return Err(invalid());
    }
    Ok(NoteRef {
        team: Some(team),
        path: format!("/{path}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(team: Option<&str>, path: &str) -> NoteRef {
        NoteRef {
            team: team.map(String::from),
            path: path.into(),
        }
    }

    #[test]
    fn number() {
        assert_eq!(parse("123"), Ok(note(None, "/notes/123")));
    }

    #[test]
    fn url() {
        assert_eq!(
            parse("https://example.kibe.la/notes/123"),
            Ok(note(Some("example"), "/notes/123"))
        );
        assert_eq!(
            parse("https://example.kibe.la/notes/123?foo=1#comment_3"),
            Ok(note(Some("example"), "/notes/123"))
        );
        assert_eq!(
            parse("https://example.kibe.la/notes/123/"),
            Ok(note(Some("example"), "/notes/123"))
        );
    }

    #[test]
    fn invalid() {
        for arg in [
            "",
            "abc",
            "http://example.kibe.la/notes/1",
            "https://example.com/notes/1",
            "https://example.kibe.la/",
            "https://.kibe.la/notes/1",
            "https://evil.example#.kibe.la/notes/1",
            "https://user@evil.example.kibe.la/notes/1",
        ] {
            assert!(parse(arg).is_err(), "{arg}");
        }
    }
}
