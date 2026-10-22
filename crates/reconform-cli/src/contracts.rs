use crate::{
    compare::Finding,
    model::{Bundle, Completeness, MAX_ITEMS, Payload, VERSION, id},
};
use reconform_core::{
    Check, Contract, Evidence, ObservationView, Verdict, aggregate, evaluate_contract,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractPlan {
    pub schema_version: String,
    pub scenario: String,
    #[serde(deserialize_with = "crate::model::items")]
    pub checks: Vec<ContractCheck>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContractCheck {
    pub id: String,
    pub checkpoint: String,
    pub channel: String,
    pub occurrence: u32,
    pub required: bool,
    pub expression: Expression,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    Exists {},
    NonEmpty {},
    Between {
        #[serde(deserialize_with = "crate::model::number")]
        min: f64,
        #[serde(deserialize_with = "crate::model::number")]
        max: f64,
    },
    ExitSuccess {},
    Equals {
        value: Payload,
    },
    JsonExists {
        pointer: String,
    },
    EventContains {
        event: String,
    },
    EventOrder {
        #[serde(deserialize_with = "crate::model::items")]
        events: Vec<String>,
    },
}
impl Expression {
    fn contract(&self) -> Option<Contract> {
        Some(match self {
            Self::Exists {} => Contract::Exists,
            Self::NonEmpty {} => Contract::NonEmpty,
            Self::Between { min, max } => Contract::Between {
                min: *min,
                max: *max,
            },
            Self::ExitSuccess {} => Contract::ExitSuccess,
            _ => return None,
        })
    }
}
impl ContractPlan {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != VERSION || !id(&self.scenario) {
            return Err("invalid contract schema version or scenario");
        }
        if self.checks.is_empty()
            || self.checks.len() > MAX_ITEMS
            || !self.checks.iter().any(|c| c.required)
        {
            return Err("contracts need 1..1024 checks and a required check");
        }
        let mut ids = BTreeSet::new();
        for c in &self.checks {
            if let Expression::JsonExists { pointer } = &c.expression
                && !crate::model::json_pointer(pointer)
            {
                return Err("invalid JSON pointer");
            }
            if let Expression::EventOrder { events } = &c.expression
                && events.is_empty()
            {
                return Err("event order requires at least one event");
            }
            if !id(&c.id) || !id(&c.checkpoint) || !id(&c.channel) || !ids.insert(&c.id) {
                return Err("invalid or duplicate contract check identifier");
            }
            if let Expression::Between { min, max } = c.expression
                && (!min.is_finite() || !max.is_finite() || min > max)
            {
                return Err("range bounds must be finite and ordered");
            }
        }
        Ok(())
    }
    pub fn selects(&self, checkpoint: &str, channel: &str, occurrence: u32) -> bool {
        self.checks.iter().any(|c| {
            c.checkpoint == checkpoint && c.channel == channel && c.occurrence == occurrence
        })
    }
}

pub fn evaluate(bundle: &Bundle, plan: &ContractPlan) -> (Verdict, Vec<Finding>) {
    let mut checks: Vec<_> = plan.checks.iter().collect();
    checks.sort_by_key(|c| {
        (
            bundle.checkpoints.iter().position(|p| p == &c.checkpoint),
            &c.channel,
            c.occurrence,
            &c.id,
        )
    });
    let mut gate = Vec::new();
    let mut findings = Vec::new();
    for c in checks {
        let observation = bundle.observations.iter().find(|o| {
            o.checkpoint == c.checkpoint && o.channel == c.channel && o.occurrence == c.occurrence
        });
        let verdict = evaluate_expression(&c.expression, observation);
        let code = match verdict {
            Verdict::Pass => "CONTRACT_SATISFIED",
            Verdict::Fail => "CONTRACT_VIOLATED",
            Verdict::Unsupported => "CONTRACT_UNSUPPORTED",
            Verdict::Inconclusive if observation.is_none() => "OBSERVATION_MISSING",
            Verdict::Inconclusive => "OBSERVATION_PARTIAL",
            _ => "CONTRACT_ERROR",
        };
        gate.push(Check {
            required: c.required,
            verdict,
        });
        findings.push(Finding {
            measurements: None,
            id: c.id.clone(),
            checkpoint: c.checkpoint.clone(),
            channel: c.channel.clone(),
            occurrence: c.occurrence,
            required: c.required,
            verdict: verdict.as_str().into(),
            code: code.into(),
            location: None,
        });
    }
    (aggregate(gate), findings)
}

fn evaluate_expression(
    expression: &Expression,
    observation: Option<&crate::model::Observation>,
) -> Verdict {
    let Some(observation) = observation else {
        return if matches!(expression, Expression::Exists {}) {
            Verdict::Fail
        } else {
            Verdict::Inconclusive
        };
    };
    if observation.completeness != Completeness::Complete {
        return Verdict::Inconclusive;
    }
    let result = match expression {
        Expression::Equals { value } => {
            if value.kind() != observation.payload.kind() {
                return Verdict::Unsupported;
            }
            value == &observation.payload
        }
        Expression::JsonExists { pointer } => {
            let Ok(value) = crate::compare::json_value(&observation.payload) else {
                return Verdict::Error;
            };
            value.pointer(pointer).is_some()
        }
        Expression::EventContains { event } => {
            let Payload::Events(events) = &observation.payload else {
                return Verdict::Unsupported;
            };
            events.contains(event)
        }
        Expression::EventOrder { events: expected } => {
            let Payload::Events(events) = &observation.payload else {
                return Verdict::Unsupported;
            };
            let mut next = expected.iter();
            let mut wanted = next.next();
            for event in events {
                if wanted == Some(event) {
                    wanted = next.next();
                }
            }
            wanted.is_none()
        }
        _ => {
            if let Payload::Json(value) = &observation.payload {
                return match expression {
                    Expression::Exists {} => Verdict::Pass,
                    Expression::NonEmpty {} => {
                        if match value {
                            serde_json::Value::Null => false,
                            serde_json::Value::String(v) => !v.is_empty(),
                            serde_json::Value::Array(v) => !v.is_empty(),
                            serde_json::Value::Object(v) => !v.is_empty(),
                            _ => true,
                        } {
                            Verdict::Pass
                        } else {
                            Verdict::Fail
                        }
                    }
                    _ => Verdict::Unsupported,
                };
            }
            let evidence = Evidence::Complete(match &observation.payload {
                Payload::Text(v) => ObservationView::Bytes(v.as_bytes()),
                Payload::Bytes(v) => ObservationView::Bytes(v),
                Payload::Scalar(v) => ObservationView::Scalar(*v),
                Payload::Exit(v) => ObservationView::Exit(*v),
                Payload::Events(v) => ObservationView::Events(v),
                Payload::Json(_) => return Verdict::Unsupported,
            });
            return expression
                .contract()
                .map_or(Verdict::Unsupported, |contract| {
                    evaluate_contract(contract, evidence)
                });
        }
    };
    if result { Verdict::Pass } else { Verdict::Fail }
}
