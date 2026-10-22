use crate::{
    Error, Options, digest, emit,
    model::{Bundle, Completeness, Observation, Payload, VERSION, items},
    read, read_bytes, write_new,
};
use process_wrap::std::{ChildWrapper, CommandWrap};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
static CANCELLED: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: String,
    scenario: String,
    subject: String,
    #[serde(deserialize_with = "items")]
    steps: Vec<Step>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Step {
    checkpoint: String,
    executable: String,
    #[serde(deserialize_with = "items")]
    args: Vec<String>,
    cwd: String,
    env: BTreeMap<String, String>,
    stdin: String,
    timeout_ms: u64,
    max_output_bytes: usize,
    #[serde(default, deserialize_with = "items")]
    files: Vec<CaptureFile>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CaptureFile {
    channel: String,
    path: String,
    max_bytes: usize,
}
struct Prepared {
    executable: PathBuf,
    cwd: PathBuf,
    fingerprint: String,
}

fn prepare(manifest: &Manifest, root: Option<&Path>) -> Result<Vec<Prepared>, Error> {
    if manifest.schema_version != VERSION
        || manifest.steps.is_empty()
        || manifest.steps.len() > 32
        || manifest.steps.iter().fold(0usize, |n, s| {
            n.saturating_add(s.max_output_bytes.saturating_mul(8))
                .saturating_add(s.files.iter().fold(0usize, |n, f| {
                    n.saturating_add(f.max_bytes.saturating_mul(4))
                }))
        }) > 7 * 1024 * 1024
    {
        return Err(Error::config(
            "capture requires schema 1.0, 1..32 steps and total serialized output budget below 7 MiB",
        ));
    }
    // Reuse the recorded-format invariants before executing anything.
    Bundle {
        schema_version: VERSION.into(),
        scenario: manifest.scenario.clone(),
        subject: manifest.subject.clone(),
        provenance: "preflight".into(),
        environment: None,
        checkpoints: manifest
            .steps
            .iter()
            .map(|s| s.checkpoint.clone())
            .collect(),
        observations: vec![],
    }
    .validate()
    .map_err(Error::config)?;
    let mut output_paths = std::collections::BTreeSet::new();
    manifest
        .steps
        .iter()
        .map(|step| {
            if !(1..=300_000).contains(&step.timeout_ms)
                || !(1..=262_144).contains(&step.max_output_bytes)
                || step.stdin.len() > 262_144
                || step
                    .args
                    .iter()
                    .any(|a| a.len() > 32768 || a.contains('\0'))
                || step.env.len() > 128
                || step.env.iter().any(|(k, v)| {
                    k.is_empty() || k.contains(['=', '\0']) || v.contains('\0') || v.len() > 32768
                })
            {
                return Err(Error::config(
                    "invalid process budget, argument or environment",
                ));
            }
            if !Path::new(&step.executable).is_absolute() || !Path::new(&step.cwd).is_absolute() {
                return Err(Error::config("executable and cwd must be absolute paths"));
            }
            let executable = fs::canonicalize(&step.executable).map_err(Error::io)?;
            let cwd = fs::canonicalize(&step.cwd).map_err(Error::io)?;
            let mut channels = std::collections::BTreeSet::from(["stdout", "stderr", "exit"]);
            if step.files.len() > 16 {
                return Err(Error::config("at most 16 file captures per step"));
            }
            for file in &step.files {
                if !crate::model::id(&file.channel)
                    || !channels.insert(&file.channel)
                    || !(1..=1024 * 1024).contains(&file.max_bytes)
                    || file.path.is_empty()
                    || !Path::new(&file.path)
                        .components()
                        .all(|p| matches!(p, std::path::Component::Normal(_)))
                {
                    return Err(Error::config(
                        "invalid captured channel, relative file path or file budget",
                    ));
                }
                let path = cwd.join(&file.path);
                if !output_paths.insert(path.clone()) {
                    return Err(Error::config("steps must use distinct captured file paths"));
                }
                if fs::symlink_metadata(&path).is_ok() {
                    return Err(Error::config(
                        "captured output must not exist before launch",
                    ));
                }
                let parent = fs::canonicalize(
                    path.parent()
                        .ok_or_else(|| Error::config("invalid captured path"))?,
                )
                .map_err(Error::io)?;
                if !parent.starts_with(&cwd) {
                    return Err(Error::config("captured path escapes working directory"));
                }
            }
            if !executable.is_file()
                || !cwd.is_dir()
                || root.is_some_and(|r| !executable.starts_with(r) || !cwd.starts_with(r))
            {
                return Err(Error::config(
                    "executable/cwd must exist within the explicitly allowed root",
                ));
            }
            #[cfg(windows)]
            if executable
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("exe"))
            {
                return Err(Error::config(
                    "Windows capture accepts native .exe files only",
                ));
            }
            let (_, identity) = read_bytes(&step.executable, 256 * 1024 * 1024)?;
            Ok(Prepared {
                executable,
                cwd,
                fingerprint: identity.digest,
            })
        })
        .collect()
}

