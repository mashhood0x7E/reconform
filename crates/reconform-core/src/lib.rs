//! Pure comparisons and verification gates, with no external dependencies.
//!
//! ```
//! use reconform_core::{compare_bytes, aggregate, Check, Verdict};
//! let verdict = compare_bytes(b"expected", b"actual");
//! assert_eq!(aggregate([Check { required: true, verdict }]), Verdict::Fail);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod contract;
pub use contract::{Contract, Evidence, ObservationView, evaluate_contract};

/// Result of a verification check, distinct from an application's exit code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// A supported check completed successfully.
    Pass,
    /// Comparable evidence violates the expectation.
    Fail,
    /// Evidence is missing, partial, or incomparable.
    Inconclusive,
    /// The requested comparison is unavailable.
    Unsupported,
    /// A check was intentionally not attempted.
    Skipped,
    /// The verifier could not perform the check.
    Error,
}

impl Verdict {
    /// Stable lowercase wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Inconclusive => "inconclusive",
            Self::Unsupported => "unsupported",
            Self::Skipped => "skipped",
            Self::Error => "error",
        }
    }
}

/// One gate input. Optional results remain visible but do not gate success.
#[derive(Clone, Copy, Debug)]
pub struct Check {
    /// Whether this check must pass.
    pub required: bool,
    /// Observed result.
    pub verdict: Verdict,
}

/// Aggregate required checks: fail, error, unknown, then pass.
///
/// No required checks yields Error. All skipped yields Skipped; a mix of skipped
/// and passed checks yields Inconclusive. Unsupported maps to Inconclusive.
#[must_use]
pub fn aggregate(checks: impl IntoIterator<Item = Check>) -> Verdict {
    let mut counts = [0usize; 6];
    for check in checks.into_iter().filter(|c| c.required) {
        let i = match check.verdict {
            Verdict::Pass => 0,
            Verdict::Fail => 1,
            Verdict::Error => 2,
            Verdict::Inconclusive | Verdict::Unsupported => 3,
            Verdict::Skipped => 4,
        };
        counts[i] += 1;
        counts[5] += 1;
    }
    if counts[1] > 0 {
        Verdict::Fail
    } else if counts[2] > 0 || counts[5] == 0 {
        Verdict::Error
    } else if counts[3] > 0 {
        Verdict::Inconclusive
    } else if counts[4] == counts[5] {
        Verdict::Skipped
    } else if counts[4] > 0 {
        Verdict::Inconclusive
    } else {
        Verdict::Pass
    }
}

/// Exact byte equality. Empty values are valid observations, not missing evidence.
#[must_use]
pub fn compare_bytes(left: &[u8], right: &[u8]) -> Verdict {
    if left == right {
        Verdict::Pass
    } else {
        Verdict::Fail
    }
}

/// First unequal byte offset, including the first absent byte for unequal lengths.
#[must_use]
pub fn first_difference(left: &[u8], right: &[u8]) -> Option<usize> {
    left.iter()
        .zip(right)
        .position(|(a, b)| a != b)
        .or_else(|| (left.len() != right.len()).then_some(left.len().min(right.len())))
}

/// Invalid scalar input or tolerance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NumericError;
impl std::fmt::Display for NumericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("values must be finite; tolerance must be finite and nonnegative")
    }
}
impl std::error::Error for NumericError {}

/// Compare finite numbers using an inclusive absolute tolerance.
///
/// # Errors
/// Rejects NaN, infinity, and negative or non-finite tolerances. Overflowing
/// subtraction of finite, opposite-sign values correctly yields a mismatch.
pub fn compare_numbers(left: f64, right: f64, tolerance: f64) -> Result<Verdict, NumericError> {
    if !left.is_finite() || !right.is_finite() || !tolerance.is_finite() || tolerance < 0.0 {
        return Err(NumericError);
    }
    Ok(if (left - right).abs() <= tolerance {
        Verdict::Pass
    } else {
        Verdict::Fail
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gates_never_pass_without_required_evidence() {
        assert_eq!(aggregate([]), Verdict::Error);
        for v in [
            Verdict::Inconclusive,
            Verdict::Unsupported,
            Verdict::Skipped,
            Verdict::Error,
            Verdict::Fail,
        ] {
            assert_ne!(
                aggregate([Check {
                    required: true,
                    verdict: v
                }]),
                Verdict::Pass
            );
        }
        assert_eq!(
            aggregate([Check {
                required: false,
                verdict: Verdict::Pass
            }]),
            Verdict::Error
        );
    }
    #[test]
    fn precedence_is_order_independent() {
        let all = [
            Verdict::Pass,
            Verdict::Fail,
            Verdict::Error,
            Verdict::Inconclusive,
            Verdict::Unsupported,
            Verdict::Skipped,
        ];
        for a in all {
            for b in all {
                let check = |verdict| Check {
                    required: true,
                    verdict,
                };
                let actual = aggregate([check(a), check(b)]);
                assert_eq!(actual, aggregate([check(b), check(a)]));
                if a == Verdict::Fail || b == Verdict::Fail {
                    assert_eq!(actual, Verdict::Fail);
                } else if a == Verdict::Error || b == Verdict::Error {
                    assert_eq!(actual, Verdict::Error);
                } else if a == Verdict::Pass && b == Verdict::Pass {
                    assert_eq!(actual, Verdict::Pass);
                } else {
                    assert_ne!(actual, Verdict::Pass);
                }
            }
        }
        assert_eq!(
            aggregate([
                Check {
                    required: true,
                    verdict: Verdict::Pass
                },
                Check {
                    required: false,
                    verdict: Verdict::Fail
                }
            ]),
            Verdict::Pass
        );
    }
    #[test]
    fn bytes_and_numeric_boundaries() {
        assert_eq!(first_difference(b"abc", b"ab"), Some(2));
        assert_eq!(first_difference(b"abc", b"axc"), Some(1));
        assert_eq!(compare_bytes(b"", b""), Verdict::Pass);
        assert_eq!(compare_numbers(1.0, 1.5, 0.5), Ok(Verdict::Pass));
        assert_eq!(
            compare_numbers(f64::MAX, -f64::MAX, f64::MAX),
            Ok(Verdict::Fail)
        );
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(compare_numbers(bad, 1.0, 0.0).is_err());
            assert!(compare_numbers(1.0, 1.0, bad).is_err());
        }
        assert!(compare_numbers(0.0, 0.0, -1.0).is_err());
    }
}
