use reconform_core::{Check, Verdict, aggregate, compare_bytes};

// A caller-owned comparison composes with the gate without a registry or runtime.
fn compare_ascii_case_insensitive(a: &str, b: &str) -> Verdict {
    if a.eq_ignore_ascii_case(b) {
        Verdict::Pass
    } else {
        Verdict::Fail
    }
}
fn main() {
    assert_eq!(compare_bytes(b"a", b"b"), Verdict::Fail);
    let verdict = compare_ascii_case_insensitive("HELLO", "hello");
    assert_eq!(
        aggregate([Check {
            required: true,
            verdict
        }]),
        Verdict::Pass
    );
}
