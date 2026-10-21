# Recorded comparison example

`reference.json` and `candidate.json` contain a deliberate capitalization mismatch.
`plan.json` checks stdout at the named checkpoint `done`.

The accompanying Rust programs emit those recordings without any Reconform dependency:

```powershell
rustc crates/reconform-cli/examples/recorded/reference.rs -o target/reference.exe
rustc crates/reconform-cli/examples/recorded/candidate.rs -o target/candidate.exe
```

Their stdout is a recording bundle, not the raw application output. This example
demonstrates the import boundary; the CLI does not capture programs automatically.
The integration test compiles both programs, saves their emitted bundles and checks
that the CLI reports a mismatch. No application instrumentation framework is required.
