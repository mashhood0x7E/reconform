mod compare;
mod contracts;
mod model;

use compare::Finding;
use contracts::ContractPlan;
use model::{Bundle, MAX_BYTES, Plan, VERSION};
use reconform_core::Verdict;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

const HELP: &str = "Reconform: compare recorded observations (experimental)\n\n  reconform compare LEFT RIGHT --plan PLAN [--format human|json] [--output FILE]\n  reconform check BUNDLE --plan CONTRACTS [--format human|json] [--output FILE]\n  reconform validate FILE --kind bundle|plan|contracts [--format human|json]\n  reconform version\n\nInputs: schema 1.0 UTF-8 JSON, at most 8 MiB each. No target is executed.\nOutput files must not already exist. Exit: 0 pass, 1 mismatch, 2 invalid input,\n3 I/O error, 4 inconclusive/unsupported. See docs/recorded-format.md.\n";

#[derive(Debug)]
struct Error {
    exit: u8,
    code: &'static str,
    message: String,
}
impl Error {
    fn config(message: impl Into<String>) -> Self {
        Self {
            exit: 2,
            code: "CONFIG_INVALID",
            message: message.into(),
        }
    }
    fn io(error: io::Error) -> Self {
        Self {
            exit: 3,
            code: "IO_ERROR",
            message: error.to_string(),
        }
    }
}

#[derive(Serialize)]
struct InputIdentity {
    path: String,
    digest: String,
}
#[derive(Serialize)]
struct Subject<'a> {
    id: &'a str,
    provenance: &'a str,
    environment: &'a Option<String>,
}
#[derive(Serialize)]
struct Report<'a, P> {
    schema_version: &'static str,
    tool_version: &'static str,
    scenario: &'a str,
    verdict: &'static str,
    method: &'static str,
    policy_digest: String,
    plan: &'a P,
    inputs: Vec<InputIdentity>,
    subjects: Vec<Subject<'a>>,
    findings: Vec<Finding>,
    first_divergence: Option<String>,
    unselected_observations: Vec<usize>,
    limitations: Vec<&'static str>,
    reproduce: Vec<String>,
}

fn digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

fn read<T: DeserializeOwned>(path: &str) -> Result<(T, InputIdentity), Error> {
    let absolute = fs::canonicalize(path).map_err(Error::io)?;
    if !fs::metadata(&absolute).map_err(Error::io)?.is_file() {
        return Err(Error::config("input must be a regular file"));
    }
    let mut open = OpenOptions::new();
    open.read(true);
    // A FIFO swapped in after metadata must not hang before the descriptor check.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY);
    }
    let file = open.open(&absolute).map_err(Error::io)?;
    if !file.metadata().map_err(Error::io)?.is_file() {
        return Err(Error::config("input must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::io)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::config("input exceeds 8 MiB"));
    }
    // Avoid reflecting untrusted payloads or credentials in parser diagnostics.
    let value = serde_json::from_slice(&bytes).map_err(|e| {
        Error::config(format!(
            "invalid JSON/schema at line {}, column {}",
            e.line(),
            e.column()
        ))
    })?;
    let path = absolute
        .to_str()
        .ok_or_else(|| Error::config("input path must be UTF-8 for reproduction"))?
        .to_owned();
    Ok((
        value,
        InputIdentity {
            path,
            digest: digest(&bytes),
        },
    ))
}

struct Options {
    positional: Vec<String>,
    plan: Option<String>,
    kind: Option<String>,
    output: Option<String>,
    json: bool,
}
fn options(args: &[String]) -> Result<Options, Error> {
    let mut result = Options {
        positional: Vec::new(),
        plan: None,
        kind: None,
        output: None,
        json: false,
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with("--") {
            if !seen.insert(arg) {
                return Err(Error::config("duplicate option"));
            }
            i += 1;
            let value = args
                .get(i)
                .ok_or_else(|| Error::config("option requires a value"))?;
            match arg.as_str() {
                "--plan" => result.plan = Some(value.clone()),
                "--kind" => result.kind = Some(value.clone()),
                "--output" => result.output = Some(value.clone()),
                "--format" => match value.as_str() {
                    "json" => result.json = true,
                    "human" => (),
                    _ => return Err(Error::config("format must be human or json")),
                },
                _ => return Err(Error::config("unknown option")),
            }
        } else {
            result.positional.push(arg.clone());
        }
        i += 1;
    }
    Ok(result)
}

// A same-directory temporary file and hard link make the completed report visible
// atomically without replacing an existing destination. Unsupported filesystems
// fail explicitly. No input-controlled output path is ever followed automatically.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(Error::io)?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::config("output needs a filename"))?;
    let destination = parent.join(name);
    let mut temporary: Option<(PathBuf, File)> = None;
    for counter in 0..100 {
        let p = parent.join(format!(".reconform-{}-{counter}.tmp", std::process::id()));
        let mut open = OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open.mode(0o600);
        }
        match open.open(&p) {
            Ok(file) => {
                temporary = Some((p, file));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(Error::io(e)),
        }
    }
    let (temp, mut file) =
        temporary.ok_or_else(|| Error::config("no temporary report filename available"))?;
    let result = (|| {
        file.write_all(bytes).map_err(Error::io)?;
        file.sync_all().map_err(Error::io)?;
        drop(file);
        fs::hard_link(&temp, destination).map_err(Error::io)
    })();
    let cleanup = fs::remove_file(temp).map_err(Error::io);
    result.and(cleanup)
}

