use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "reconform-integrations-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn put(&self, name: &str, value: &Value) {
        fs::write(self.0.join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_reconform"));
        c.current_dir(&self.0);
        c
    }
    fn executable(&self) -> PathBuf {
        let source = r#"use std::{io::{Read,Write},time::Duration};
fn main() {
 let args:Vec<String>=std::env::args().collect();
 match args[1].as_str() {
 "echo" => {let mut b=Vec::new();std::io::stdin().read_to_end(&mut b).unwrap();std::io::stdout().write_all(&b).unwrap();eprint!("{}:{}",args[2],std::env::var("RECONFORM_TEST").unwrap());},
 "sleep" => std::thread::sleep(Duration::from_secs(10)),
 "flood" => loop {std::io::stdout().write_all(&[65;4096]).unwrap();},
 "child" => {std::thread::sleep(Duration::from_millis(700));std::fs::write("escaped.txt","alive").unwrap();},
 "parent" => {let _child=std::process::Command::new(std::env::current_exe().unwrap()).arg("child").spawn().unwrap();std::thread::sleep(Duration::from_secs(10));},
 "exit" => std::process::exit(17),
 "file" => {std::fs::copy(&args[2], &args[3]).unwrap();},
 _ => panic!()
 }
}"#;
        fs::write(self.0.join("subject.rs"), source).unwrap();
        let exe = self
            .0
            .join(format!("subject{}", std::env::consts::EXE_SUFFIX));
        assert!(
            Command::new("rustc")
                .arg(self.0.join("subject.rs"))
                .arg("-o")
                .arg(&exe)
                .status()
                .unwrap()
                .success()
        );
        exe
    }
    fn manifest(&self, exe: &std::path::Path, mode: &str) -> Value {
        json!({"schema_version":"1.0","scenario":"capture-test","subject":"native","steps":[{"checkpoint":"done","executable":exe,"args":[mode,"literal ; & $()"],"cwd":self.0,"env":{"RECONFORM_TEST":"explicit"},"stdin":"hello\n","timeout_ms":3000,"max_output_bytes":65536}]})
    }
    fn capture(&self, name: &str) -> Output {
        self.command()
            .args(["capture", "capture.json", "--allow-exec"])
            .arg(&self.0)
            .args(["--output", name, "--format", "json"])
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn parse(output: Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn capture_is_explicit_bounded_and_reusable() {
    let f = Fixture::new();
    let exe = f.executable();
    let mut m = f.manifest(&exe, "echo");
    f.put("capture.json", &m);
    parse(
        f.command()
            .args([
                "capture",
                "capture.json",
                "--output",
                "denied.json",
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
        2,
    );
    assert!(!f.0.join("denied.json").exists());
    parse(
        f.command()
            .args(["plan", "capture.json", "--format", "json"])
            .output()
            .unwrap(),
        0,
    );
    parse(f.capture("bundle.json"), 0);
    let bundle: Value =
        serde_json::from_slice(&fs::read(f.0.join("bundle.json")).unwrap()).unwrap();
    assert_eq!(
        bundle["observations"][0]["payload"]["value"],
        json!(b"hello\n")
    );
    assert_eq!(
        bundle["observations"][1]["payload"]["value"],
        json!(b"literal ; & $():explicit")
    );
    assert_eq!(bundle["observations"][2]["payload"]["value"], 0);
    parse(
        f.command()
            .args([
                "validate",
                "bundle.json",
                "--kind",
                "bundle",
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
        0,
    );
    parse(f.capture("bundle.json"), 2);
    for (mode, reason) in [
        ("sleep", "TIMEOUT"),
        ("flood", "OUTPUT_LIMIT"),
        ("parent", "TIMEOUT"),
    ] {
        m["steps"][0]["args"][0] = json!(mode);
        m["steps"][0]["stdin"] = json!("");
        m["steps"][0]["timeout_ms"] = json!(200);
        f.put("capture.json", &m);
        let start = Instant::now();
        let report = parse(f.capture(&format!("{mode}.json")), 4);
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(report["outcomes"][0], reason);
        let b: Value =
            serde_json::from_slice(&fs::read(f.0.join(format!("{mode}.json"))).unwrap()).unwrap();
        assert_eq!(b["observations"][0]["completeness"], "partial");
    }
    std::thread::sleep(Duration::from_millis(800));
    assert!(
        !f.0.join("escaped.txt").exists(),
        "descendant survived cleanup"
    );
    m["steps"][0]["args"][0] = json!("exit");
    m["steps"][0]["timeout_ms"] = json!(3000);
    f.put("capture.json", &m);
    parse(f.capture("exit.json"), 0);
    let b: Value = serde_json::from_slice(&fs::read(f.0.join("exit.json")).unwrap()).unwrap();
    assert_eq!(b["observations"][2]["payload"]["value"], 17);
    m["steps"][0]["executable"] = json!(f.0.join("missing.exe"));
    f.put("capture.json", &m);
    parse(f.capture("missing.json"), 3);
    assert!(!f.0.join("missing.json").exists());
}

#[cfg(feature = "png")]
fn png_file(f: &Fixture, name: &str, width: u32, pixels: &[u8]) {
    let mut encoder = png::Encoder::new(fs::File::create(f.0.join(name)).unwrap(), width, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}
#[cfg(feature = "png")]
#[test]
fn png_comparison_reports_metrics_and_rejects_bad_inputs() {
    let f = Fixture::new();
    png_file(&f, "left.png", 2, &[0, 0, 0, 255, 10, 20, 30, 255]);
    png_file(&f, "right.png", 2, &[0, 0, 0, 255, 12, 20, 30, 255]);
    let mut policy = json!({"schema_version":"1.0","tolerance":0,"max_mismatch_fraction":0.0,"region":null,"assume_srgb":false});
    f.put("visual.json", &policy);
    let run = || {
        f.command()
            .args([
                "compare-png",
                "left.png",
                "right.png",
                "--plan",
                "visual.json",
                "--format",
                "json",
            ])
            .output()
            .unwrap()
    };
    let report = parse(run(), 1);
    assert_eq!(report["measurements"]["mismatched_pixels"], 1);
    assert_eq!(report["measurements"]["bounds"], json!([1, 0, 2, 1]));
    let diff = parse(
        f.command()
            .args([
                "compare-png",
                "left.png",
                "right.png",
                "--plan",
                "visual.json",
                "--diff",
                "diff.png",
                "--format",
                "json",
            ])
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(diff["diff"]["media_type"], "image/png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(
        fs::read(f.0.join("diff.png")).unwrap(),
    ))
    .read_info()
    .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    assert_eq!(pixels, vec![0, 0, 0, 255, 2, 0, 0, 255]);
    policy["tolerance"] = json!(2);
    f.put("visual.json", &policy);
    parse(run(), 0);
    policy["tolerance"] = json!(0);
    policy["region"] = json!([0, 0, 1, 1]);
    f.put("visual.json", &policy);
    parse(run(), 0);
    png_file(&f, "right.png", 1, &[0, 0, 0, 255]);
    parse(run(), 4);
    fs::write(f.0.join("right.png"), b"not a PNG").unwrap();
    parse(run(), 2);
    let mut bytes = fs::read(f.0.join("left.png")).unwrap();
    bytes.truncate(bytes.len() - 10);
    fs::write(f.0.join("right.png"), bytes).unwrap();
    parse(run(), 2);
}

#[cfg(feature = "png")]
#[test]
fn captured_pngs_flow_through_recorded_comparison() {
    let f = Fixture::new();
    let exe = f.executable();
    png_file(&f, "source.png", 1, &[3, 4, 5, 255]);
    let mut m = f.manifest(&exe, "file");
    m["steps"][0]["stdin"] = json!("");
    m["steps"][0]["args"] = json!(["file", "source.png", "shot.png"]);
    m["steps"][0]["files"] = json!([{"channel":"screenshot","path":"shot.png","max_bytes":4096}]);
    f.put("capture.json", &m);
    parse(f.capture("left.json"), 0);
    // Existing files cannot masquerade as newly captured evidence.
    parse(f.capture("stale.json"), 2);
    fs::remove_file(f.0.join("shot.png")).unwrap();
    png_file(&f, "source.png", 1, &[7, 4, 5, 255]);
    parse(f.capture("right.json"), 0);
    f.put("plan.json",&json!({"schema_version":"1.0","scenario":"capture-test","strict_environment":true,"checks":[{"id":"image","checkpoint":"done","channel":"screenshot","occurrence":0,"required":true,"policy":{"kind":"png","tolerance":0,"max_mismatch_fraction":0.0,"region":null,"assume_srgb":false}}]}));
    let report = parse(
        f.command()
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
            .unwrap(),
        1,
    );
    assert_eq!(
        report["findings"][0]["measurements"]["max_channel_error"],
        4
    );
    assert_eq!(report["first_divergence"], "image");
    m["steps"][0]["files"][0]["path"] = json!("../outside.png");
    f.put("capture.json", &m);
    parse(f.capture("traversal.json"), 2);
}
