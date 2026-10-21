# reconform-core

Pure byte comparisons, finite numeric comparisons, and verification gates.
No third-party dependencies, I/O, runtime initialization, or process execution.

```rust
use reconform_core::{Check, Verdict, aggregate, compare_bytes};
let verdict = compare_bytes(b"expected", b"actual");
assert_eq!(aggregate([Check { required: true, verdict }]), Verdict::Fail);
```

Required failures take precedence over verifier errors, then inconclusive results.
Missing required evidence must be represented as Inconclusive by the caller.
An empty gate returns Error; optional checks cannot establish a passing gate.
Comparisons borrow input slices. Numeric tolerance is absolute and inclusive;
NaN, infinity and invalid tolerances return a typed error.

Experimental API; no stable release or tested minimum Rust version yet.
Licensed under MIT OR Apache-2.0.

Candidate-only checks use the same gate semantics:

```rust
use reconform_core::{Contract, Evidence, ObservationView, Verdict, evaluate_contract};
let evidence = Evidence::Complete(ObservationView::Exit(0));
assert_eq!(evaluate_contract(Contract::ExitSuccess, evidence), Verdict::Pass);
```

`Evidence` distinguishes complete, missing and incomplete capture. Exists checks
recorded presence; NonEmpty, Between and ExitSuccess apply to matching value kinds.
