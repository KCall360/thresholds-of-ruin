# Measurement checkpoints — October 30, 2026

These are recovery archives, not a production change or a CI qualification.

- `retention-measurements-v2.zip`: 261,263 bytes; SHA-256 `f5732fa78c9a9dd1c76cb81f388078dc9b806dde01a8a24a7ca54a9e27399616`. Three debug workloads and two existing history tests passed on base a3853eaa391730ecbd06bdb03dd40c3994a1927f. Repeated unchanged observations share one root; staggered observations retain additional roots; disconnect releases those extra roots. These are root counts, not heap savings. No retention cap changed. The archive preserves its earlier plan alongside the final report; read report.md for results.
- `prepared-discovery-growth.zip`: 23,050 bytes; SHA-256 `411cd9776557f20b92893b8a7ed632e6b90e9f8dfb952e9b95d2a9fb92294bde`. Prepared test-only instrumentation, source, patch, manifest and plan on base a3853eaa. This checkpoint is uncompiled and has no runtime result. Later local work is not represented by this immutable archive.

Restore onto the archive's stated base, not the older source tree of this backup branch. No PR was opened for this backup. Existing backup contents are preserved.
