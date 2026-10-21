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
