use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "reconform-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let f = Self(dir);
        f.put("left.json", &bundle());
        f.put("right.json", &bundle());
        f.put("plan.json", &plan());
        f
    }
    fn put(&self, name: &str, value: &Value) {
        fs::write(self.0.join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }
    fn compare(&self) -> Output {
        self.command()
            .args([
                "compare",
                "left.json",
                "right.json",
                "--plan",
                "plan.json",
                "--format",
                "json",
            ])
            .output()
            .unwrap()
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_reconform"));
        c.current_dir(&self.0);
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn bundle() -> Value {
    json!({"schema_version":"1.0","scenario":"sample","subject":"subject","provenance":"test fixture","environment":"same","checkpoints":["done"],"observations":[{"checkpoint":"done","channel":"stdout","occurrence":0,"completeness":"complete","payload":{"kind":"text","value":"hello"}}]})
}
fn plan() -> Value {
    json!({"schema_version":"1.0","scenario":"sample","strict_environment":false,"checks":[{"id":"output","checkpoint":"done","channel":"stdout","occurrence":0,"required":true,"policy":{"kind":"exact"}}]})
}
fn parsed(output: &Output, exit: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn json_text_policies_and_structured_contracts() {
    let f = Fixture::new();
    let mut a = bundle();
    let mut b = bundle();
    let mut p = plan();
    a["observations"][0]["payload"] =
        json!({"kind":"json","value":{"total":10,"state":{"ready":true}}});
    b["observations"][0]["payload"] =
        json!({"kind":"json","value":{"state":{"ready":true},"total":10}});
    f.put("left.json", &a);
    f.put("right.json", &b);
    parse_success(&f.compare());
    p["checks"][0]["policy"] = json!({"kind":"json","pointer":"/total","tolerance":1.0});
    b["observations"][0]["payload"]["value"]["total"] = json!(11);
    f.put("plan.json", &p);
    f.put("right.json", &b);
    parsed(&f.compare(), 0);
    b["observations"][0]["payload"]["value"]["total"] = json!(12);
    f.put("right.json", &b);
    parsed(&f.compare(), 1);
    p["checks"][0]["policy"]["pointer"] = json!("/missing");
    f.put("plan.json", &p);
    assert_eq!(
        parsed(&f.compare(), 4)["findings"][0]["code"],
        "JSON_PATH_MISSING"
    );
    p["checks"][0]["policy"]["pointer"] = json!("/bad~2");
    f.put("plan.json", &p);
    parsed(&f.compare(), 2);
    p["checks"][0]["policy"] =
        json!({"kind":"text","normalize_newlines":true,"trim_trailing_whitespace":true});
    a["observations"][0]["payload"] = json!({"kind":"text","value":"hello \r\n"});
    b["observations"][0]["payload"] = json!({"kind":"text","value":"hello\n"});
    f.put("left.json", &a);
    f.put("right.json", &b);
    f.put("plan.json", &p);
    parsed(&f.compare(), 0);
    let mut c = json!({"schema_version":"1.0","scenario":"sample","checks":[{"id":"contract","checkpoint":"done","channel":"stdout","occurrence":0,"required":true,"expression":{"kind":"equals","value":{"kind":"text","value":"hello\n"}}}]});
    let run = || {
        f.command()
            .args([
                "check",
                "right.json",
                "--plan",
                "contracts.json",
                "--format",
                "json",
            ])
            .output()
            .unwrap()
    };
    f.put("contracts.json", &c);
    parsed(&run(), 0);
    b["observations"][0]["payload"] =
        json!({"kind":"events","value":["load","noise","save","load"]});
    f.put("right.json", &b);
    c["checks"][0]["expression"] = json!({"kind":"event_order","events":["load","save"]});
    f.put("contracts.json", &c);
    parsed(&run(), 0);
    c["checks"][0]["expression"] = json!({"kind":"event_order","events":["save","save"]});
    f.put("contracts.json", &c);
    parsed(&run(), 1);
    c["checks"][0]["expression"] = json!({"kind":"event_contains","event":"noise"});
    f.put("contracts.json", &c);
    parsed(&run(), 0);
    b["observations"][0]["payload"] = json!({"kind":"json","value":{"ready":null}});
    f.put("right.json", &b);
    c["checks"][0]["expression"] = json!({"kind":"json_exists","pointer":"/ready"});
    f.put("contracts.json", &c);
    parsed(&run(), 0);
    c["checks"][0]["expression"]["pointer"] = json!("/missing");
    f.put("contracts.json", &c);
    parsed(&run(), 1);
}

fn parse_success(output: &Output) {
    parsed(output, 0);
}

#[test]
fn saved_reports_junit_init_and_json_integrity() {
    let f = Fixture::new();
    let init = parsed(
        &f.command()
            .args(["init", "example", "--format", "json"])
            .output()
            .unwrap(),
        0,
    );
    let args: Vec<_> = init["compare"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|v| v.as_str().unwrap())
        .collect();
    parsed(&f.command().args(args).output().unwrap(), 1);
    assert!(
        parsed(
            &f.command()
                .args(["doctor", "--format", "json"])
                .output()
                .unwrap(),
            0
        )["capture_available"]
            .as_bool()
            .unwrap()
    );
    let mut b = bundle();
    b["observations"][0]["payload"]["value"] = json!("different");
    f.put("right.json", &b);
    let report = parsed(&f.compare(), 1);
    f.put("report.json", &report);
    let junit = f
        .command()
        .args(["report", "report.json", "--format", "junit"])
        .output()
        .unwrap();
    assert_eq!(junit.status.code(), Some(1));
    let xml = String::from_utf8(junit.stdout).unwrap();
    assert!(xml.contains("failures=\"1\""));
    assert!(xml.contains("<failure "));
    let mut wrong = report;
    wrong["verdict"] = json!("pass");
    f.put("report.json", &wrong);
    parsed(
        &f.command()
            .args(["report", "report.json", "--format", "json"])
            .output()
            .unwrap(),
        2,
    );
    let mut p = plan();
    p["checks"][0]["policy"] = json!({"kind":"json","pointer":"","tolerance":0});
    f.put("plan.json", &p);
    b["observations"][0]["payload"] = json!({"kind":"text","value":"{\"n\":1,\"n\":2}"});
    f.put("left.json", &b);
    f.put("right.json", &b);
    parsed(&f.compare(), 3);
    b["observations"][0]["payload"]["value"] = json!("{\"n\":18446744073709551616}");
    f.put("left.json", &b);
    b["observations"][0]["payload"]["value"] = json!("{\"n\":18446744073709551617}");
    f.put("right.json", &b);
    parsed(&f.compare(), 1);
    p["checks"][0]["policy"] = json!({"kind":"json","pointer":"/n","tolerance":1});
    f.put("plan.json", &p);
    assert_eq!(
        parsed(&f.compare(), 4)["findings"][0]["code"],
        "JSON_NUMBER_PRECISION"
    );
}
#[test]
fn pass_fail_rerun_and_deterministic_reports() {
    let f = Fixture::new();
    parsed(&f.compare(), 0);
    let mut b = bundle();
    b["observations"][0]["payload"]["value"] = json!("hallo");
    f.put("right.json", &b);
    let first = f.compare();
    let report = parsed(&first, 1);
    assert_eq!(
        report["findings"][0]["location"],
        json!({"kind":"byte","index":1})
    );
    assert_eq!(report["first_divergence"], "output");
    assert_eq!(first.stdout, f.compare().stdout);
    let args: Vec<_> = report["reproduce"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|v| v.as_str().unwrap())
        .collect();
    let rerun = Command::new(env!("CARGO_BIN_EXE_reconform"))
        .args(args)
        .output()
        .unwrap();
    assert_eq!(first.stdout, rerun.stdout);
}
#[test]
fn absence_and_incomplete_captures_never_pass() {
    let f = Fixture::new();
    for completeness in ["partial", "unknown", "unavailable"] {
        let mut b = bundle();
        b["observations"][0]["completeness"] = json!(completeness);
        f.put("right.json", &b);
        assert_eq!(
            parsed(&f.compare(), 4)["findings"][0]["code"],
            "OBSERVATION_PARTIAL"
        );
    }
    let mut b = bundle();
    b["observations"] = json!([]);
    f.put("right.json", &b);
    f.put("left.json", &b);
    assert_eq!(
        parsed(&f.compare(), 4)["findings"][0]["code"],
        "OBSERVATION_MISSING"
    );
}
#[test]
fn invalid_schema_ids_duplicates_and_empty_gate_are_rejected() {
    let f = Fixture::new();
    for (field, value) in [
        ("schema_version", json!("2.0")),
        ("scenario", json!("../escape")),
        ("extra", json!(true)),
        ("checkpoints", json!(["done", "done"])),
    ] {
        let mut b = bundle();
        b[field] = value;
        f.put("right.json", &b);
        parsed(&f.compare(), 2);
    }
    let mut b = bundle();
    let duplicate = b["observations"][0].clone();
    b["observations"].as_array_mut().unwrap().push(duplicate);
    f.put("right.json", &b);
    parsed(&f.compare(), 2);
    f.put("right.json", &bundle());
    let mut p = plan();
    p["checks"] = json!([]);
    f.put("plan.json", &p);
    parsed(&f.compare(), 2);
    let mut p = plan();
    p["checks"][0]["required"] = json!(false);
    f.put("plan.json", &p);
    parsed(&f.compare(), 2);
}
#[test]
fn environment_kinds_and_policies_are_explicit() {
    let f = Fixture::new();
    let mut p = plan();
    p["strict_environment"] = json!(true);
    f.put("plan.json", &p);
    let mut b = bundle();
    b["environment"] = Value::Null;
    f.put("right.json", &b);
    parsed(&f.compare(), 4);
    f.put("plan.json", &plan());
    b["observations"][0]["payload"] = json!({"kind":"exit","value":0});
    f.put("right.json", &b);
    assert_eq!(
        parsed(&f.compare(), 4)["findings"][0]["code"],
        "KIND_MISMATCH"
    );
    f.put("right.json", &bundle());
    let mut p = plan();
    p["checks"][0]["policy"] = json!({"kind":"absolute","tolerance":1});
    f.put("plan.json", &p);
    assert_eq!(
        parsed(&f.compare(), 4)["findings"][0]["verdict"],
        "unsupported"
    );
}
#[test]
fn numeric_events_bytes_and_exit_comparisons() {
    let f = Fixture::new();
    for (kind, a, b) in [
        ("scalar", json!(12.0), json!(13.0)),
        ("events", json!(["a", "b"]), json!(["a"])),
        ("bytes", json!([0, 255]), json!([0, 254])),
        ("exit", json!(0), json!(1)),
    ] {
        let mut left = bundle();
        left["observations"][0]["payload"] = json!({"kind":kind,"value":a});
        f.put("left.json", &left);
        f.put("right.json", &left);
        parsed(&f.compare(), 0);
        let mut right = left;
        right["observations"][0]["payload"]["value"] = b;
        f.put("right.json", &right);
        parsed(&f.compare(), 1);
    }
    let mut b = bundle();
    b["observations"][0]["payload"] = json!({"kind":"scalar","value":12.5});
    f.put("left.json", &b);
    b["observations"][0]["payload"]["value"] = json!(13.0);
    f.put("right.json", &b);
    let mut p = plan();
    p["checks"][0]["policy"] = json!({"kind":"absolute","tolerance":0.5});
    f.put("plan.json", &p);
    parsed(&f.compare(), 0);
}
#[test]
fn errors_do_not_echo_payload_secrets() {
    let f = Fixture::new();
    let secret = "secret-canary-not-for-reports";
    fs::write(
        f.0.join("right.json"),
        format!("{{\"{secret}\":\"{secret}\"}}"),
    )
    .unwrap();
    let output = f.compare();
    parsed(&output, 2);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    fs::remove_file(f.0.join("right.json")).unwrap();
    parsed(&f.compare(), 3);
}
#[test]
fn input_limit_and_duplicate_fields() {
    let f = Fixture::new();
    fs::write(f.0.join("right.json"), vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();
    parsed(&f.compare(), 2);
    let text =
        serde_json::to_string(&bundle())
            .unwrap()
            .replacen("{", "{\"scenario\":\"duplicate\",", 1);
    fs::write(f.0.join("right.json"), text).unwrap();
    parsed(&f.compare(), 2);
}
#[test]
fn atomic_output_refuses_overwrite_and_preserves_input() {
    let f = Fixture::new();
    let args = [
        "compare",
        "left.json",
        "right.json",
        "--plan",
        "plan.json",
        "--format",
        "json",
        "--output",
        "report.json",
    ];
    let out = f.command().args(args).output().unwrap();
    parsed(&out, 0);
    assert_eq!(fs::read(f.0.join("report.json")).unwrap(), out.stdout);
    parsed(&f.command().args(args).output().unwrap(), 3);
    assert_eq!(fs::read(f.0.join("report.json")).unwrap(), out.stdout);
    assert!(
        !fs::read_dir(&f.0).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "tmp"))
    );
}
#[test]
fn validate_and_usage_errors() {
    let f = Fixture::new();
    parsed(
        &f.command()
            .args([
                "validate",
                "left.json",
                "--kind",
                "bundle",
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
        0,
    );
    for args in [
        vec!["compare", "--format", "json"],
        vec!["compare", "left.json", "right.json", "--format", "json"],
        vec!["unknown", "--format", "json"],
        vec![
            "validate",
            "left.json",
            "--kind",
            "bundle",
            "--format",
            "json",
            "--kind",
            "plan",
        ],
    ] {
        parsed(&f.command().args(args).output().unwrap(), 2);
    }
}

#[test]
fn independent_program_recordings_compare_end_to_end() {
    let f = Fixture::new();
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/recorded");
    for (name, output) in [("reference", "left.json"), ("candidate", "right.json")] {
        let exe = f.0.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let compiled = Command::new("rustc")
            .arg(examples.join(format!("{name}.rs")))
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let recording = Command::new(exe).output().unwrap();
        assert!(recording.status.success());
        fs::write(f.0.join(output), recording.stdout).unwrap();
    }
    fs::copy(examples.join("plan.json"), f.0.join("plan.json")).unwrap();
    assert_eq!(
        parsed(&f.compare(), 1)["first_divergence"],
        "greeting-output"
    );
}

#[test]
fn report_semantics_match_golden_fixtures() {
    let f = Fixture::new();
    for (name, exit) in [("pass", 0), ("fail", 1), ("inconclusive", 4)] {
        let mut b = bundle();
        if name == "fail" {
            b["observations"][0]["payload"]["value"] = json!("hallo");
        }
        if name == "inconclusive" {
            b["observations"] = json!([]);
        }
        f.put("right.json", &b);
        let report = parsed(&f.compare(), exit);
        // Only this documented semantic projection excludes fixture-specific paths/digests.
        let projection = json!({"verdict":report["verdict"],"first_divergence":report["first_divergence"],"findings":report["findings"]});
        let golden: Value = serde_json::from_slice(
            &fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/golden/{name}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(projection, golden);
        let input_path = report["inputs"][0]["path"].as_str().unwrap();
        assert_eq!(
            report["inputs"][0]["digest"],
            format!(
                "blake3:{}",
                blake3::hash(&fs::read(input_path).unwrap()).to_hex()
            )
        );
    }
}

#[test]
fn ordering_optional_findings_and_policy_identity() {
    let f = Fixture::new();
    let mut b = bundle();
    let mut second = b["observations"][0].clone();
    second["channel"] = json!("optional");
    b["observations"].as_array_mut().unwrap().push(second);
    f.put("left.json", &b);
    b["observations"][1]["payload"]["value"] = json!("changed");
    f.put("right.json", &b);
    let mut p = plan();
    let mut optional = p["checks"][0].clone();
    optional["id"] = json!("optional-check");
    optional["channel"] = json!("optional");
    optional["required"] = json!(false);
    p["checks"].as_array_mut().unwrap().push(optional);
    f.put("plan.json", &p);
    let original = parsed(&f.compare(), 0);
    assert_eq!(original["findings"][0]["verdict"], "fail");
    assert_eq!(original["first_divergence"], "optional-check");
    b["observations"].as_array_mut().unwrap().reverse();
    f.put("right.json", &b);
    assert_eq!(parsed(&f.compare(), 0)["findings"], original["findings"]);
    p["strict_environment"] = json!(true);
    f.put("plan.json", &p);
    assert_ne!(
        parsed(&f.compare(), 0)["policy_digest"],
        original["policy_digest"]
    );
}

fn contracts() -> Value {
    json!({"schema_version":"1.0","scenario":"sample","checks":[{"id":"output","checkpoint":"done","channel":"stdout","occurrence":0,"required":true,"expression":{"kind":"non_empty"}}]})
}
fn check(f: &Fixture) -> Output {
    f.command()
        .args([
            "check",
            "right.json",
            "--plan",
            "contracts.json",
            "--format",
            "json",
        ])
        .output()
        .unwrap()
}
#[test]
fn candidate_only_contracts_preserve_unknowns() {
    let f = Fixture::new();
    f.put("contracts.json", &contracts());
    parsed(&check(&f), 0);
    let mut b = bundle();
    b["observations"][0]["payload"]["value"] = json!("");
    f.put("right.json", &b);
    parsed(&check(&f), 1);
    b["observations"][0]["completeness"] = json!("partial");
    f.put("right.json", &b);
    parsed(&check(&f), 4);
    b["observations"] = json!([]);
    f.put("right.json", &b);
    parsed(&check(&f), 4);
    let mut p = contracts();
    p["checks"][0]["expression"] = json!({"kind":"exists"});
    f.put("contracts.json", &p);
    assert_eq!(
        parsed(&check(&f), 1)["findings"][0]["code"],
        "CONTRACT_VIOLATED"
    );
    f.put("right.json", &bundle());
    parsed(&check(&f), 0);
}
#[test]
fn contract_bounds_exit_and_reproduction() {
    let f = Fixture::new();
    let mut p = contracts();
    p["checks"][0]["expression"] = json!({"kind":"between","min":2.0,"max":3.0});
    f.put("contracts.json", &p);
    let mut b = bundle();
    b["observations"][0]["payload"] = json!({"kind":"scalar","value":3.0});
    f.put("right.json", &b);
    parsed(&check(&f), 0);
    b["observations"][0]["payload"]["value"] = json!(4.0);
    f.put("right.json", &b);
    let result = check(&f);
    let report = parsed(&result, 1);
    let argv: Vec<_> = report["reproduce"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        result.stdout,
        Command::new(env!("CARGO_BIN_EXE_reconform"))
            .args(argv)
            .output()
            .unwrap()
            .stdout
    );
    p["checks"][0]["expression"] = json!({"kind":"between","min":4.0,"max":3.0});
    f.put("contracts.json", &p);
    parsed(&check(&f), 2);
    p["checks"][0]["expression"] = json!({"kind":"exit_success"});
    f.put("contracts.json", &p);
    parsed(&check(&f), 4);
    b["observations"][0]["payload"] = json!({"kind":"exit","value":0});
    f.put("right.json", &b);
    parsed(&check(&f), 0);
    b["observations"][0]["payload"]["value"] = json!(1);
    f.put("right.json", &b);
    parsed(&check(&f), 1);
}
#[test]
fn contract_validation_rejects_empty_gates_duplicates_and_unknown_fields() {
    let f = Fixture::new();
    f.put("contracts.json", &contracts());
    parsed(
        &f.command()
            .args([
                "validate",
                "contracts.json",
                "--kind",
                "contracts",
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
        0,
    );
    let mut p = contracts();
    p["checks"] = json!([]);
    f.put("contracts.json", &p);
    parsed(&check(&f), 2);
    let mut p = contracts();
    p["checks"][0]["required"] = json!(false);
    f.put("contracts.json", &p);
    parsed(&check(&f), 2);
    let mut p = contracts();
    let duplicate = p["checks"][0].clone();
    p["checks"].as_array_mut().unwrap().push(duplicate);
    f.put("contracts.json", &p);
    parsed(&check(&f), 2);
    let mut p = contracts();
    p["checks"][0]["expression"]["ignored"] = json!(true);
    f.put("contracts.json", &p);
    parsed(&check(&f), 2);
}

#[test]
fn parameterless_policies_reject_unknown_fields() {
    let f = Fixture::new();
    let mut p = plan();
    p["checks"][0]["policy"]["tolerence"] = json!(99);
    f.put("plan.json", &p);
    parsed(&f.compare(), 2);
    for kind in ["exists", "non_empty", "exit_success"] {
        let mut p = contracts();
        p["checks"][0]["expression"] = json!({"kind":kind,"ignored":true});
        f.put("contracts.json", &p);
        parsed(&check(&f), 2);
    }
}

#[test]
fn directory_inputs_fail_without_reading() {
    let f = Fixture::new();
    parsed(
        &f.command()
            .args(["validate", ".", "--kind", "bundle", "--format", "json"])
            .output()
            .unwrap(),
        2,
    );
}

#[cfg(unix)]
#[test]
fn fifo_inputs_are_rejected_and_reports_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    assert!(
        Command::new("mkfifo")
            .arg(f.0.join("pipe"))
            .status()
            .unwrap()
            .success()
    );
    let mut child = f
        .command()
        .args(["validate", "pipe", "--kind", "bundle", "--format", "json"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert_eq!(status.code(), Some(2));
            break;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("FIFO input blocked");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    parsed(
        &f.command()
            .args([
                "compare",
                "left.json",
                "right.json",
                "--plan",
                "plan.json",
                "--format",
                "json",
                "--output",
                "report.json",
            ])
            .output()
            .unwrap(),
        0,
    );
    assert_eq!(
        fs::metadata(f.0.join("report.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn collection_limits_and_truncated_utf8_are_rejected() {
    let f = Fixture::new();
    let mut b = bundle();
    b["observations"][0]["payload"] = json!({"kind":"events","value":vec!["x";10001]});
    f.put("right.json", &b);
    parsed(&f.compare(), 2);
    let mut b = bundle();
    b["observations"] = json!(vec![b["observations"][0].clone(); 1025]);
    f.put("right.json", &b);
    parsed(&f.compare(), 2);
    f.put("right.json", &bundle());
    let mut p = plan();
    p["checks"] = json!(vec![p["checks"][0].clone(); 1025]);
    f.put("plan.json", &p);
    parsed(&f.compare(), 2);
    f.put("plan.json", &plan());
    for bytes in [
        vec![0xff],
        vec![0xef, 0xbb],
        b"{\"schema_version\":\"1.0\"".to_vec(),
    ] {
        fs::write(f.0.join("right.json"), bytes).unwrap();
        parsed(&f.compare(), 2);
    }
}
