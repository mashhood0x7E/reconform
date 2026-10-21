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
    use reconform_core::{Contract, Evidence, ObservationView, evaluate_contract};
    assert_eq!(
        evaluate_contract(
            Contract::ExitSuccess,
            Evidence::Complete(ObservationView::Exit(0))
        ),
        Verdict::Pass
    );
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
