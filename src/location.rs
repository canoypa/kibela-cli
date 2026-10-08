use crate::config;

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub team: Option<String>,
    pub path: String,
}

pub fn note(arg: &str) -> Result<Location, String> {
    if is_number(arg) {
        return Ok(Location {
            team: None,
            path: format!("/notes/{arg}"),
        });
    }
    from_url(arg).ok_or_else(|| {
        "neither a note number nor a note URL (https://<team>.kibe.la/notes/<number>)".to_string()
    })
}

pub fn folder(arg: &str) -> Result<Location, String> {
    let location = if is_number(arg) {
        Location {
            team: None,
            path: format!("/folders/{arg}"),
        }
    } else if arg.starts_with('/') {
        Location {
            team: None,
            path: arg.to_string(),
        }
    } else {
        from_url(arg).unwrap_or(Location {
            team: None,
            path: String::new(),
        })
    };
    // Only /folders/<number> reaches folderFromPath; the /notes/folder/<name> path that
    // Folder.path returns is not found there.
    match location.path.strip_prefix("/folders/") {
        Some(number) if is_number(number) => Ok(location),
        _ => Err(
            "neither a folder number, a path (/folders/<number>), nor a folder URL \
             (https://<team>.kibe.la/folders/<number>). For a subfolder, pass its fixedPath"
                .to_string(),
        ),
    }
}

fn is_number(arg: &str) -> bool {
    !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit())
}

fn from_url(arg: &str) -> Option<Location> {
    let rest = arg.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let team = config::parse_team(host.strip_suffix(".kibe.la")?).ok()?;
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    Some(Location {
        team: Some(team),
        path: format!("/{path}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(team: Option<&str>, path: &str) -> Location {
        Location {
            team: team.map(String::from),
            path: path.into(),
        }
    }

    #[test]
    fn note_number() {
        assert_eq!(note("123"), Ok(location(None, "/notes/123")));
    }

    #[test]
    fn note_url() {
        assert_eq!(
            note("https://example.kibe.la/notes/123"),
            Ok(location(Some("example"), "/notes/123"))
        );
        assert_eq!(
            note("https://example.kibe.la/notes/123?foo=1#comment_3"),
            Ok(location(Some("example"), "/notes/123"))
        );
        assert_eq!(
            note("https://example.kibe.la/notes/123/"),
            Ok(location(Some("example"), "/notes/123"))
        );
    }

    #[test]
    fn folder_number_path_and_url() {
        assert_eq!(folder("45"), Ok(location(None, "/folders/45")));
        assert_eq!(folder("/folders/45"), Ok(location(None, "/folders/45")));
        assert_eq!(
            folder("https://example.kibe.la/folders/45?order_by=title&group_id=6"),
            Ok(location(Some("example"), "/folders/45"))
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
            assert!(note(arg).is_err(), "{arg}");
            assert!(folder(arg).is_err(), "{arg}");
        }
        for arg in [
            "/folders/",
            "/folders/abc",
            "/notes/1",
            "https://example.kibe.la/notes/1",
            "https://example.kibe.la/notes/folder/Team%2FDocs?group_id=6",
        ] {
            assert!(folder(arg).is_err(), "{arg}");
        }
    }
}
