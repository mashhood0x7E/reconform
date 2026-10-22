use crate::{
    Error, Options, capture,
    contracts::ContractPlan,
    model::{Plan, VERSION},
    read,
};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    schema_version: String,
    candidate: String,
    reference: Option<String>,
    plan: String,
    evidence_dir: String,
}

pub fn execute(opts: &Options) -> Result<u8, Error> {
    if opts.positional.len() != 1
        || opts.kind.is_some()
        || opts.plan.is_some()
        || opts.diff.is_some()
    {
        return Err(Error::config(
            "run requires SCENARIO --allow-exec ROOT [--output REPORT]",
        ));
    }
    let root = fs::canonicalize(
        opts.allow_exec
            .as_ref()
            .ok_or_else(|| Error::config("run requires --allow-exec ROOT"))?,
    )
    .map_err(Error::io)?;
    let (scenario, _) = read::<Scenario>(&opts.positional[0])?;
    if scenario.schema_version != VERSION || !Path::new(&scenario.evidence_dir).is_absolute() {
        return Err(Error::config(
            "run requires schema 1.0 and an absolute fresh evidence_dir",
        ));
    }
    let evidence = Path::new(&scenario.evidence_dir);
    if evidence.exists() || !evidence.parent().is_some_and(Path::is_dir) {
        return Err(Error::config(
            "evidence_dir must be fresh with an existing parent",
        ));
    }
    if let Some(output) = &opts.output {
        let p = Path::new(output);
        if p.exists()
            || !p
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .is_dir()
        {
            return Err(Error::config(
                "report must be a fresh path with an existing parent",
            ));
        }
    }
    let candidate = capture::preflight(&scenario.candidate, &root)?;
    if let Some(reference) = &scenario.reference {
        let reference = capture::preflight(reference, &root)?;
        let (plan, _) = read::<Plan>(&scenario.plan)?;
        plan.validate().map_err(Error::config)?;
        if candidate.scenario != reference.scenario
            || candidate.checkpoints != reference.checkpoints
            || !candidate.files.is_disjoint(&reference.files)
            || candidate.scenario != plan.scenario
            || plan.checks.iter().any(|c| {
                c.occurrence != 0
                    || !candidate
                        .channels
                        .contains(&(c.checkpoint.clone(), c.channel.clone()))
                    || !reference
                        .channels
                        .contains(&(c.checkpoint.clone(), c.channel.clone()))
            })
        {
            return Err(Error::config(
                "scenario, checkpoint order or plan references differ",
            ));
        }
        if plan.checks.iter().any(|c| {
            if c.channel == "exit" {
                !matches!(c.policy, crate::model::Policy::Exact {})
            } else {
                matches!(c.policy, crate::model::Policy::Absolute { .. })
            }
        }) {
            return Err(Error::config(
                "comparison policy is incompatible with the captured channel type",
            ));
        }
        #[cfg(not(feature = "png"))]
        if plan
            .checks
            .iter()
            .any(|c| matches!(c.policy, crate::model::Policy::Png { .. }))
        {
            return Err(Error::config(
                "PNG comparisons require a build with --features png",
            ));
        }
    } else {
        let (plan, _) = read::<ContractPlan>(&scenario.plan)?;
        plan.validate().map_err(Error::config)?;
        if candidate.scenario != plan.scenario
            || plan.checks.iter().any(|c| {
                c.occurrence != 0
                    || !candidate
                        .channels
                        .contains(&(c.checkpoint.clone(), c.channel.clone()))
            })
        {
            return Err(Error::config(
                "contract scenario or checkpoint references differ",
            ));
        }
        use crate::contracts::Expression;
        use crate::model::Payload;
        if plan.checks.iter().any(|c| !match &c.expression {
            Expression::Exists {} => true,
            Expression::NonEmpty {} | Expression::JsonExists { .. } => c.channel != "exit",
            Expression::ExitSuccess {} => c.channel == "exit",
            Expression::Equals {
                value: Payload::Exit(_),
            } => c.channel == "exit",
            Expression::Equals {
                value: Payload::Bytes(_),
            } => c.channel != "exit",
            _ => false,
        }) {
            return Err(Error::config(
                "contract is incompatible with captured byte/exit channels",
            ));
        }
    }
    fs::create_dir(evidence).map_err(Error::io)?;
    let candidate_bundle = evidence
        .join("candidate.json")
        .to_string_lossy()
        .into_owned();
    let reference_bundle = evidence
        .join("reference.json")
        .to_string_lossy()
        .into_owned();
    for (manifest, output) in scenario
        .reference
        .iter()
        .map(|r| (r, &reference_bundle))
        .chain(std::iter::once((&scenario.candidate, &candidate_bundle)))
    {
        let capture_opts = Options {
            positional: vec![manifest.clone()],
            plan: None,
            kind: None,
            output: Some(output.clone()),
            json: true,
            allow_exec: opts.allow_exec.clone(),
            diff: None,
            junit: false,
        };
        if capture::execute(&capture_opts, false, true)? != 0 {
            crate::emit(
                &serde_json::json!({"schema_version":VERSION,"method":"process-run/1","verdict":"inconclusive","code":"CAPTURE_INCOMPLETE","bundle":output}),
                "Run inconclusive: capture incomplete; evidence retained.\n",
                opts,
            )?;
            return Ok(4);
        }
    }
    let mut args = if scenario.reference.is_some() {
        vec!["compare".into(), reference_bundle, candidate_bundle]
    } else {
        vec!["check".into(), candidate_bundle]
    };
    args.extend([
        "--plan".into(),
        scenario.plan,
        "--format".into(),
        if opts.junit {
            "junit"
        } else if opts.json {
            "json"
        } else {
            "human"
        }
        .into(),
    ]);
    if let Some(output) = &opts.output {
        args.extend(["--output".into(), output.clone()]);
    }
    crate::execute(&args)
}
