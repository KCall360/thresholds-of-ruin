# Service contract pilot

This opt-in pilot moves catalog scenario authoring and assertions into Python
while exercising the production Rust `Service`, package constructor and outbound
encoder. It does not replace the [testing policy](testing.md), existing tests,
real WebSocket witnesses or actual Text/native acceptance. Lifecycle and
persistence expansion require review after the catalog experiment.

## Verifier and CI ownership

The existing Rust CI partitions run this suite on Windows/Linux in debug and
release, without adding jobs. `scripts/service_contract_ci.py` runs the selected
`cargo test` command with the host feature and Cargo JSON messages. Integration
test selection also builds the feature-required, `test = false` binary as an
ordinary executable. The wrapper requires exactly one matching normal-bin
artifact with the expected source path and feature. It uses that executable and
its hash only after the Rust suite succeeds; missing, duplicate or invalid
artifacts fail without a guessed path or fallback build.
The host suite must select at least one case. Selected/run counts, failures,
errors and skips are reported separately; empty, skipped or incomplete execution
cannot produce a successful gate. The count grows with the suite and is not
hardcoded to the pilot inventory.

Local `quick` and `push` use the same wrapper when `tor-server` is in their
selected package set. Host adapter, scenario and wrapper Python changes select
that owner. Unrelated quick checks are not broadened. Clippy/documentation use
the same host feature when selected. The Python CI partitions continue building
and exercising the production applications with their default feature set.
No existing Rust, process, native or isolated-simulation suite is removed.

For a standalone selected-package run:

```sh
python scripts/service_contract_ci.py --profile debug --package tor-server
```

The suite has an explicit filename because it runs against the artifact from
its owning Rust stage, rather than unconfigured generic Python discovery.
Pure adapter/wrapper tests join normal tooling discovery. Unrelated reduced
checks do not imply the production Service suite ran.

## Iterate against a qualified host

Build the dedicated host once at an owned, immutable source checkpoint:

```sh
cargo build -p tor-server --bin tor-service-contract-host --features service-contract-host --locked
```

Set `TOR_SERVICE_CONTRACT_HOST` to that executable's absolute path and
`TOR_SERVICE_CONTRACT_SHA256` to the hash recorded with its qualified build,
then run:

```sh
python -m unittest discover -s scripts -p service_contract_tests.py -v
```

On Windows the executable ends in `.exe`. Respect `CARGO_TARGET_DIR` and the
selected profile when locating it. The adapter never invokes Cargo or selects a
different executable. Missing configuration is an error, not a skipped suite.
The host announces its adapter version, selected feature and protocol identity;
the Python adapter checks the current source protocol and the expected
executable SHA-256 before creating a case. That hash is caller-provided artifact
identity, not qualification by itself. A hash is not sufficient evidence
that changed Rust source has been rebuilt; preserve the build's input receipt.

Python-only fixture and assertion changes can run against the unchanged host.
Rust implementation, protocol or host changes require a fresh qualified binary.
The feature is explicitly opt-in and the binary is excluded from ordinary
unfeatured workspace builds. Keep its feature set stable during comparisons;
feature changes themselves invalidate affected build evidence.

## Boundary and isolation

`scripts/service_contract_host.py` sends one typed operation per line. Python
owns ordinary control flow and assertions. There is no expression evaluator,
scenario language or replicated client-state model. `Request` values are the
existing production protocol type, decoded from a complete raw client envelope
by the same `decode_request` byte/depth checks used by transport. Drained frames preserve the production
encoded text, including escaped request IDs and UTF-8 byte lengths.

The current host supports only creating a memory case from authored parts,
connecting, submitting requests, draining, disconnecting, read-only state/history
audit and closing. It does not run a simulation loop implicitly. Account setup
is trusted test setup, not authentication-token or CLI coverage. The adapter
does not implement WebSocket handshake, I/O scheduling, malformed-wire socket
close behavior or transport backpressure; those remain real transport witnesses.
Malformed wire envelopes are explicit host errors here, not simulated socket
replies. Audit is an explicitly privileged test observation, never a public
client response. State/history equality is a limited projection, not a claim
of full RNG, private-authority or durable-state equality.

