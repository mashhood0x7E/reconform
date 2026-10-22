# reconform-cli

Executable capture and verification of recorded text, byte, scalar, event and exit observations.
The installed binary is named `reconform`.

```
reconform compare reference.json candidate.json --plan plan.json --format json
reconform validate reference.json --kind bundle
reconform validate plan.json --kind plan
```

This package is unreleased. `capture` executes explicitly authorized native targets;
`plan` validates a capture manifest without execution. Add `--features png` when
installing to enable `compare-png` and PNG policies on captured byte observations.
See the [capture guide](https://github.com/mashhood0x7E/reconform/blob/main/docs/process-capture.md)
and [PNG guide](https://github.com/mashhood0x7E/reconform/blob/main/docs/png-comparison.md).
Input schema, examples and exit codes are documented in the repository's
[recorded-format guide](https://github.com/mashhood0x7E/reconform/blob/main/docs/recorded-format.md).
Libraries remain independent of the CLI's serde/serde_json/BLAKE3 dependencies.
Licensed under MIT OR Apache-2.0.

Candidate-only contracts are available with `reconform check candidate.json --plan
contracts.json`. Supported expressions are Exists, NonEmpty, Between and ExitSuccess.
Incomplete captures cannot satisfy these contracts.

Contracts also support typed equality, JSON fields and event occurrence/order.
`run` preflights and executes complete capture/verification scenarios. `report`
renders saved reports, and `--format junit` provides a CI-oriented summary.
`init` creates a runnable recorded example; `doctor` reports compiled capabilities.