// A guard covers errors during pipe setup, reading, waiting and serialization.
struct ChildGuard(Box<dyn ChildWrapper>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.start_kill();
    }
}
enum Message {
    Output(usize, Vec<u8>),
    End(usize),
    InputDone,
    Io,
}

fn pipe(mut input: impl Read + Send + 'static, channel: usize, tx: mpsc::SyncSender<Message>) {
    thread::spawn(move || {
        let mut buffer = [0; 4096];
        loop {
            match input.read(&mut buffer) {
                Ok(0) => {
                    let _ = tx.send(Message::End(channel));
                    break;
                }
                Ok(n) => {
                    if tx
                        .send(Message::Output(channel, buffer[..n].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    let _ = tx.send(Message::Io);
                    break;
                }
            }
        }
    });
}

fn run(step: &Step, prepared: &Prepared) -> Result<(Vec<Observation>, &'static str), Error> {
    let mut command = CommandWrap::with_new(&prepared.executable, |c| {
        c.args(&step.args)
            .current_dir(&prepared.cwd)
            .env_clear()
            .envs(&step.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    #[cfg(unix)]
    command.wrap(process_wrap::std::ProcessGroup::leader());
    #[cfg(windows)]
    {
        command.wrap(process_wrap::std::JobObject);
        command.wrap(process_wrap::std::CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW,
        ));
    }
    let mut child = ChildGuard(command.spawn().map_err(Error::io)?);
    let (tx, rx) = mpsc::sync_channel(8);
    let stdout = child
        .0
        .stdout()
        .take()
        .ok_or_else(|| Error::config("stdout pipe unavailable"))?;
    let stderr = child
        .0
        .stderr()
        .take()
        .ok_or_else(|| Error::config("stderr pipe unavailable"))?;
    let mut stdin = child
        .0
        .stdin()
        .take()
        .ok_or_else(|| Error::config("stdin pipe unavailable"))?;
    pipe(stdout, 0, tx.clone());
    pipe(stderr, 1, tx.clone());
    let input = step.stdin.clone();
    thread::spawn(move || {
        let message = if stdin.write_all(input.as_bytes()).is_err() {
            Message::Io
        } else {
            Message::InputDone
        };
        drop(stdin);
        let _ = tx.send(message);
    });
    let start = Instant::now();
    let mut streams = [Vec::new(), Vec::new()];
    let mut closed = [false; 2];
    let mut status = None;
    let mut input_done = false;
    let reason = loop {
        if CANCELLED.load(Ordering::Relaxed) {
            break "CANCELLED";
        }
        if start.elapsed() >= Duration::from_millis(step.timeout_ms) {
            break "TIMEOUT";
        }
        if status.is_none() {
            status = child.0.try_wait().map_err(Error::io)?;
        }
        if status.is_some() && closed == [true, true] && input_done {
            break "COMPLETE";
        }
        match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(Message::Output(channel, data)) => {
                let remaining = step.max_output_bytes - streams[channel].len();
                streams[channel].extend_from_slice(&data[..data.len().min(remaining)]);
                if data.len() > remaining {
                    break "OUTPUT_LIMIT";
                }
            }
            Ok(Message::End(channel)) => closed[channel] = true,
            Ok(Message::InputDone) => input_done = true,
            Ok(Message::Io) => break "CAPTURE_IO",
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !input_done || closed != [true, true] {
                    break "CAPTURE_IO";
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }
    };
    // Terminate descendants even when the parent has already exited. Never wait
    // indefinitely for a descendant that deliberately escapes its process group.
    let _ = child.0.start_kill();
    let cleanup = Instant::now();
    while child.0.try_wait().map_err(Error::io)?.is_none()
        && cleanup.elapsed() < Duration::from_secs(2)
    {
        thread::sleep(Duration::from_millis(5));
    }
    let complete = reason == "COMPLETE";
    let mut observations = Vec::new();
    for (channel, bytes) in ["stdout", "stderr"].into_iter().zip(streams) {
        observations.push(Observation {
            checkpoint: step.checkpoint.clone(),
            channel: channel.into(),
            occurrence: 0,
            completeness: if complete {
                Completeness::Complete
            } else {
                Completeness::Partial
            },
            payload: Payload::Bytes(bytes),
        });
    }
    observations.push(Observation {
        checkpoint: step.checkpoint.clone(),
        channel: "exit".into(),
        occurrence: 0,
        completeness: if complete && status.and_then(|s| s.code()).is_some() {
            Completeness::Complete
        } else {
            Completeness::Unavailable
        },
        payload: Payload::Exit(status.and_then(|s| s.code()).unwrap_or(-1)),
    });
    Ok((observations, reason))
}

pub struct Capabilities {
    pub scenario: String,
    pub checkpoints: Vec<String>,
    pub channels: std::collections::BTreeSet<(String, String)>,
    pub files: std::collections::BTreeSet<PathBuf>,
}
pub fn preflight(path: &str, root: &Path) -> Result<Capabilities, Error> {
    let (manifest, _) = read::<Manifest>(path)?;
    let prepared = prepare(&manifest, Some(root))?;
    let mut channels = std::collections::BTreeSet::new();
    let mut files = std::collections::BTreeSet::new();
    for (step, prepared) in manifest.steps.iter().zip(prepared) {
        for channel in ["stdout", "stderr", "exit"]
            .into_iter()
            .chain(step.files.iter().map(|f| f.channel.as_str()))
        {
            channels.insert((step.checkpoint.clone(), channel.into()));
        }
        for file in &step.files {
            files.insert(prepared.cwd.join(&file.path));
        }
    }
    Ok(Capabilities {
        scenario: manifest.scenario,
        checkpoints: manifest.steps.into_iter().map(|s| s.checkpoint).collect(),
        channels,
        files,
    })
}

pub fn execute(opts: &Options, dry_run: bool, quiet: bool) -> Result<u8, Error> {
    if opts.positional.len() != 1 || opts.kind.is_some() || opts.plan.is_some() {
        return Err(Error::config(
            "capture/plan requires MANIFEST; capture also requires --allow-exec ROOT --output BUNDLE",
        ));
    }
    let root = opts
        .allow_exec
        .as_ref()
        .map(fs::canonicalize)
        .transpose()
        .map_err(Error::io)?;
    if !dry_run && (root.is_none() || opts.output.is_none()) {
        return Err(Error::config(
            "capture requires --allow-exec ROOT and --output BUNDLE",
        ));
    }
    if opts.output.as_ref().is_some_and(|p| Path::new(p).exists()) {
        return Err(Error::config("output already exists"));
    }
    if let Some(output) = &opts.output {
        let parent = Path::new(output)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !parent.is_dir() {
            return Err(Error::config("output parent must be an existing directory"));
        }
    }
    let (manifest, identity) = read::<Manifest>(&opts.positional[0])?;
    let prepared = prepare(&manifest, root.as_deref())?;
    if dry_run {
        let report = json!({"schema_version":VERSION,"method":"process-plan/1","manifest":identity,"steps":manifest.steps.iter().zip(&prepared).map(|(s,p)| json!({"checkpoint":s.checkpoint,"executable":p.executable,"executable_digest":p.fingerprint,"cwd":p.cwd,"timeout_ms":s.timeout_ms,"max_output_bytes_per_stream":s.max_output_bytes,"environment_keys":s.env.keys().collect::<Vec<_>>()})).collect::<Vec<_>>(),"permissions":["Executes supplied native programs with the caller's privileges; no sandbox or network isolation.","Environment is cleared and only manifest entries are supplied."]});
        emit(
            &report,
            "Process plan validated; no target executed. Use --format json for details.\n",
            opts,
        )?;
        return Ok(0);
    }
    static HANDLER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !HANDLER
        .get_or_init(|| ctrlc::set_handler(|| CANCELLED.store(true, Ordering::Relaxed)).is_ok())
    {
        return Err(Error::config("cannot install process cancellation handler"));
    }
    let mut observations = Vec::new();
    let mut outcomes = Vec::new();
    for (step, p) in manifest.steps.iter().zip(&prepared) {
        let (mut captured, mut reason) = run(step, p)?;
        for file in &step.files {
            let path = p.cwd.join(&file.path);
            let bytes = fs::canonicalize(&path)
                .ok()
                .filter(|p2| p2.starts_with(&p.cwd))
                .and_then(|p2| {
                    p2.to_str()
                        .and_then(|s| read_bytes(s, file.max_bytes as u64).ok())
                });
            let (payload, completeness) = match bytes {
                Some((bytes, _)) => (
                    Payload::Bytes(bytes),
                    if reason == "COMPLETE" {
                        Completeness::Complete
                    } else {
                        Completeness::Partial
                    },
                ),
                None => {
                    reason = "FILE_CAPTURE_ERROR";
                    (Payload::Bytes(vec![]), Completeness::Unavailable)
                }
            };
            captured.push(Observation {
                checkpoint: step.checkpoint.clone(),
                channel: file.channel.clone(),
                occurrence: 0,
                completeness,
                payload,
            });
        }
        observations.extend(captured);
        outcomes.push(reason);
        if reason != "COMPLETE" {
            break;
        }
    }
    let provenance = json!({"method":"process-capture/1","manifest_digest":identity.digest,"executables":prepared.iter().map(|p| &p.fingerprint).collect::<Vec<_>>(),"outcomes":outcomes}).to_string();
    let bundle = Bundle {
        schema_version: VERSION.into(),
        scenario: manifest.scenario,
        subject: manifest.subject,
        provenance,
        environment: Some(digest(
            &serde_json::to_vec(&manifest.steps.iter().map(|s| &s.env).collect::<Vec<_>>())
                .map_err(|_| Error::config("environment serialization failed"))?,
        )),
        checkpoints: manifest
            .steps
            .iter()
            .map(|s| s.checkpoint.clone())
            .collect(),
        observations,
    };
    bundle.validate().map_err(Error::config)?;
    let bytes =
        serde_json::to_vec(&bundle).map_err(|_| Error::config("capture serialization failed"))?;
    if bytes.len() as u64 > crate::model::MAX_BYTES {
        return Err(Error::config(
            "captured bundle exceeds 8 MiB; reduce step/output budgets",
        ));
    }
    write_new(
        Path::new(
            opts.output
                .as_deref()
                .ok_or_else(|| Error::config("missing output"))?,
        ),
        &bytes,
    )?;
    let complete = outcomes.iter().all(|r| *r == "COMPLETE");
    if quiet {
        return Ok(if complete { 0 } else { 4 });
    }
    let report = json!({"schema_version":VERSION,"method":"process-capture/1","verdict":if complete {"captured"} else {"inconclusive"},"outcomes":outcomes,"bundle_digest":digest(&bytes),"output":opts.output});
    let stdout_opts = Options {
        positional: vec![],
        plan: None,
        kind: None,
        output: None,
        json: opts.json,
        allow_exec: None,
        diff: None,
        junit: false,
    };
    emit(
        &report,
        if complete {
            "Captured observations; run compare or check to obtain a verification verdict.\n"
        } else {
            "Capture incomplete; partial evidence saved.\n"
        },
        &stdout_opts,
    )?;
    Ok(if complete { 0 } else { 4 })
}
