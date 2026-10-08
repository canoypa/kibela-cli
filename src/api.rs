use std::{
    cell::Cell,
    thread,
    time::{Duration, Instant},
};

use reqwest::{
    StatusCode, blocking,
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
};
use serde_json::{Value, json};

const MIN_INTERVAL: Duration = Duration::from_millis(100);
const MAX_RETRIES: usize = 3;

#[derive(Debug, PartialEq)]
pub enum Error {
    Unauthorized,
    TeamNotFound,
    NotFound,
    TeamBudgetExhausted { wait: Duration },
    Other(String),
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Data(Value),
    Retry { wait: Duration, reason: String },
    Fail(Error),
}

pub struct Client {
    team: String,
    token: String,
    http: blocking::Client,
    last_request: Cell<Option<Instant>>,
}

impl Client {
    pub fn new(team: &str, token: &str) -> Result<Self, Error> {
        // Kibela answers an unknown team with a redirect; following it would hide that.
        let http = blocking::Client::builder()
            .redirect(Policy::none())
            .user_agent(concat!("kibela-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self {
            team: team.to_string(),
            token: token.to_string(),
            http,
            last_request: Cell::new(None),
        })
    }

    pub fn query(&self, query: &str, variables: Value) -> Result<Value, Error> {
        let body = json!({ "query": query, "variables": variables }).to_string();
        let mut retries = 0;
        loop {
            match self.send(&body)? {
                Outcome::Data(data) => return Ok(data),
                Outcome::Fail(error) => return Err(error),
                Outcome::Retry { wait, reason } => {
                    if retries == MAX_RETRIES {
                        return Err(Error::Other(format!(
                            "{reason} (gave up after {MAX_RETRIES} retries)"
                        )));
                    }
                    retries += 1;
                    thread::sleep(wait);
                }
            }
        }
    }

    fn send(&self, body: &str) -> Result<Outcome, Error> {
        if let Some(last) = self.last_request.get() {
            thread::sleep(MIN_INTERVAL.saturating_sub(last.elapsed()));
        }
        self.last_request.set(Some(Instant::now()));

        let team = &self.team;
        let response = self
            .http
            .post(format!("https://{team}.kibe.la/api/v1"))
            .bearer_auth(&self.token)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .body(body.to_string())
            .send()
            .map_err(|e| {
                Error::Other(format!(
                    "request to {team}.kibe.la failed: {}",
                    with_causes(&e)
                ))
            })?;
        let status = response.status();
        let text = response.text().map_err(|e| {
            Error::Other(format!(
                "failed to read the response from {team}.kibe.la: {}",
                with_causes(&e)
            ))
        })?;
        Ok(classify(status, &text))
    }
}

fn with_causes(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text += &format!(": {cause}");
        source = cause.source();
    }
    text
}

fn classify(status: StatusCode, text: &str) -> Outcome {
    if status == StatusCode::UNAUTHORIZED {
        return Outcome::Fail(Error::Unauthorized);
    }
    if status.is_redirection() {
        return Outcome::Fail(Error::TeamNotFound);
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Outcome::Retry {
            wait: MIN_INTERVAL,
            reason: "too many requests".into(),
        };
    }
    let Ok(body) = serde_json::from_str::<Value>(text) else {
        return Outcome::Fail(Error::Other(format!("unexpected response ({status})")));
    };
    let Some(errors) = body["errors"]
        .as_array()
        .filter(|errors| !errors.is_empty())
    else {
        if !status.is_success() {
            return Outcome::Fail(Error::Other(format!("unexpected response ({status})")));
        }
        return Outcome::Data(body["data"].clone());
    };

    for error in errors {
        let extensions = &error["extensions"];
        match extensions["code"].as_str() {
            Some("TOKEN_BUDGET_EXHAUSTED") => {
                return Outcome::Retry {
                    wait: wait_of(extensions),
                    reason: "the token's short-term budget is used up".into(),
                };
            }
            Some("TEAM_BUDGET_EXHAUSTED") => {
                return Outcome::Fail(Error::TeamBudgetExhausted {
                    wait: wait_of(extensions),
                });
            }
            Some("NOT_FOUND") => return Outcome::Fail(Error::NotFound),
            Some("REQUEST_LIMIT_EXCEEDED") => {
                return Outcome::Fail(Error::Other(format!(
                    "the query costs {} but a request may cost at most {}",
                    text_of(&extensions["cost"]),
                    text_of(&extensions["maxCostPerRequest"]),
                )));
            }
            _ => {}
        }
    }
    let messages: Vec<&str> = errors
        .iter()
        .filter_map(|e| e["message"].as_str())
        .collect();
    Outcome::Fail(Error::Other(messages.join("\n")))
}

