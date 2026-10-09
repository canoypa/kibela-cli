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
    from_path(arg).or_else(|| from_url(arg)).ok_or_else(|| {
        "neither a note number, a path (/notes/<number>), nor a note URL \
         (https://<team>.kibe.la/notes/<number>)"
            .to_string()
    })
}

pub fn folder(arg: &str) -> Result<Location, String> {
    let location = if is_number(arg) {
        Location {
            team: None,
            path: format!("/folders/{arg}"),
        }
    } else {
        from_path(arg)
            .or_else(|| from_url(arg))
            .unwrap_or(Location {
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

#[derive(Clone, Debug, PartialEq)]
pub enum GroupKey {
    Path(String),
    Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub team: Option<String>,
    pub key: GroupKey,
}

pub fn group(arg: &str) -> Result<Group, String> {
    if is_number(arg) {
        return Ok(Group {
            team: None,
            key: GroupKey::Path(format!("/groups/{arg}")),
        });
    }
    let location = if arg.starts_with('/') {
        from_path(arg)
    } else if arg.starts_with("https://") {
        from_url(arg)
    } else if arg.is_empty() {
        None
    } else {
        return Ok(Group {
            team: None,
            key: GroupKey::Name(arg.to_string()),
        });
    };
    match location {
        Some(location)
            if location
                .path
                .strip_prefix("/groups/")
                .is_some_and(is_number) =>
        {
            Ok(Group {
                team: location.team,
                key: GroupKey::Path(location.path),
            })
        }
        _ => Err(
            "neither a group name, number, path (/groups/<number>), nor a group URL \
             (https://<team>.kibe.la/groups/<number>)"
                .to_string(),
        ),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub team: Option<String>,
    pub account: String,
}

pub fn user(arg: &str) -> Result<User, String> {
    let (team, path) = if arg.starts_with("https://") {
        match from_url(arg) {
            Some(location) => (location.team, location.path),
            None => (None, String::new()),
        }
    } else if arg.starts_with('/') {
        (None, arg.to_string())
    } else {
        (None, format!("/@{}", arg.strip_prefix('@').unwrap_or(arg)))
    };
    match path.strip_prefix("/@") {
        Some(account) if !account.is_empty() && !account.contains('/') => Ok(User {
            team,
            account: account.to_string(),
        }),
        _ => Err("neither an account, a path (/@<account>), nor a user URL \
             (https://<team>.kibe.la/@<account>)"
            .to_string()),
    }
}

fn is_number(arg: &str) -> bool {
    !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit())
}

fn from_path(arg: &str) -> Option<Location> {
    Some(Location {
        team: None,
        path: page_path(arg.strip_prefix('/')?)?,
    })
}

fn from_url(arg: &str) -> Option<Location> {
    let rest = arg.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let team = config::parse_team(host.strip_suffix(".kibe.la")?).ok()?;
    Some(Location {
        team: Some(team),
        path: page_path(path)?,
    })
}

/// The path of the page, without the query, the fragment (such as `#comment_3`), or a
/// trailing slash. `path` is what follows the first `/`.
fn page_path(path: &str) -> Option<String> {
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    (!path.is_empty()).then(|| format!("/{path}"))
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
    fn note_path() {
        assert_eq!(note("/notes/123"), Ok(location(None, "/notes/123")));
        assert_eq!(
            note("/notes/123#comment_3"),
            Ok(location(None, "/notes/123"))
        );
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
    fn group_name_number_path_and_url() {
        let path = |team: Option<&str>| Group {
            team: team.map(String::from),
            key: GroupKey::Path("/groups/6".into()),
        };
        assert_eq!(group("6"), Ok(path(None)));
        assert_eq!(group("/groups/6"), Ok(path(None)));
        assert_eq!(
            group("https://example.kibe.la/groups/6?tab=notes"),
            Ok(path(Some("example")))
        );
        assert_eq!(
            group("Design review"),
            Ok(Group {
                team: None,
                key: GroupKey::Name("Design review".into()),
            })
        );
        for arg in [
            "",
            "/groups/abc",
            "/notes/1",
            "https://example.kibe.la/notes/1",
            "https://example.com/groups/6",
        ] {
            assert!(group(arg).is_err(), "{arg}");
        }
    }

    #[test]
    fn user_account_path_and_url() {
        let user_in = |team: Option<&str>| User {
            team: team.map(String::from),
            account: "alice".into(),
        };
        for arg in ["alice", "@alice", "/@alice"] {
            assert_eq!(user(arg), Ok(user_in(None)), "{arg}");
        }
        assert_eq!(
            user("https://example.kibe.la/@alice?tab=notes"),
            Ok(user_in(Some("example")))
        );
        for arg in [
            "",
            "@",
            "/@",
            "/notes/1",
            "a/b",
            "https://example.kibe.la/notes/1",
        ] {
            assert!(user(arg).is_err(), "{arg}");
        }
    }

    #[test]
    fn invalid() {
        for arg in [
            "",
            "/",
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
