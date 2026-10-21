# Security

This unreleased implementation reads bounded local JSON files and compares recorded
observations. It does not launch targets, decode images, load plugins or upload data.
It is not a sandbox or proof of application equivalence. Do not treat recorded input
as authenticated evidence.

Reports omit observation payloads, but include file paths and declared provenance and
environment metadata. Keep secrets out of those fields. Report output inherits the
selected directory's filesystem permissions. Use a private directory for sensitive work.

For a vulnerability, use GitHub private vulnerability reporting if enabled. Do not post
credentials, proprietary recordings or exploit payloads in public issues. There are no
published supported versions yet; dependency advisories must be reviewed before release.

Unix reports use mode 0600. Unix input reads reject special files and use nonblocking
open flags to prevent FIFO waits. Input/output directories should be controlled by
the invoking user; concurrent hostile directory mutation is outside this tool's
isolation guarantees.
