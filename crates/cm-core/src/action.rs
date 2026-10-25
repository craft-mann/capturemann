//! Who did something (`Actor`) and the revision record kept in the manifest.

use crate::error::CmError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use time::OffsetDateTime;

/// The originator of an action: `user` or `agent:<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Actor {
    User,
    Agent(String),
}

impl Actor {
    /// `agent:<name>`; the name must be non-empty.
    pub fn agent(name: impl Into<String>) -> Result<Self, CmError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(CmError::invalid_field("by", "agent name is empty"));
        }
        Ok(Actor::Agent(name))
    }

    pub fn is_agent(&self) -> bool {
        matches!(self, Actor::Agent(_))
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::User => f.write_str("user"),
            Actor::Agent(name) => write!(f, "agent:{name}"),
        }
    }
}

impl FromStr for Actor {
    type Err = CmError;
    fn from_str(s: &str) -> Result<Self, CmError> {
        if s == "user" {
            return Ok(Actor::User);
        }
        match s.strip_prefix("agent:") {
            Some(name) => Actor::agent(name),
            None => Err(CmError::invalid_field(
                "by",
                format!("expected `user` or `agent:<name>`, got `{s}`"),
            )),
        }
    }
}

impl TryFrom<String> for Actor {
    type Error = CmError;
    fn try_from(s: String) -> Result<Self, CmError> {
        s.parse()
    }
}
impl From<Actor> for String {
    fn from(a: Actor) -> String {
        a.to_string()
    }
}

/// One entry of `manifest.revisions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    pub rev: u64,
    pub action: String,
    pub by: Actor,
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn actor_serde_strings() {
        assert_eq!(serde_json::to_string(&Actor::User).unwrap(), "\"user\"");
        let a = Actor::agent("claude").unwrap();
        assert_eq!(serde_json::to_string(&a).unwrap(), "\"agent:claude\"");
        assert_eq!(
            serde_json::from_str::<Actor>("\"agent:claude\"").unwrap(),
            a
        );
        for bad in ["\"agent:\"", "\"robot\"", "\"\""] {
            assert!(serde_json::from_str::<Actor>(bad).is_err(), "{bad}");
        }
        assert!(a.is_agent() && !Actor::User.is_agent());
    }

    #[test]
    fn revision_matches_manifest_json() {
        let json =
            r#"{ "rev": 3, "action": "trim", "by": "agent:claude", "at": "2026-10-19T09:20:41Z" }"#;
        let r: Revision = serde_json::from_str(json).unwrap();
        assert_eq!(r.rev, 3);
        assert_eq!(r.by, Actor::agent("claude").unwrap());
        assert_eq!(r.at, datetime!(2026-10-19 09:20:41 UTC));
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["at"], "2026-10-19T09:20:41Z");
        assert_eq!(v["by"], "agent:claude");
    }
}
