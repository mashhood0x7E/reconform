# Recorded observations, schema 1.0

The CLI reads UTF-8 JSON only. Inputs are limited to 8 MiB per file, 1024 checkpoints,
1024 observations and 1024 checks. JSON recursion is bounded by serde_json's default
parser limit. Unknown fields, unknown payload/policy tags, duplicate struct fields,
unsupported schema versions, duplicate observation keys and invalid IDs are errors.
Collection counts are limited during deserialization; semantic validation then checks the bounded values. No file named by a payload
is opened, and no command is executed.

## Bundle

```json
{
  "schema_version": "1.0",
  "scenario": "greeting",
  "subject": "reference",
  "provenance": "manual recording from build abc123",
  "environment": "windows-fixture-v1",
  "checkpoints": ["done"],
  "observations": [{
    "checkpoint": "done", "channel": "stdout", "occurrence": 0,
    "completeness": "complete",
    "payload": {"kind": "text", "value": "hello\n"}
  }]
}
```

IDs contain 1..128 ASCII letters, digits, dots, underscores or hyphens. They are not
filesystem paths. Observation keys `(checkpoint, channel, occurrence)` are unique;
checkpoints must be declared. Occurrences are unsigned 32-bit integers. Both bundles
must declare the same scenario and ordered checkpoint list as used by the plan.
The subject name is descriptive; using the same bundle twice is permitted.

Provenance is a nonempty string of at most 4096 UTF-8 bytes. Environment is null,
omitted (unknown), or a nonempty string of at most 4096 bytes. These metadata fields
are echoed verbatim: do not put secrets in them. Environment equality compares the
declared strings; it does not probe or authenticate the actual host.

Completeness is `complete`, `partial`, `unknown`, or `unavailable`. Only complete
observations can pass. A missing observation is different from an empty text/event
value. Bundle contents are claims supplied by the caller, not signed capture evidence.

| Payload kind | Value | Exact semantics |
| --- | --- | --- |
| text | UTF-8 string | Identical bytes, no newline/case/Unicode normalization |
| bytes | Array of integers 0..255 | Identical bytes |
| scalar | Finite JSON number | Parsed as IEEE-754 f64; exact equality of that value |
| events | Array of up to 10,000 strings | Identical ordered strings, including duplicates |
| exit | Signed 32-bit integer | Equal recorded exit codes; equal nonzero codes can pass parity |

Scalars intentionally use f64: use text/bytes for lossless arbitrary-size integers.
JSON objects are not a supported observation family. Exit parity is not an assertion
that either process succeeded. The caller is responsible for honest completeness.

## Comparison plan

```json
{
  "schema_version": "1.0", "scenario": "greeting",
  "strict_environment": false,
  "checks": [{
    "id": "greeting-output", "checkpoint": "done", "channel": "stdout",
    "occurrence": 0, "required": true, "policy": {"kind": "exact"}
  }]
}
```

At least one check must be required. Check IDs are unique. Policy `exact` supports
all listed payload kinds. `{"kind":"absolute","tolerance":0.5}` supports scalar
values only and means `abs(left - right) <= tolerance`; tolerance must be finite and
nonnegative. Unsupported kind/policy combinations produce `unsupported` findings.
Different payload kinds yield `inconclusive`, without coercion.

Strict environment mode makes checks inconclusive when either environment is unknown
or their values differ. Otherwise this condition appears in report limitations.
Unselected observations are counted; no implicit check is added or removed.

## Report and rerun

`--format json` emits one JSON document. Reports carry schema/tool/method versions,
scenario, aggregate verdict, normalized plan, input BLAKE3 digests and absolute paths,
subject metadata, findings, first divergence, coverage omissions and limitations.
Payload contents are not echoed. Locations are zero-based byte offsets (text/bytes)
or event indices. A shorter sequence differs at its first missing index.

Findings sort by declared checkpoint order, channel, occurrence, then check ID.
The first divergence is the first comparable mismatch, including optional checks;
it is not a root-cause claim. Required Fail > Error > Inconclusive/Unsupported > Pass.
Optional failures do not change the gate. The core API also supports Skipped: all
required skipped gives Skipped; mixed pass/skipped gives Inconclusive. The CLI does
not currently produce skipped checks.