fn wait_of(extensions: &Value) -> Duration {
    let millis = match &extensions["waitMilliseconds"] {
        Value::String(s) => s.parse().unwrap_or(0),
        value => value.as_u64().unwrap_or(0),
    };
    Duration::from_millis(millis)
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        value => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(extensions: Value) -> String {
        json!({ "errors": [{ "message": "m", "extensions": extensions }] }).to_string()
    }

    #[test]
    fn data_is_returned_without_the_envelope() {
        let body = json!({ "data": { "note": { "title": "t" } } }).to_string();
        assert_eq!(
            classify(StatusCode::OK, &body),
            Outcome::Data(json!({ "note": { "title": "t" } }))
        );
    }

    #[test]
    fn http_statuses() {
        assert_eq!(
            classify(StatusCode::UNAUTHORIZED, ""),
            Outcome::Fail(Error::Unauthorized)
        );
        assert_eq!(
            classify(StatusCode::FOUND, ""),
            Outcome::Fail(Error::TeamNotFound)
        );
        assert_eq!(
            classify(StatusCode::TOO_MANY_REQUESTS, ""),
            Outcome::Retry {
                wait: MIN_INTERVAL,
                reason: "too many requests".into()
            }
        );
        let body = json!({ "data": null }).to_string();
        assert_eq!(
            classify(StatusCode::INTERNAL_SERVER_ERROR, &body),
            Outcome::Fail(Error::Other(
                "unexpected response (500 Internal Server Error)".into()
            ))
        );
        let not_found = errors(json!({ "code": "NOT_FOUND" }));
        assert_eq!(
            classify(StatusCode::NOT_FOUND, &not_found),
            Outcome::Fail(Error::NotFound)
        );
    }

    #[test]
    fn token_budget_waits_as_told() {
        let body = errors(json!({ "code": "TOKEN_BUDGET_EXHAUSTED", "waitMilliseconds": "5201" }));
        let Outcome::Retry { wait, .. } = classify(StatusCode::OK, &body) else {
            panic!()
        };
        assert_eq!(wait, Duration::from_millis(5201));
    }

    #[test]
    fn team_budget_fails_with_the_wait() {
        let body =
            errors(json!({ "code": "TEAM_BUDGET_EXHAUSTED", "waitMilliseconds": "1800000" }));
        assert_eq!(
            classify(StatusCode::OK, &body),
            Outcome::Fail(Error::TeamBudgetExhausted {
                wait: Duration::from_millis(1_800_000)
            })
        );
    }

    #[test]
    fn error_codes() {
        let not_found = errors(json!({ "code": "NOT_FOUND" }));
        assert_eq!(
            classify(StatusCode::OK, &not_found),
            Outcome::Fail(Error::NotFound)
        );

        let too_costly = errors(
            json!({ "code": "REQUEST_LIMIT_EXCEEDED", "cost": "10101", "maxCostPerRequest": "10000" }),
        );
        assert_eq!(
            classify(StatusCode::OK, &too_costly),
            Outcome::Fail(Error::Other(
                "the query costs 10101 but a request may cost at most 10000".into()
            ))
        );

        let other = errors(json!({ "code": "SOMETHING" }));
        assert_eq!(
            classify(StatusCode::OK, &other),
            Outcome::Fail(Error::Other("m".into()))
        );
    }
}
