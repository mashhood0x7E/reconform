# Controlled ledger pilot

Run on 21 October 2026, Windows MSVC, Rust 1.90. The separate sibling project
`reconform-pilot` has no Reconform dependencies and is not part of this repository.
It is generated test software, not third-party adoption evidence.

The reference ledger uses a BTreeMap; the corrected candidate independently
aggregates a vector then sorts. Inputs are account/integer-cent CSV records. Output
is sorted account totals; invalid rows and checked integer overflow exit with code 2.
A deliberately faulty candidate ignores debits, drops unterminated final rows and
retains input order.

| Case | Faulty candidate parity | Corrected candidate parity |
| --- | --- | --- |
| Basic positive transactions | Pass | Pass |
| Debit transactions | Fail at stdout | Pass |
| Last row without newline | Fail at stdout | Pass |
| Unsorted account order | Fail at stdout | Pass |
| Invalid integer | Pass (same rejection) | Pass |
| Integer overflow | Pass (same rejection) | Pass |
| CRLF input | Pass | Pass |
| Empty input | Pass | Pass |
| 10,000 transactions | Fail at stdout | Pass |

All 18 comparisons produced expected exits. Each was repeated through its report's
argument array, with identical JSON and exit code. Nine candidate-only contract runs
also matched expectations: successful exit and nonempty output for normal cases,
explicitly optional nonempty output for empty input, and failed ExitSuccess for the
two deliberate rejection cases. All 27 reports passed structural schema validation.

Executable SHA-256 identities are recorded as capture provenance; Reconform separately
hashes complete recording inputs with BLAKE3. Captures include stdout, stderr and exit.
The pilot's `run.ps1` is capture test infrastructure, not a shipped execution adapter.
No sandbox, independent usability study, or external maintainer approval is implied.

## Native capture and visual scenario follow-up

On 22 October the ledger cases were rerun using Reconform's `capture` command
instead of the pilot's .NET capture helper. All 27 native captures completed;
the 18 comparisons and 18 report reruns matched the earlier outcomes.

A second independent project, `reconform-pilot/visual`, renders a 32×16 image and
prints JSON state at `after-movement`. It depends on the PNG codec, with no
Reconform dependency. The CLI `run` command captures stdout, exit status and a
fresh screenshot for each subject, then evaluates JSON and PNG policies together.

| Candidate | State check | Image check | Overall |
| --- | --- | --- | --- |
| Corrected implementation | Pass | Pass | Pass |
| Position shifted by one pixel | Fail | Fail | Fail |
| Correct position, wrong square color | Pass | Fail | Fail |

All three reports passed the published schema and reproduced byte-for-byte from
their retained bundles. The color-only defect measured 16 mismatched pixels of
512 (0.03125), maximum channel error 255, mean channel error 3.984375 and bounds
`[6,4,10,8]`. Standalone PNG comparison emitted a 125-byte diff PNG with its
BLAKE3 identity; its report also passed independent schema validation. This
separates a behavioral defect from a purely graphical change using actual
executable output, while retaining the limitations of a controlled synthetic pilot.
