# Compatibility and release scope

The initial 0.1.0 release candidate supports offline recorded-evidence comparison,
candidate-only recorded contracts, and standalone in-memory RGBA8 comparison.
It does not execute applications or decode images. This scope is deliberately smaller
than the longer-term Reconform product architecture.

## Rust APIs

Minimum Rust version: 1.90. Edition: 2024. Windows MSVC and Linux are tested in CI.
Both comparison libraries use safe Rust and have no third-party dependencies under
default features. There are no optional features in this release.

Patch releases in the 0.1 series should preserve public signatures, verdict semantics,
accepted valid inputs and default policy behavior. A breaking Rust API change requires
a 0.2 release and migration notes. Expanding enum variants is treated as a breaking
change for exhaustive downstream matches. Security fixes may reject previously
accepted malformed inputs, with a changelog entry and regression test.

## Wire formats

Bundles, comparison plans, contract plans and reports use schema 1.0. Unknown versions
or unknown input fields are rejected. A report consumer should check both schema_version
and method; the supported methods are recorded-parity/1 and recorded-contracts/1.
Comparison algorithms and normalization defaults cannot silently change in a patch.

CLI exit codes 0..4 are documented in recorded-format.md. Human wording may improve;
JSON keys, diagnostic codes and documented verdict meaning are the integration contract.
Rerun recipes preserve argument boundaries and include content digests for verification.

## Not part of 0.1

Process capture and lifecycle control, persistent content-addressed artifact storage,
PNG/diff-image CLI integration, semantic JSON observations, baseline promotion,
frame timelines, game adapters, MCP and reduction remain future work. No command or
feature flag pretends to implement them. macOS and other targets are not qualified.

## Publication

The package set is reconform-core, renderparity and reconform-cli, all at 0.1.0.
Publish the libraries before the CLI and allow registry index propagation. The owner
must approve actual publication. Local package verification and publish --dry-run
never substitute for that approval or for checking the eventual hosted docs.rs builds.

Qualification evidence is in measurements.md and pilot.md; CI runs formatting,
Clippy, unit/integration/doc tests, independent consumers, schema checks and packaging.
The controlled pilot is not an external usability study or a guarantee of bug absence.
