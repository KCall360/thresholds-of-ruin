# Test objectives and PR assurance

The testing regime distinguishes **PR assurance** from **full qualification**.
PR assurance combines cheap deterministic contracts, real boundary witnesses and
change-dependent expansion. Full qualification runs the exhaustive Windows/Linux
debug/release matrix on its configured cadence. A selected PR result must never
be described as that full matrix passing. See the [testing policy](testing.md)
for the authoritative gates and [CI cost baseline](ci-cost-baseline.md) for
measured runner cost, which is different from elapsed time.

## One reviewed owner, current discovery

[`scripts/ci_test_contracts.py`](../../scripts/ci_test_contracts.py) owns the
contract registry. Its `select(paths, discovered_ids, root=..., test_metadata=...)` interface receives
complete changed paths and independent current Python discovery, plus the explicit
physics correctness smoke. Independent metadata records actual `ProcessTestCase`
inheritance and the harness graphical classification without running setup.
Explicit `APPLICATION_OWNERS` additionally marks the rules-source resolver and
bundled-rules authoring classes, which launch the real compiler without inheriting
the process harness. They must not enter a no-build pure-tooling lane. Missing
or inconsistent metadata requests full verification. Both OS debug lanes retain
all pure Python checks, including pure classes in files named `_process.py`, so
path, CRLF and capture regressions are not hidden by filename heuristics. It returns `full_required`, `requires_rust`, `requires_apps`, reasons, contract names,
exact `linux_debug`, `windows_debug`, `linux_release` method IDs and the cheap
`consumer_3d` compatibility requirement. The planner owns complete Git evidence;
the runner owns qualified binaries and exact selected/result reconciliation.
Rust and the production Service suite are separate execution owners. A pure-tooling
change runs the two OS pure-Python lanes without compiling Rust or application
binaries. A process assertion/helper change builds applications but does not by
itself require the Rust suite. Runtime Rust/scenario changes require both. Python
fixtures reaching the separate Service suite select its production Rust host even
when their discovered adapter tests are pure.

The registry reviews **module/class owners**, not a second permanent list of
thousands of method IDs. A changed test module selects all its current methods,
including newly added methods. A new module/class without an owner, missing fixed
boundary witness, duplicate/failed discovery, unavailable/deleted dependency or
unmapped path requires full verification. Review new ownership explicitly; never
regenerate an allowlist automatically merely to obtain a smaller green result.

Python helpers follow transitive local imports, including imports inside functions,
plus the reviewed `RUNTIME_SCRIPT_EDGES` table for CLI-invoked study tools, report
validators and retained source identities that do not appear as imports. A new
literal local-script reference in the affected dependency closure must be imported
or registered, otherwise selection falls back to full; a harmless mock/control
reference may conservatively cause that fallback rather than being auto-approved.
Opaque dispatcher changes such as performance comparison or arena search require
full verification. Synthesized filenames and arbitrary runtime dispatch are not a
whole-program proof: reviewers must register new dispatch relationships or retain
full selection, never assume the literal scan discovers every computed edge.
Dynamic file reads are not import edges: unowned script fixtures/data fall back to
full rather than disappearing. Scenario trees have explicit broad owners. Unknown
or deleted source edges fail closed. This is a conservative dependency model, not
a proof of every semantic dependency or a substitute for reviewing changed
contracts. Additional feature-specific tests remain required by the testing policy.

## Objectives, not counts

| Family | Objective and independent evidence | PR ownership | Extended ownership |
| --- | --- | --- | --- |
| World/simulation | Deterministic geometry, arithmetic, actor rules, independent expected values, ordinary actors and cross-feature interactions | Rust unit/integration owners; affected application witnesses | Full profiles/platforms; broader workloads |
| Service | Production package constructor, Service and encoded requests/replies; exact pagination, private authority, scheduler decisions | Production Service suite; permutations at this narrow boundary | Full Service suite; never a replacement for sockets or durable storage |
| Protocol/schema | Strict current-format fields, historical-version rejection, current golden bytes, independent recorded samples | Rust codec/fixture tests and affected clients | Complete profile/platform matrix |
| Persistence | Durable save barriers, rejected-command atomicity, duplicate receipts, corruption preservation, checkpoint/journal recovery | Real crash/restart sentinel; affected storage owners | All fault points and full matrix; process kill does not prove hardware power-loss durability |
| Application | Actual headless/Text clients, ordinary action effects and disclosed output | Small fixed cross-feature witnesses plus affected process owners | All actual-process contracts |
| Native | Genuine keyboard/mouse path and presented result | Windows native sentinel and affected native tests | All Windows/Linux native contracts; no implicit graphical waiver |
| 3D | Shared API/model compatibility versus actual GPU presentation | Cheap downstream compatibility for protocol/client-common changes; own workflow for client changes | Full native rendering matrix; scenario-only refresh is not a full-render trigger |
| Scenario/content | Authored source, compiler, certificates, source-preserving fixtures and generated identities | Source checks plus compiler/ordinary boundary owners | All package invariant and topology routes |
| Performance correctness | Stable work/count bounds and complete valid producer output | Deterministic counters and eight-shape physics smoke | Full repeated benchmark samples; controlled release comparisons separately |
| Floor route | Paid discovery, complete ordinary traversal, immutable room/gold identities and source-free revisit | Full route when topology/discovery/save authority is affected | Complete omission and dark-maze routes otherwise |
| Earned route | Ordinary progression to earned HD2, repeated execution and exact source-free recovery | Full route when growth, navigation, route driver or persisted authority is affected | Complete fixed-seed campaign otherwise; not a balance/breadth claim |
| Arena/studies | Curated scenario compilation, ordinary engine execution, exact replay | Affected owner and representative maintained producer | Full curated enumeration |
| Tooling | Selection, receipts, reports, fixtures and driver completion discipline | All pure source/tooling tests on both OS | Platform-specific helper and exhaustive caller coverage |

