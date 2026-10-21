use crate::Verdict;

/// A borrowed value exposed by a recording or application.
#[derive(Clone, Copy, Debug)]
pub enum ObservationView<'a> {
    /// Text or arbitrary bytes; emptiness is measured in bytes.
    Bytes(&'a [u8]),
    /// A finite scalar value.
    Scalar(f64),
    /// A recorded process exit code.
    Exit(i32),
    /// Ordered named events; emptiness is measured in events.
    Events(&'a [String]),
}

/// Evidence availability. An incomplete value cannot satisfy a contract.
#[derive(Clone, Copy, Debug)]
pub enum Evidence<'a> {
    /// The recording contains no matching observation.
    Missing,
    /// Capture was partial, unknown, or unavailable.
    Incomplete,
    /// A complete observation supplied by the caller.
    Complete(ObservationView<'a>),
}

/// Candidate-only assertions that require no reference implementation.
#[derive(Clone, Copy, Debug)]
pub enum Contract {
    /// The recording contains a complete observation, even if its value is empty.
    Exists,
    /// The byte string or event sequence has at least one element.
    NonEmpty,
    /// A finite scalar lies within inclusive, finite ordered bounds.
    Between {
        /// Inclusive lower bound.
        min: f64,
        /// Inclusive upper bound.
        max: f64,
    },
    /// A recorded process exit code equals zero.
    ExitSuccess,
}

/// Evaluate a contract without I/O or allocations.
///
/// Missing evidence fails Exists and makes other contracts Inconclusive.
/// Incomplete evidence always yields Inconclusive. A non-finite value or invalid
/// range yields Error, and an incompatible value kind yields Unsupported.
/// Exists checks the recording, not the external world's state.
///
/// ```
/// use reconform_core::{Contract, Evidence, ObservationView, Verdict, evaluate_contract};
/// let value = Evidence::Complete(ObservationView::Scalar(4.0));
/// assert_eq!(evaluate_contract(Contract::Between { min: 0.0, max: 4.0 }, value), Verdict::Pass);
/// assert_eq!(evaluate_contract(Contract::Exists, Evidence::Missing), Verdict::Fail);
/// ```
#[must_use]
pub fn evaluate_contract(contract: Contract, evidence: Evidence<'_>) -> Verdict {
    if let Contract::Between { min, max } = contract
        && (!min.is_finite() || !max.is_finite() || min > max)
    {
        return Verdict::Error;
    }
    let value = match evidence {
        Evidence::Missing => {
            return if matches!(contract, Contract::Exists) {
                Verdict::Fail
            } else {
                Verdict::Inconclusive
            };
        }
        Evidence::Incomplete => return Verdict::Inconclusive,
        Evidence::Complete(value) => value,
    };
    if let ObservationView::Scalar(value) = value
        && !value.is_finite()
    {
        return Verdict::Error;
    }
    let passed = match (contract, value) {
        (Contract::Exists, _) => true,
        (Contract::NonEmpty, ObservationView::Bytes(v)) => !v.is_empty(),
        (Contract::NonEmpty, ObservationView::Events(v)) => !v.is_empty(),
        (Contract::Between { min, max }, ObservationView::Scalar(v)) => v >= min && v <= max,
        (Contract::ExitSuccess, ObservationView::Exit(code)) => code == 0,
        _ => return Verdict::Unsupported,
    };
    if passed { Verdict::Pass } else { Verdict::Fail }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_and_incomplete_are_distinct() {
        for contract in [
            Contract::Exists,
            Contract::NonEmpty,
            Contract::Between { min: 0.0, max: 1.0 },
            Contract::ExitSuccess,
        ] {
            assert_eq!(
                evaluate_contract(contract, Evidence::Incomplete),
                Verdict::Inconclusive
            );
            assert_ne!(
                evaluate_contract(contract, Evidence::Missing),
                Verdict::Pass
            );
        }
        assert_eq!(
            evaluate_contract(Contract::Exists, Evidence::Missing),
            Verdict::Fail
        );
        assert_eq!(
            evaluate_contract(
                Contract::Exists,
                Evidence::Complete(ObservationView::Bytes(b""))
            ),
            Verdict::Pass
        );
        assert_eq!(
            evaluate_contract(
                Contract::NonEmpty,
                Evidence::Complete(ObservationView::Bytes(b""))
            ),
            Verdict::Fail
        );
    }
    #[test]
    fn ranges_and_exit_codes() {
        let contract = Contract::Between {
            min: -1.0,
            max: 1.0,
        };
        for n in [-1.0, 0.0, 1.0] {
            assert_eq!(
                evaluate_contract(contract, Evidence::Complete(ObservationView::Scalar(n))),
                Verdict::Pass
            );
        }
        assert_eq!(
            evaluate_contract(contract, Evidence::Complete(ObservationView::Scalar(2.0))),
            Verdict::Fail
        );
        assert_eq!(
            evaluate_contract(
                contract,
                Evidence::Complete(ObservationView::Scalar(f64::NAN))
            ),
            Verdict::Error
        );
        assert_eq!(
            evaluate_contract(Contract::Between { min: 2.0, max: 1.0 }, Evidence::Missing),
            Verdict::Error
        );
        for (code, expected) in [(0, Verdict::Pass), (1, Verdict::Fail), (-1, Verdict::Fail)] {
            assert_eq!(
                evaluate_contract(
                    Contract::ExitSuccess,
                    Evidence::Complete(ObservationView::Exit(code))
                ),
                expected
            );
        }
        assert_eq!(
            evaluate_contract(
                Contract::ExitSuccess,
                Evidence::Complete(ObservationView::Scalar(0.0))
            ),
            Verdict::Unsupported
        );
    }
}
