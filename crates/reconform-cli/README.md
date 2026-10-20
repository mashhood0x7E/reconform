# reconform-cli

Offline verification of recorded text, byte, scalar, event and exit observations.
The installed binary is named `reconform`.

```
reconform compare reference.json candidate.json --plan plan.json --format json
reconform validate reference.json --kind bundle
reconform validate plan.json --kind plan
```

This package is experimental and unreleased. It does not execute targets.
Input schema, examples and exit codes are documented in the repository's
[recorded-format guide](https://github.com/mashhood0x7E/reconform/blob/main/docs/recorded-format.md).
Libraries remain independent of the CLI's serde/serde_json/BLAKE3 dependencies.
Licensed under MIT OR Apache-2.0.
