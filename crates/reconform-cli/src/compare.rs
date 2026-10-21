use crate::model::{Bundle, CheckSpec, Completeness, Payload, Plan, Policy};
use reconform_core::{Check, Verdict, aggregate, compare_bytes, compare_numbers, first_difference};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    pub checkpoint: String,
    pub channel: String,
    pub occurrence: u32,
    pub required: bool,
    pub verdict: String,
    pub code: String,
    pub location: Option<Location>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "index",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Location {
    Byte(usize),
    Event(usize),
}

pub fn evaluate(left: &Bundle, right: &Bundle, plan: &Plan) -> (Verdict, Vec<Finding>) {
    let mut checks: Vec<&CheckSpec> = plan.checks.iter().collect();
    checks.sort_by_key(|c| {
        (
            left.checkpoints.iter().position(|p| p == &c.checkpoint),
            &c.channel,
            c.occurrence,
            &c.id,
        )
    });
    let mut gate = Vec::new();
    let mut findings = Vec::new();
    for check in checks {
        let (verdict, code, location) = compare(left, right, check, plan.strict_environment);
        gate.push(Check {
            required: check.required,
            verdict,
        });
        findings.push(Finding {
            id: check.id.clone(),
            checkpoint: check.checkpoint.clone(),
            channel: check.channel.clone(),
            occurrence: check.occurrence,
            required: check.required,
            verdict: verdict.as_str().into(),
            code: code.into(),
            location,
        });
    }
    (aggregate(gate), findings)
}

fn compare(
    left: &Bundle,
    right: &Bundle,
    check: &CheckSpec,
    strict: bool,
) -> (Verdict, &'static str, Option<Location>) {
    if strict && (left.environment.is_none() || left.environment != right.environment) {
        return (Verdict::Inconclusive, "ENVIRONMENT_MISMATCH", None);
    }
    let (Some(a), Some(b)) = (left.find(check), right.find(check)) else {
        return (Verdict::Inconclusive, "OBSERVATION_MISSING", None);
    };
    if a.completeness != Completeness::Complete || b.completeness != Completeness::Complete {
        return (Verdict::Inconclusive, "OBSERVATION_PARTIAL", None);
    }
    if a.payload.kind() != b.payload.kind() {
        return (Verdict::Inconclusive, "KIND_MISMATCH", None);
    }
    let (verdict, location) = match (&a.payload, &b.payload, &check.policy) {
        (Payload::Text(a), Payload::Text(b), Policy::Exact {}) => (
            compare_bytes(a.as_bytes(), b.as_bytes()),
            first_difference(a.as_bytes(), b.as_bytes()).map(Location::Byte),
        ),
        (Payload::Bytes(a), Payload::Bytes(b), Policy::Exact {}) => (
            compare_bytes(a, b),
            first_difference(a, b).map(Location::Byte),
        ),
        (Payload::Exit(a), Payload::Exit(b), Policy::Exact {}) => {
            (if a == b { Verdict::Pass } else { Verdict::Fail }, None)
        }
        (Payload::Scalar(a), Payload::Scalar(b), policy) => {
            let tolerance = match policy {
                Policy::Exact {} => 0.0,
                Policy::Absolute { tolerance } => *tolerance,
            };
            (
                compare_numbers(*a, *b, tolerance).unwrap_or(Verdict::Error),
                None,
            )
        }
        (Payload::Events(a), Payload::Events(b), Policy::Exact {}) => {
            let index = a
                .iter()
                .zip(b)
                .position(|(a, b)| a != b)
                .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())));
            (
                if index.is_none() {
                    Verdict::Pass
                } else {
                    Verdict::Fail
                },
                index.map(Location::Event),
            )
        }
        _ => return (Verdict::Unsupported, "POLICY_UNSUPPORTED", None),
    };
    (
        verdict,
        match verdict {
            Verdict::Pass => "COMPARE_EQUAL",
            Verdict::Fail => "COMPARE_DIFFERENT",
            _ => "COMPARE_ERROR",
        },
        location,
    )
}
