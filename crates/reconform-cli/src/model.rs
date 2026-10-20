use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const VERSION: &str = "1.0";
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_ITEMS: usize = 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub schema_version: String,
    pub scenario: String,
    pub subject: String,
    pub provenance: String,
    pub environment: Option<String>,
    pub checkpoints: Vec<String>,
    pub observations: Vec<Observation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub checkpoint: String,
    pub channel: String,
    pub occurrence: u32,
    pub completeness: Completeness,
    pub payload: Payload,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Complete,
    Partial,
    Unknown,
    Unavailable,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Payload {
    Text(String),
    Bytes(Vec<u8>),
    Scalar(f64),
    Events(Vec<String>),
    Exit(i32),
}
impl Payload {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Bytes(_) => "bytes",
            Self::Scalar(_) => "scalar",
            Self::Events(_) => "events",
            Self::Exit(_) => "exit",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: String,
    pub scenario: String,
    pub strict_environment: bool,
    pub checks: Vec<CheckSpec>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSpec {
    pub id: String,
    pub checkpoint: String,
    pub channel: String,
    pub occurrence: u32,
    pub required: bool,
    pub policy: Policy,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Policy {
    Exact,
    Absolute { tolerance: f64 },
}

pub fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
impl Bundle {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != VERSION {
            return Err("unsupported bundle schema_version; expected 1.0");
        }
        if !id(&self.scenario) || !id(&self.subject) {
            return Err("invalid scenario or subject identifier");
        }
        if self.provenance.is_empty()
            || self.provenance.len() > 4096
            || self
                .environment
                .as_ref()
                .is_some_and(|e| e.is_empty() || e.len() > 4096)
        {
            return Err("invalid provenance or environment");
        }
        if self.checkpoints.is_empty()
            || self.checkpoints.len() > MAX_ITEMS
            || self.observations.len() > MAX_ITEMS
        {
            return Err("bundle count exceeds limits or checkpoints are empty");
        }
        let mut checkpoints = BTreeSet::new();
        for checkpoint in &self.checkpoints {
            if !id(checkpoint) || !checkpoints.insert(checkpoint) {
                return Err("invalid or duplicate checkpoint");
            }
        }
        let mut keys = BTreeSet::new();
        for o in &self.observations {
            if !checkpoints.contains(&o.checkpoint)
                || !id(&o.channel)
                || !keys.insert((&o.checkpoint, &o.channel, o.occurrence))
            {
                return Err("invalid, undeclared or duplicate observation key");
            }
            match &o.payload {
                Payload::Scalar(v) if !v.is_finite() => return Err("scalar must be finite"),
                Payload::Events(v) if v.len() > 10000 => return Err("event count exceeds 10000"),
                _ => (),
            }
        }
        Ok(())
    }
    pub fn find(&self, spec: &CheckSpec) -> Option<&Observation> {
        self.observations.iter().find(|o| {
            o.checkpoint == spec.checkpoint
                && o.channel == spec.channel
                && o.occurrence == spec.occurrence
        })
    }
}
impl Plan {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != VERSION {
            return Err("unsupported plan schema_version; expected 1.0");
        }
        if !id(&self.scenario)
            || self.checks.is_empty()
            || self.checks.len() > MAX_ITEMS
            || !self.checks.iter().any(|c| c.required)
        {
            return Err(
                "plan needs a valid scenario and 1..1024 checks with at least one required check",
            );
        }
        let mut ids = BTreeSet::new();
        for c in &self.checks {
            if !id(&c.id) || !id(&c.checkpoint) || !id(&c.channel) || !ids.insert(&c.id) {
                return Err("invalid or duplicate check identifier");
            }
            if let Policy::Absolute { tolerance } = c.policy
                && (!tolerance.is_finite() || tolerance < 0.0)
            {
                return Err("tolerance must be finite and nonnegative");
            }
        }
        Ok(())
    }
}
