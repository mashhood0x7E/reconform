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

The CLI has three direct third-party dependencies: serde, serde_json and BLAKE3.
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
