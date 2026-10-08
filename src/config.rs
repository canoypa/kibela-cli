use std::{env, fs, io, path::PathBuf};

use serde_json::{Value, json};

#[derive(Default)]
pub struct Config {
    pub default_team: Option<String>,
    pub teams: Vec<String>,
}

pub fn parse_team(name: &str) -> Result<String, String> {
    let valid = (1..=63).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if valid {
        Ok(name.to_string())
    } else {
        Err(format!("{name} is not a valid team name"))
    }
}

fn path() -> Result<PathBuf, String> {
    let base = match env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
    {
        Some(dir) => dir,
        None => {
            let home = env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .ok_or("HOME is not set")?;
            PathBuf::from(home).join(".config")
        }
    };
    Ok(base.join("kibela").join("config.json"))
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let path = path()?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("failed to read {}: {e}", path.display())),
        };
        let value: Value = serde_json::from_str(&text)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        Ok(Self {
            default_team: value["defaultTeam"].as_str().map(String::from),
            teams: value["teams"]
                .as_array()
                .map(|teams| {
                    teams
                        .iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn resolve_team(
        &self,
        flag: Option<&str>,
        from_url: Option<&str>,
    ) -> Result<String, String> {
        match (flag, from_url) {
            (Some(flag), Some(url)) if flag != url => {
                return Err(format!(
                    "--team {flag} differs from the team in the URL ({url})"
                ));
            }
            (Some(team), _) | (None, Some(team)) => return Ok(team.to_string()),
            (None, None) => {}
        }
        if let Some(team) = &self.default_team {
            return Ok(team.clone());
        }
        match self.teams.as_slice() {
            [team] => Ok(team.clone()),
            [] => Err("no team is registered. Run `kibela token set <team>`".into()),
            _ => {
                Err("no default team. Run `kibela team use <team>` or pass `--team <team>`".into())
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = path()?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        }
        let value = json!({ "defaultTeam": self.default_team, "teams": self.teams });
        let text = serde_json::to_string_pretty(&value).expect("config is serializable") + "\n";
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text)
            .and_then(|()| fs::rename(&temp, &path))
            .map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    pub fn has_team(&self, team: &str) -> bool {
        self.teams.iter().any(|t| t == team)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(default_team: Option<&str>, teams: &[&str]) -> Config {
        Config {
            default_team: default_team.map(String::from),
            teams: teams.iter().map(|t| t.to_string()).collect(),
        }
    }

    #[test]
    fn team_names() {
        for name in ["example", "a", "my-team", "team2"] {
            assert_eq!(parse_team(name), Ok(name.into()));
        }
        for name in [
            "",
            "evil.example#",
            "evil.example/",
            "a.b",
            "-a",
            "a-",
            "a b",
            "user@host",
            &"a".repeat(64),
        ] {
            assert!(parse_team(name).is_err(), "{name}");
        }
    }

    #[test]
    fn flag_or_url_comes_first() {
        let c = config(Some("a"), &["a", "b"]);
        assert_eq!(c.resolve_team(Some("b"), None), Ok("b".into()));
        assert_eq!(c.resolve_team(None, Some("b")), Ok("b".into()));
        assert_eq!(c.resolve_team(Some("b"), Some("b")), Ok("b".into()));
        assert!(c.resolve_team(Some("a"), Some("b")).is_err());
    }

    #[test]
    fn then_the_default_team() {
        assert_eq!(
            config(Some("b"), &["a", "b"]).resolve_team(None, None),
            Ok("b".into())
        );
    }

    #[test]
    fn then_the_only_team() {
        assert_eq!(
            config(None, &["a"]).resolve_team(None, None),
            Ok("a".into())
        );
    }

    #[test]
    fn otherwise_an_error() {
        assert!(config(None, &[]).resolve_team(None, None).is_err());
        assert!(config(None, &["a", "b"]).resolve_team(None, None).is_err());
    }
}
