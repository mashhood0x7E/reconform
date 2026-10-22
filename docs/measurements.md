# Local measurements

Measured on 21 October 2026, native Windows x86-64 MSVC, rustc 1.90.0.
These are local observations, not performance guarantees or release qualification.
CPU model and installed memory were unavailable because the host denied CIM access.

| Independent release consumer | Clean Cargo build | Executable bytes | Third-party normal/build dependencies |
| --- | ---: | ---: | ---: |
| core-consumer | 1.57 s | 120832 | 0 |
| frame-consumer | 2.21 s | 179712 | 0 |

Both builds used their own empty target directories and independent Cargo workspaces,
with default features. They ran concurrently, so build times include contention.
Binary sizes include the example and Rust runtime, not just the library.

The frame example compared 1920x1080 borrowed RGBA8 inputs with a changed 128x128
region, exact policy, 50 repetitions: p50 16.759 ms, p95 24.568 ms. Input allocation
and decoding were excluded; no decoder is used. The source is
`examples/frame-consumer/src/main.rs`. Run it with `--release`; timing varies by host.
This is a smoke measurement without warmup or controlled machine load. Allocation
counts and peak memory were not measured. No NFR throughput qualification is claimed.

The original offline CLI used serde, serde_json and BLAKE3.
Serde/serde_json supply the JSON parser and typed wire format; std has no JSON codec.
BLAKE3 supplies reproducible cryptographic input identities; std hashes are unsuitable.
BLAKE3's pure Rust feature disables native assembly use, although its build dependency
still includes cc. Neither library depends on any of these packages. The lockfile
records exact versions. License/advisory review remains a release prerequisite.

Reproduce dependency checks with `cargo tree --manifest-path
examples/core-consumer/Cargo.toml -e normal,build` and the corresponding frame consumer.
CLI release build size, event benchmarks, allocations, Linux/macOS results and a
full supply-chain audit have not yet been measured.

Release review on 21 October 2026: all 20 registry package versions in Cargo.lock
were queried through OSV's public batch API; no advisory matches were returned.
This is a point-in-time database check, not a source audit or proof of safety.
Their declared license expressions offer MIT, Apache-2.0, or equivalent permissive
options; unicode-ident additionally requires Unicode-3.0 notices. Distribution must
preserve applicable third-party notices. No automatic package publication occurred.

The CLI additionally uses libc on Unix for O_NONBLOCK/O_NOCTTY, avoiding blocking
opens on special files. libc was already present in the cross-platform lockfile.
Core and RenderParity remain dependency-free. The supported minimum toolchain is
1.90, verified by the Windows/Linux GitHub Actions matrix.

A separate CSV ledger pilot exercised 18 comparisons, 9 contract runs and 18 identical
reruns. Results and limitations are in [pilot.md](pilot.md). Its test project remains
outside this repository.

On 22 October 2026 the CLI added process-wrap for Windows Job Objects and Unix
process groups, ctrlc for cancellation, platform Windows bindings for hidden
process creation, and optional png for decoding/encoding. These avoid maintaining
custom OS lifecycle and image codec implementations. They are confined to the
CLI; both independent library dependency graphs remain unchanged. PNG can be
omitted at build time. The process dependencies can be removed with the capture
module without changing either library's API.

The all-feature, all-platform resolved metadata contains 54 registry packages.
An OSV batch query for those exact versions returned zero advisory matches on
22 October 2026. Declared licenses are permissive MIT/Apache alternatives, Zlib,
and Unicode-3.0 obligations; this remains a database/license-metadata review,
not a source security audit. Cargo.lock records the complete versions.

The external ledger pilot was rerun through native capture: 27 captures,
18 comparisons (four deliberate failures), and 18 byte-identical report reruns.
All matched the earlier independent capture harness results. Process integration
tests additionally exercise literal arguments, deadlines, output flooding,
nonzero exit, fresh-file checks, traversal rejection and descendant termination.

A later registry query identified the local processor as Intel Core i5-8300H
2.30 GHz. Repeating the independent 1080p release consumer on 22 October gave
p50 16.412 ms and p95 24.756 ms over 50 samples. Both independent consumer graphs
still contain zero third-party normal/build dependencies. This remains a local
smoke measurement, with no claim of controlled system load or measured peak RSS.
