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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    Exists {},
    NonEmpty {},
    Between { min: f64, max: f64 },
    ExitSuccess {},
}
impl Expression {
    fn contract(self) -> Contract {
        match self {
            Self::Exists {} => Contract::Exists,
            Self::NonEmpty {} => Contract::NonEmpty,
            Self::Between { min, max } => Contract::Between { min, max },
            Self::ExitSuccess {} => Contract::ExitSuccess,
        }
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
        let evidence = match observation {
            None => Evidence::Missing,
            Some(o) if o.completeness != Completeness::Complete => Evidence::Incomplete,
            Some(o) => Evidence::Complete(match &o.payload {
                Payload::Text(v) => ObservationView::Bytes(v.as_bytes()),
                Payload::Bytes(v) => ObservationView::Bytes(v),
                Payload::Scalar(v) => ObservationView::Scalar(*v),
                Payload::Exit(v) => ObservationView::Exit(*v),
                Payload::Events(v) => ObservationView::Events(v),
            }),
        };
        let verdict = evaluate_contract(c.expression.contract(), evidence);
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
