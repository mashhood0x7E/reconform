# Reconform

Compare recorded observations from a reference and candidate implementation.
Reconform reports which checks differ, which could not be verified, and how to
repeat the comparison. Neither application needs to depend on Reconform.

This is an unreleased, experimental implementation. The CLI supports offline
text, bytes, finite numbers, ordered string events, and exit-code comparisons.
`renderparity` separately compares borrowed RGBA8 frames. Process execution,
PNG decoding, contract-only scenarios, and baseline management are not available.

## Try it

Requires Rust/Cargo (tested with Rust 1.90 on Windows MSVC).

```powershell
cargo run -p reconform-cli -- compare examples/recorded/reference.json examples/recorded/candidate.json --plan examples/recorded/plan.json --format json
```

The example intentionally differs at byte 0 and exits with status **1**. Compare
`reference.json` with itself to see a passing report. To install the local CLI:

```powershell
cargo install --path crates/reconform-cli --locked
```

The JSON report includes each check's result, the first comparable mismatch,
input digests, the applied policy, provenance, and a rerun argument array.
`--output report.json` also saves the canonical JSON, refusing to replace an
existing file. Missing or partial observations never satisfy a required check.

See [recorded format and exit codes](docs/recorded-format.md) for preparing inputs.
The [example](examples/recorded/README.md) includes two standalone Rust programs.

## Libraries

| Package | Use |
| --- | --- |
| [reconform-core](crates/reconform-core) | Byte/numeric comparisons and explicit verdict aggregation |
| [renderparity](crates/renderparity) | Borrowed RGBA8 comparison, tolerance, rectangular regions and mismatch bounds |
| [reconform-cli](crates/reconform-cli) | JSON input, bounded file reads and offline reports |

Both libraries have zero third-party dependencies and perform no I/O. JSON parsing
and BLAKE3 hashing belong to the CLI. Independent consumer examples live under
`examples/core-consumer` and `examples/frame-consumer`.

## Development

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Windows is locally tested; Linux checks are configured in CI but are not yet
verified. See [development notes](CONTRIBUTING.md) and [measurements](docs/measurements.md).

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