fn emit(value: &impl Serialize, human: &str, options: &Options) -> Result<(), Error> {
    let mut json = serde_json::to_vec_pretty(value)
        .map_err(|_| Error::config("report serialization failed"))?;
    json.push(b'\n');
    if json.len() > 2 * 1024 * 1024 {
        return Err(Error::config("report exceeds 2 MiB"));
    }
    if let Some(path) = &options.output {
        write_new(Path::new(path), &json)?;
    }
    io::stdout()
        .lock()
        .write_all(if options.json {
            &json
        } else {
            human.as_bytes()
        })
        .map_err(Error::io)
}

fn execute(args: &[String]) -> Result<u8, Error> {
    let Some(command) = args.first() else {
        print!("{HELP}");
        return Ok(0);
    };
    if args.len() == 1 && ["--help", "-h", "help"].contains(&command.as_str()) {
        print!("{HELP}");
        return Ok(0);
    }
    if args.len() == 1 && ["version", "--version"].contains(&command.as_str()) {
        println!("reconform {} (schema 1.0)", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    let opts = options(&args[1..])?;
    match command.as_str() {
        "validate" => {
            if opts.positional.len() != 1 || opts.plan.is_some() || opts.output.is_some() {
                return Err(Error::config(
                    "validate requires FILE --kind bundle|plan|contracts",
                ));
            }
            match opts.kind.as_deref() {
                Some("bundle") => read::<Bundle>(&opts.positional[0])?
                    .0
                    .validate()
                    .map_err(Error::config)?,
                Some("plan") => read::<Plan>(&opts.positional[0])?
                    .0
                    .validate()
                    .map_err(Error::config)?,
                Some("contracts") => read::<ContractPlan>(&opts.positional[0])?
                    .0
                    .validate()
                    .map_err(Error::config)?,
                _ => {
                    return Err(Error::config(
                        "validate requires --kind bundle|plan|contracts",
                    ));
                }
            }
            emit(
                &serde_json::json!({"schema_version": VERSION, "valid": true}),
                "Valid schema 1.0 input\n",
                &opts,
            )?;
            Ok(0)
        }
        "compare" => {
            if opts.positional.len() != 2 || opts.kind.is_some() {
                return Err(Error::config("compare requires LEFT RIGHT --plan PLAN"));
            }
            let plan_path = opts
                .plan
                .as_deref()
                .ok_or_else(|| Error::config("compare requires --plan PLAN"))?;
            let (plan, plan_id) = read::<Plan>(plan_path)?;
            let (left, left_id) = read::<Bundle>(&opts.positional[0])?;
            let (right, right_id) = read::<Bundle>(&opts.positional[1])?;
            plan.validate().map_err(Error::config)?;
            left.validate().map_err(Error::config)?;
            right.validate().map_err(Error::config)?;
            if left.scenario != plan.scenario
                || right.scenario != plan.scenario
                || left.checkpoints != right.checkpoints
            {
                return Err(Error::config(
                    "scenario identities or checkpoint order differ",
                ));
            }
            if plan
                .checks
                .iter()
                .any(|c| !left.checkpoints.contains(&c.checkpoint))
            {
                return Err(Error::config("plan references an undeclared checkpoint"));
            }
            let (verdict, findings) = compare::evaluate(&left, &right, &plan);
            let first_divergence = findings
                .iter()
                .find(|f| f.verdict == "fail")
                .map(|f| f.id.clone());
            let canonical_policy = serde_json::to_vec(&plan)
                .map_err(|_| Error::config("policy serialization failed"))?;
            let reproduce = vec![
                "reconform".into(),
                "compare".into(),
                left_id.path.clone(),
                right_id.path.clone(),
                "--plan".into(),
                plan_id.path.clone(),
                "--format".into(),
                "json".into(),
            ];
            let mut limitations = vec![
                "Recorded observations are caller-supplied claims, not authenticated captures.",
                "Comparison covers selected observations only; it does not prove program equivalence.",
                "Retain original input files and verify their digests before rerunning.",
            ];
            if left.environment.is_none() || left.environment != right.environment {
                limitations.push("Environment compatibility is unknown or different.");
            }
            let unselected_observations = [&left, &right].map(|b| {
                b.observations
                    .iter()
                    .filter(|o| {
                        !plan.checks.iter().any(|c| {
                            c.checkpoint == o.checkpoint
                                && c.channel == o.channel
                                && c.occurrence == o.occurrence
                        })
                    })
                    .count()
            });
            let report = Report {
                schema_version: VERSION,
                tool_version: env!("CARGO_PKG_VERSION"),
                scenario: &plan.scenario,
                verdict: verdict.as_str(),
                method: "recorded-parity/1",
                policy_digest: digest(&canonical_policy),
                plan: &plan,
                inputs: vec![left_id, right_id, plan_id],
                subjects: vec![
                    Subject {
                        id: &left.subject,
                        provenance: &left.provenance,
                        environment: &left.environment,
                    },
                    Subject {
                        id: &right.subject,
                        provenance: &right.provenance,
                        environment: &right.environment,
                    },
                ],
                findings,
                first_divergence,
                unselected_observations: unselected_observations.to_vec(),
                limitations,
                reproduce,
            };
            let mut human = format!("{}: {}\n", plan.scenario, verdict.as_str());
            for f in &report.findings {
                human.push_str(&format!(
                    "  {}: {} ({}, {} / {}, occurrence {})\n",
                    f.id, f.verdict, f.code, f.checkpoint, f.channel, f.occurrence
                ));
            }
            human.push_str(
                "Use --format json for input digests, policies and the rerun argument array.\n",
            );
            emit(&report, &human, &opts)?;
            Ok(match verdict {
                Verdict::Pass => 0,
                Verdict::Fail => 1,
                Verdict::Error => 3,
                _ => 4,
            })
        }
        "check" => {
            if opts.positional.len() != 1 || opts.kind.is_some() {
                return Err(Error::config("check requires BUNDLE --plan CONTRACTS"));
            }
            let path = opts
                .plan
                .as_deref()
                .ok_or_else(|| Error::config("check requires --plan CONTRACTS"))?;
            let (plan, plan_id) = read::<ContractPlan>(path)?;
            let (bundle, bundle_id) = read::<Bundle>(&opts.positional[0])?;
            plan.validate().map_err(Error::config)?;
            bundle.validate().map_err(Error::config)?;
            if plan.scenario != bundle.scenario
                || plan
                    .checks
                    .iter()
                    .any(|c| !bundle.checkpoints.contains(&c.checkpoint))
            {
                return Err(Error::config(
                    "contract scenario or checkpoint does not match bundle",
                ));
            }
            let (verdict, findings) = contracts::evaluate(&bundle, &plan);
            let first_divergence = findings
                .iter()
                .find(|f| f.verdict == "fail")
                .map(|f| f.id.clone());
            let reproduce = vec![
                "reconform".into(),
                "check".into(),
                bundle_id.path.clone(),
                "--plan".into(),
                plan_id.path.clone(),
                "--format".into(),
                "json".into(),
            ];
            let policy = serde_json::to_vec(&plan)
                .map_err(|_| Error::config("policy serialization failed"))?;
            let unselected = bundle
                .observations
                .iter()
                .filter(|o| !plan.selects(&o.checkpoint, &o.channel, o.occurrence))
                .count();
            let report = Report {
                schema_version: VERSION,
                tool_version: env!("CARGO_PKG_VERSION"),
                scenario: &plan.scenario,
                verdict: verdict.as_str(),
                method: "recorded-contracts/1",
                policy_digest: digest(&policy),
                plan: &plan,
                inputs: vec![bundle_id, plan_id],
                subjects: vec![Subject {
                    id: &bundle.subject,
                    provenance: &bundle.provenance,
                    environment: &bundle.environment,
                }],
                findings,
                first_divergence,
                unselected_observations: vec![unselected],
                limitations: vec![
                    "Contracts inspect caller-supplied recorded observations, not live application state.",
                    "Exists asserts the presence of a complete observation in this recording.",
                    "Retain original input files and verify their digests before rerunning.",
                ],
                reproduce,
            };
            let mut human = format!("{}: {}\n", plan.scenario, verdict.as_str());
            for f in &report.findings {
                human.push_str(&format!("  {}: {} ({})\n", f.id, f.verdict, f.code));
            }
            emit(&report, &human, &opts)?;
            Ok(match verdict {
                Verdict::Pass => 0,
                Verdict::Fail => 1,
                Verdict::Error => 3,
                _ => 4,
            })
        }
        _ => Err(Error::config("unknown command; use --help")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .map(|s| s.into_string())
        .collect::<Result<_, _>>()
    {
        Ok(args) => args,
        Err(_) => {
            eprintln!("arguments must be UTF-8");
            return ExitCode::from(2);
        }
    };
    match execute(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if args.windows(2).any(|a| a == ["--format", "json"]) {
                let value = serde_json::json!({"schema_version": VERSION, "verdict": "error", "code": error.code, "message": error.message});
                let _ = writeln!(io::stdout().lock(), "{value}");
            } else {
                eprintln!("{}: {}", error.code, error.message);
            }
            ExitCode::from(error.exit)
        }
    }
}
