use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const VERSION: &str = "1.0";
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_ITEMS: usize = 1024;

// Enforce collection limits while deserializing, rather than first allocating an
// arbitrarily large Vec and rejecting it during semantic validation.
fn limited<'de, D, T, const N: usize>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "an array with at most {N} elements")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while values.len() < N {
                match seq.next_element()? {
                    Some(value) => values.push(value),
                    None => return Ok(values),
                }
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom("array limit exceeded"));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor::<T, N>(std::marker::PhantomData))
}

pub fn items<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    limited::<D, T, MAX_ITEMS>(deserializer)
}
fn events<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    limited::<D, String, 10000>(deserializer)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub schema_version: String,
    pub scenario: String,
    pub subject: String,
    pub provenance: String,
    pub environment: Option<String>,
    #[serde(deserialize_with = "items")]
    pub checkpoints: Vec<String>,
    #[serde(deserialize_with = "items")]
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
    Events(#[serde(deserialize_with = "events")] Vec<String>),
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
    #[serde(deserialize_with = "items")]
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
    Exact {},
    Absolute {
        tolerance: f64,
    },
    Png {
        tolerance: u8,
        max_mismatch_fraction: f64,
        region: Option<[usize; 4]>,
        assume_srgb: bool,
    },
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
            if let Policy::Png {
                max_mismatch_fraction,
                region,
                ..
            } = &c.policy
                && (!max_mismatch_fraction.is_finite()
                    || !(0.0..=1.0).contains(max_mismatch_fraction)
                    || region.is_some_and(|[x, y, r, b]| x >= r || y >= b))
            {
                return Err("invalid PNG mismatch fraction or region");
            }
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
