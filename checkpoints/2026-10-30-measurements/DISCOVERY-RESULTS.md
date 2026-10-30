# Discovery measurement checkpoints

These immutable archives extend the earlier prepared checkpoint; they do not replace or relabel its evidence.

- `completed-discovery-growth.zip`: 220,350 bytes; SHA-256 `ce175cb9eb280d66258433fc336dfaeccd0b2d073776cb2a266cfeb3980d9e73`. Completed debug counters and 54 discovery tests, including explicitly enabled 1,024-fact diagnostic workload. Read result-and-design.md. No production restructuring.
- `prepared-timing-attribution.zip`: 20,001 bytes; SHA-256 `9003fb4186652305d8fdabb2e3ed3349610ff9ba3c8c2b6fd169d69f0d5c6301`. Prepared, uncompiled optimized timing source at its checkpoint, not later test results.
- `completed-optimized-discovery-v2.zip`: 434,154 bytes; SHA-256 `60b889d3e0db32d90c1ebb152c289dcae2dfd6f2b625fa1329d6cfe09cae7d17`. Final optimized baseline attribution, seven samples per mode plus warmup, and all 54 discovery tests. Exact optimized source and binary identities are in the archive. Use this v2, not the superseded local v1.

The vector is retained. Copy counts grow, but these measurements do not isolate copying from sight, scanning and other discovery work, and do not justify adding a chunked structure. Timings describe a configured single-actor fixture, not end-to-end gameplay or a candidate speedup. No cap, rules, wire or save-format change is made. Source bases and validation limits are recorded separately inside each archive. No PR is opened for this backup.
