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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measurements: Option<serde_json::Value>,
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
        let mut measurements = None;
        let (verdict, code, location) = compare(
            left,
            right,
            check,
            plan.strict_environment,
            &mut measurements,
        );
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
            measurements,
        });
    }
    (aggregate(gate), findings)
}

fn compare(
    left: &Bundle,
    right: &Bundle,
    check: &CheckSpec,
    strict: bool,
    _measurements: &mut Option<serde_json::Value>,
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
        (Payload::Json(a), Payload::Json(b), Policy::Exact {}) => {
            (if a == b { Verdict::Pass } else { Verdict::Fail }, None)
        }
        (a, b, Policy::Json { pointer, tolerance }) => {
            let (Ok(a), Ok(b)) = (json_value(a), json_value(b)) else {
                return (Verdict::Error, "JSON_DECODE_ERROR", None);
            };
            let (Some(a), Some(b)) = (a.pointer(pointer), b.pointer(pointer)) else {
                return (Verdict::Inconclusive, "JSON_PATH_MISSING", None);
            };
            let verdict = if *tolerance == 0.0 || !a.is_number() || !b.is_number() {
                if a == b { Verdict::Pass } else { Verdict::Fail }
            } else {
                // Large values cannot safely enter the f64 tolerance path.
                if [a, b].iter().any(|v| {
                    v.as_i64().is_some_and(|n| n.unsigned_abs() > (1u64 << 53))
                        || v.as_u64().is_some_and(|n| n > (1u64 << 53))
                        || v.as_f64().is_none_or(|n| n.abs() >= (1u64 << 53) as f64)
                }) {
                    return (Verdict::Unsupported, "JSON_NUMBER_PRECISION", None);
                }
                match (a.as_f64(), b.as_f64()) {
                    (Some(a), Some(b)) => {
                        compare_numbers(a, b, *tolerance).unwrap_or(Verdict::Error)
                    }
                    _ => Verdict::Error,
                }
            };
            (verdict, None)
        }
        (
            a,
            b,
            Policy::Text {
                normalize_newlines,
                trim_trailing_whitespace,
            },
        ) => {
            let (Some(a), Some(b)) = (text_value(a), text_value(b)) else {
                return (Verdict::Error, "TEXT_DECODE_ERROR", None);
            };
            let a = normalize_text(a, *normalize_newlines, *trim_trailing_whitespace);
            let b = normalize_text(b, *normalize_newlines, *trim_trailing_whitespace);
            (
                compare_bytes(a.as_bytes(), b.as_bytes()),
                first_difference(a.as_bytes(), b.as_bytes()).map(Location::Byte),
            )
        }
        #[cfg(feature = "png")]
        (
            Payload::Bytes(a),
            Payload::Bytes(b),
            Policy::Png {
                tolerance,
                max_mismatch_fraction,
                region,
                assume_srgb,
            },
        ) => {
            let (verdict, code, metrics) = crate::visual::compare_encoded(
                a,
                b,
                *tolerance,
                *max_mismatch_fraction,
                *region,
                *assume_srgb,
            );
            *_measurements = metrics;
            return (verdict, code, None);
        }
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
                _ => return (Verdict::Unsupported, "POLICY_UNSUPPORTED", None),
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

pub fn json_value(payload: &Payload) -> Result<std::borrow::Cow<'_, serde_json::Value>, ()> {
    use std::borrow::Cow;
    match payload {
        Payload::Json(value) => Ok(Cow::Borrowed(value)),
        Payload::Text(value) => {
            crate::wire::unique_keys(value.as_bytes()).map_err(|_| ())?;
            serde_json::from_str(value).map(Cow::Owned).map_err(|_| ())
        }
        Payload::Bytes(value) => {
            crate::wire::unique_keys(value).map_err(|_| ())?;
            serde_json::from_slice(value)
                .map(Cow::Owned)
                .map_err(|_| ())
        }
        _ => Err(()),
    }
}
fn text_value(payload: &Payload) -> Option<&str> {
    match payload {
        Payload::Text(v) => Some(v),
        Payload::Bytes(v) => std::str::from_utf8(v).ok(),
        _ => None,
    }
}
fn normalize_text(value: &str, newlines: bool, trailing: bool) -> String {
    let value = if newlines {
        value.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        value.to_owned()
    };
    if trailing {
        value
            .split('\n')
            .map(|line| line.trim_end_matches([' ', '\t']))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        value
    }
}
