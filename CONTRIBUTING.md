# Contributing

Use stable Rust with rustfmt and Clippy. Development currently uses native Windows
MSVC; Linux CI runs the same checks. The MSVC toolchain requires Microsoft's linker.
The comparison libraries contain safe Rust and have no third-party dependencies.

Run formatting, Clippy with `-D warnings`, workspace tests, and rustdoc before a change.
For public API changes also run the independent consumers:

```
cargo run --manifest-path examples/core-consumer/Cargo.toml --release
cargo run --manifest-path examples/frame-consumer/Cargo.toml --release
```

Add regression tests for incorrect verdicts, not just successful comparisons. Missing,
partial, or unsupported evidence must not produce a required pass. Keep wire parsing
in the CLI and image decoding outside the borrowed-frame library. Document behavior
changes alongside tests. New dependencies need a concrete reason and a graph review.

The workspace lockfile is committed for reproducible CLI builds. Tests need no network
after dependencies are fetched. Publication requires a separate license/advisory review,
platform validation, package dry run and maintainer approval.