Only one case is active. Creating another requires closing the first. Case IDs
increase for the host lifetime; an old case/connection pair cannot reach a fresh
case even if Service connection IDs repeat. Closing or EOF drops the Service and
its connections. Invalid envelopes leave the current case available for audit.
The host allows at most eight connection handles and eight actors per account;
closed handles count until the case closes. Its total outbound budget is at most
512 KiB. Input lines are bounded to 1 MiB and host replies to 2 MiB; exceeding
either line/envelope limit explicitly fails and closes the host/case rather
than truncating frames or continuing after lost output. Python also bounds its
reader queue and envelopes, reaps failed children, and reports exit/stderr on
write, read or timeout failure. A whole-operation watchdog also covers pipe
writes and stdin close, so a host that stops consuming input cannot bypass the
response deadline. These are pilot harness limits, not changes to
production limits or a sandbox for untrusted fixture inputs.

## Catalog assertion ownership

The pilot retains the original Rust test
`creature_catalog_pages_fit_4096_byte_transport_and_preserve_query_state` in
`crates/server/src/creature_catalog_session_tests.rs`.

| Contract | Pilot evidence | Retained boundary evidence |
| --- | --- | --- |
| Authored catalog is populated and attachment succeeds | Successful package construction, bounded attach and expected complete selectors | Original Rust test also checks the unbounded Engine page has 32 entries and a continuation |
| Whole encoded response fits 4096 bytes, including escaped request ID | Exact outbound strings, UTF-8 byte count, live connection, 128 backslashes in request ID | Original Rust test and real process bounded catalog query |
| Pagination is complete, bounded and duplicate-free | Nonempty pages of at most 32 entries; terminal continuation; 41 unique selectors | Original Rust test; process test includes population selectors |
| Query preserves state and history | Independent before/after Engine audit | Original Rust, WebSocket and process tests |
| Reply stays private; spectator query is denied | Separate wizard/observer connections and raw messages | WebSocket authorization/context test and process spectator test |
| Fixture/assertion edits do not require Rust relink | Multiple Python fixture variants in one PID; unchanged executable SHA-256 | Record external build/test receipts during the experiment |
| Real CLI, transport and clients work | Not claimed by this adapter | `test_named_creature_catalog_lists_authored_selectors` in `scripts/test_wizard_creature_spawn_process.py` |
| Real native keyboard and visible console work | Not claimed by this adapter | `test_native_catalog_console_uses_real_keys_and_preserves_state` in the same module |

The real WebSocket witness is
`named_creature_catalog_is_private_read_only_and_context_checked` in
`crates/server/tests/it/wizard_websocket.rs`. None of these retained tests is
deleted or weakened. Source-deleted restart, lifecycle, RNG, rollback and other
UI/privacy regressions remain outside this catalog pilot.

## Qualification experiment

The historical catalog bug sent an oversized whole response and disconnected
the client. Restore the pre-fix omission of `wizard_creatures::fit_reply` in a
separate, controlled mutant checkpoint. The pilot must fail at the live-connection
or whole-frame assertion after successful fixture attachment; a compile or setup
failure is not a reproduced bug. Keep the ordinary production implementation
unchanged in the submitted patch.

Additional targeted mutants should drop or duplicate a page entry, mutate query
state/history, and broadcast a private catalog reply. Each is a proposed fault,
not a claim that every fault occurred historically. Record which assertion kills
each mutant; do not infer coverage from names or counts. Preserve independent
hand-authored expected counts rather than generating expectations from the host.

Before considering any relocation/removal, compare original and pilot assertion
maps, bug/mutant detection, isolation, failure diagnostics and developer elapsed
time. A suggested experiment target is halving Python scenario-authoring feedback
time; it is not a measured speedup. Keep build time separate from execution/setup
time and retain unsuccessful runs. Do not benchmark while another owner is using
the build/measurement window. Production changes still need compilation.
