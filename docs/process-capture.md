# Executable capture

Build the CLI with `cargo build -p reconform-cli --features png`. The PNG feature
is optional; executable capture and recorded comparisons work without it.
Targets need no Reconform dependency or instrumentation.

`reconform plan capture.json --format json` validates every step and shows
executable hashes, working directories, environment keys and budgets without
launching a target. `reconform capture capture.json --allow-exec ROOT --output
bundle.json --format json` explicitly authorizes execution inside an existing
canonical root. Both executable and working directory must be inside that root.
Paths in the manifest are absolute; arguments are literal array elements, never
shell command text. Windows accepts native `.exe` files.

```json
{
  "schema_version": "1.0",
  "scenario": "render",
  "subject": "candidate",
  "steps": [{
    "checkpoint": "after-render",
    "executable": "C:/work/demo/target/release/demo.exe",
    "args": ["--screenshot", "frame.png"],
    "cwd": "C:/work/demo/run",
    "env": {},
    "stdin": "",
    "timeout_ms": 5000,
    "max_output_bytes": 65536,
    "files": [{"channel": "frame", "path": "frame.png", "max_bytes": 1048576}]
  }]
}
```

Steps run sequentially, each as a new process. Each checkpoint records stdout
and stderr as exact byte arrays and an exit observation. Nonzero target exit is
captured behavior, not a verifier error; use an `exit_success` contract to require
success. A completed capture is not a verification pass: run `compare` or `check`
against its bundle. PNG file bytes can use the `png` comparison policy.

Environment inheritance is disabled. Supply every required entry explicitly,
including `PATH` or `SystemRoot` if the target needs it. The environment digest
identifies supplied entries only; it does not attest to the OS, fonts, hardware,
clock or network. Arguments, stdin and environment values are not echoed in
capture diagnostics, but output bytes are intentionally retained as evidence.
Do not put secrets in target output intended for shared reports.

Limits: 1–32 steps, 1–300,000 ms per step, 1–262,144 bytes per output stream,
262,144 stdin bytes, 16 files per step and 1 MiB per captured file. The combined
worst-case serialized output budget must fit below 7 MiB, leaving space for
metadata in the 8 MiB recorded format. Files must be fresh relative paths inside
the working directory, with existing parent directories. Existing files,
traversal and paths resolving outside the working directory are rejected.
No file is deleted to make a capture fresh. Use a fresh run directory to rerun.

Timeout, cancellation, output overflow or file-capture failure saves partial or
unavailable observations and returns 4. Missing executables fail preflight.
Windows Job Objects and Unix process groups terminate ordinary descendants.
SIGINT/SIGTERM on Unix and Ctrl-C on Windows request cleanup. Forced termination
of the verifier cannot guarantee cleanup on Unix. Processes that deliberately
escape Unix process groups, replace files concurrently, access the network or
modify unrelated files require OS isolation outside Reconform. This is a test
runner, not a sandbox. Executable hashes describe the preflight file; they are
not protection against concurrent binary replacement.

The original manifest and executable files are required for another capture.
Recorded bundles can be compared repeatedly without executing anything.