Reports are deterministic for identical file bytes, paths, policies and tool version.
`policy_digest` hashes serde JSON serialization of the typed plan, including its check
array order. Input digests hash exact file bytes, including whitespace. `reproduce`
is an argument array, not a shell command: preserve argument boundaries. Retain the
input files and verify their recorded digests before rerunning; the CLI does not pin
or authenticate externally changed files automatically.

`--output FILE` writes canonical JSON whether the console format is human or JSON.
A same-directory temporary file is synced and hard-linked into place. Existing
files are never replaced. Filesystems without hard-link support return an I/O error.
The CLI bounds reports to 2 MiB. A crash may leave a `.reconform-*.tmp` file.

| Exit | Meaning |
| --- | --- |
| 0 | Required comparisons passed, or a non-comparison command succeeded |
| 1 | Required comparison mismatch |
| 2 | Invalid CLI arguments, schema, policy or input limits |
| 3 | File read/write or verifier error |
| 4 | Required verification inconclusive or unsupported |

JSON-mode errors have `schema_version`, `verdict: "error"`, stable `code`
(`CONFIG_INVALID` or `IO_ERROR`) and a message. Parser errors omit payload excerpts.
Input/output paths and caller-provided metadata may still contain sensitive data.

Schema 1.0 is experimental. Unknown versions are rejected, including minor versions;
future compatibility changes require explicit fixtures and documentation.

The JSON Schemas in `docs/schemas` describe structural constraints. CLI validation
also checks UTF-8 byte limits (JSON Schema lengths count characters), unique keyed
observations/check IDs, finite f64 values and checkpoint references. Schema validation
alone does not establish semantic validity. Golden tests freeze the report's verdict,
first-divergence and finding projection; separate tests check complete report
repeatability and input digests.

## Candidate-only contracts

A contract checks one bundle without needing a reference:

```powershell
reconform check crates/reconform-cli/examples/recorded/candidate.json --plan crates/reconform-cli/examples/recorded/contracts.json --format json
reconform validate crates/reconform-cli/examples/recorded/contracts.json --kind contracts
```

Contract plans have `schema_version`, `scenario`, and `checks`. Each check has the
same ID, checkpoint, channel, occurrence and required fields as a comparison, but
uses `expression` instead of `policy`. There is no strict-environment setting because
no cross-environment pairing is performed.

| Expression | Supported evidence | Result |
| --- | --- | --- |
| `{"kind":"exists"}` | Any complete observation | Pass if present; Fail if missing |
| `{"kind":"non_empty"}` | Text, bytes, events | Pass if at least one byte/event |
| `{"kind":"between","min":0,"max":10}` | Finite scalar | Inclusive bounds; invalid/reversed ranges rejected |
| `{"kind":"exit_success"}` | Exit code | Pass only for zero |

Incomplete evidence is always inconclusive, including Exists. Missing evidence for
other expressions is inconclusive. Unsupported value/expression combinations remain
unsupported. Exists describes the recording, not whether an external file or object
exists in the actual application. At least one check must be required.

Contract reports use `method: "recorded-contracts/1"`, one subject, and two input
identities (bundle, plan). `first_divergence` identifies the first violated contract;
locations are null. Sorting, limits, gate precedence, output and rerun behavior match
offline comparison. Caller-declared metadata remains unverified.

On Unix, input files are opened nonblocking and checked to be regular files before
reading, so a FIFO cannot wait for an external writer. The CLI uses libc only for
Unix open flags; this adds no dependency to either comparison library. Report files
are created with mode 0600 on Unix. Windows files inherit the chosen directory's ACL.
The caller must control the input/output directories; this is not an adversarial
shared-directory sandbox or a snapshot of files concurrently modified by another user.

Report/error structure is described in `docs/schemas/report.schema.json`. The
embedded plan is validated separately with the comparison or contract schema;
`./scripts/check-schemas.ps1` checks emitted reports and representative inputs using
PowerShell's independent JSON Schema validator. Numeric f64 range limits remain CLI
semantic validation to avoid imposing enormous decimal literals on schema tooling.