The registry records objective, cost category and determinism/boundary rationale in
`OBJECTIVES`. Its source domains deliberately select broad ordinary application
coverage for core Rust changes. This first regime favors clear ownership over a
fragile per-symbol dependency graph. Optimization of those broad domains needs
measured cost and assertion-level evidence, not test-count targets.

## Fixed independent boundary witnesses

The exact method IDs live in `SMOKE_LINUX`, `SMOKE_WINDOWS` and `SMOKE_RELEASE`.
Missing or renamed witnesses cause conservative full selection, never an empty pass.

- Generated paired stairs: ordinary down/up, tick payment, different landing,
  spectator state equality, save/reopen and revisit; no private region IDs.
- Short journal-only exploration: four ordinary disclosed moves, no checkpoint,
  source deletion, exact state/history restore and continued ordinary work.
- Text discovery under checkpoint and journal recovery: free explicit examination,
  known/unknown trap-family privacy, secret entrance opening, source deletion and
  exact state restoration without wizard authority.
- Windows native paired stairs: real shifted keys and presented tick/landing.
- Release checkpoint-tail crash/restart: real child process and durable recovery.
- Physics: all eight actor/item/body/falling shapes once, with the same per-row
  completed durable status assertions as the full three-samples-per-shape workload.

These witnesses are **not equivalent to** a full long Rogue route. In particular,
short exploration and explicit trap examination do not establish full paid Search,
tactile dark-maze navigation, earned progression or survival balance. Those unique
routes remain affected or exhaustive owners. Adding a compact combined ordinary
Search/tactile/source-free witness would improve the common PR signal; it must
assert payment, disclosure, real traversal, saved knowledge and continued play,
without a private route oracle or a censored-as-success outcome.

The physics split removes repeated samples from the common correctness gate,
not workload shapes or durability assertions. `test_physics_smoke_selection.py`
independently mutates each status field in each row to check both modes reject
invalid reports. Full repeats remain maintained; smoke is not statistical timing
evidence. Do not add noisy wall-clock thresholds to shared CI runners.

## Affected expansion and honest deferral

Shared simulation/world runtime source changes conservatively select **both** long
routes on Linux; simulation also selects the curated arena owner. This broad
contract avoids an incomplete per-symbol graph while reducing repeated full-matrix
execution. Finer selection needs independent evidence later.

Full floor routes additionally have explicit generator, discovery, perception, geometry,
activation and persisted-authority owners. Full earned routes have growth,
combat/recovery/intention, controller and persisted-authority owners. Shared
save/restore representation changes select both. Unrelated protocol additions
select codecs and client compatibility, not automatically every long route;
route-facing observation/map/advancement representation changes do expand them.
New protocol source modules require an explicit owner review or select full.
Unknown ownership falls back to full.

Long OS-pressure tests retain the default and bounded outbound configurations:
these are distinct contracts until assertion parity proves otherwise. Actual socket
closure, continued player progress, replacement spectator state and durable replay
cannot be replaced with a Service memory case or a sleep. Changes to their source
or direct helpers select all methods in the owner; relevant transport changes
expand the extended domain.

No behavioral test is deleted by this regime. A selected result documents the
changed detection cadence. When relocating or consolidating an assertion, record
original and replacement IDs, positive fixture validity, independent expected
values, retained unique boundaries and an intended behavioral first-red mutation.
The [Service catalog consolidation](service-contracts.md#catalog-assertion-ownership)
is the precedent: encoder-fitter mutation parity was demonstrated while real
WebSocket, CLI and native witnesses stayed separate.

## Evidence limits

The initial objective audit sampled assertions and helper paths across all families;
it was not a line-by-line proof of every method. Checked-in duration estimates were
incomplete and are not job billing. New registry source tests establish selection
contracts only; application qualification still requires actual selected execution
and full cadence evidence. Report not-run, skipped, waived and failed checks
separately. Counts, a hash, a setup error or a successful zero-test filter do not
establish behavior coverage.
