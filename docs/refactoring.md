# Refactor plan

This is the accepted refactor scope. Implementation proceeds in verified
increments; a listed design is not a claim that it is implemented. Text-client
improvements and a scripting runtime are deferred.

## Contracts to preserve

The server owns authority, world topology, scheduling, and disclosure. Gameplay
requests admit intentions to a simulation-owned action queue. Admission and
execution are distinct: the scheduler resolves due actions and publishes their
effects. Queries, annotations, and session operations can finish immediately.
Player, AI, travel, and eventual script intentions should use the same execution
path. Submit-time checks do not replace execution-time checks when intervening
actions can change targets, timing, or authority.

Every authoritative mutation is prepared against private candidate state. Its
journal record and required content must be admitted to persistence before state
and ordered effects are published. Retries resolve existing receipts; reconnect
must not duplicate an uncertain action. Queue, cancellation, restart, rewind,
and region-freezing semantics belong in deterministic saved state.

Spatial indexes are backend-only derived data. Keys use region-local locations;
no global Euclidean distance is assumed. Occupancy includes every portal-resolved
body cell and its frame. Queries crossing regions follow portal topology. Clients
may index disclosed opaque identities and relative projections, never private
region identities or authoritative occupancy.

## Work sequence

1. **Explicit boundaries and focused responsibilities.** Replace implicit JSON
   conversions with exhaustive typed mappings. Separate transport, backend
   commands, simulation outcomes, journal entries, disclosed views, authoring
   schemas, compiled content, and persistence schemas. Extract responsibilities
   from the engine and session without changing ordering or durability.
2. **Reuse derived work.** Build each actor's observation once per boundary and
   reuse it for revision comparison, navigation knowledge, and client publication.
   Preserve observer privacy and intermediate memory. Prepare an AI decision once
   at its valid boundary. Share route searches. Add spatial and historical indexes
   with centralized mutation and rebuild after load, replay, rewind, and region
   transitions. Compare optimized queries with reference scans.
3. **Admission, scheduling, and stream recovery.** Make action admission and
   completion explicit; bound mailbox work and output queues with fair scheduling.
   Define slow-controller and spectator policies. Add stream identities, reset
   epochs, exact delta bases, readiness revisions, contextual receipts, and
   resynchronization. Validate snapshots and deltas centrally, including duplicate
   identifiers, arithmetic overflow, bounds, and collection invariants. Measure
   collection deltas against encoded full snapshots. Keep projected occurrences
   separate from underlying entities and protect journal identity from disclosure.
   Bound decode bytes and nesting and outbound bytes as well as message counts.
   Make integer encoding safe for future JavaScript clients. Add explicit
   capabilities and opaque, typed interaction identifiers without exposing
   private topology or eventual script functions.
4. **Scenario compilation.** Decouple authoring types from simulation internals.
   Normalize defaults and inheritance, resolve references with source diagnostics,
   and compile immutable content. Pin dependencies by identity and digest.
   Derive generation seeds from semantic parameters, stable region identity,
   generator version, and an explicit salt; use named random streams for geometry,
   population, and loot. Keep structural metadata and identifiers independent of
   activation order. Preserve lazy construction and distinguish sampled validation
   from proof of all possible generated worlds.
5. **Persistence and history.** Introduce explicit save DTOs and independent
   format/version axes. Pool immutable content across rewind boundaries. Keep the
   journal authoritative and receipts/indexes reconstructible. Bound decoding and
   reject duplicate/unknown fields in one validation path. Admit source blobs and
   journal records atomically; paths are locators, digests are identity. Measure
   compression and chunking before adoption. Add indexed, privacy-filtered history
   pagination without silently pruning retained history.
   Design portable save export separately from runtime storage. Bound resident
   history using storage-backed access when measurements justify it.
6. **Latency and extension seams.** Remove synchronous diagnostics from hot paths,
   shorten storage lock scope, and measure region fallback and checkpoint capture.
   Retain existing workload targets and investigate timing tails. Define future
   extension contracts around scoped queries, validated effects, deterministic RNG,
   named handlers, persistent typed state, scheduling, and region suspension.
   No language, VM, or package script support is added in this refactor.

## Verification

Use the repository verification tiers and process tests for touched boundaries.
Exercise retries, save/reload, replay, rewind, disclosure, reconnect, generated
regions, portal transforms, and scheduling. Prove scaling with operation counts
at multiple world sizes and compare release workload measurements before claiming
performance improvements. Windows and Linux CI remain required before merge.
Compatibility-breaking decisions must be stated explicitly before adoption.

## Current checkpoint

The ordinary wire/backend command mapping is explicit. Developer command text
still uses its existing parser and opaque wire representation. No wire, package,
save, or ruleset format has changed. Action broadcasts resolve disclosed state
once per watched actor while retaining each client's independent delta base and
stream sequence. A derived boundary cache shares the observation and exact scene
between reads and revision detection; committed ordinary actions publish their
already-computed post-action views. Region transitions invalidate those views,
and load, rewind, and diagnostic seeding start from fresh derived data. The action
queue and remaining items above are pending.

The full Windows verification tier passed in debug and release. Tests of the
deployed text, ASCII spectator, and headless executables also passed. Linux CI is
still required before merge. No merge or raw-measurement publication has occurred.

### Initial release measurements

Three interleaved baseline/refactor rounds on the same Windows host compared
ordinary play and region streaming. Values below are milliseconds, baseline to
refactor; `n` is the number of command samples on each side.

| Case | n | p50 | p95 | max | Scene/perception calls per run |
| --- | ---: | --- | --- | --- | --- |
| 8 regions, 1 actor, 100 history entries | 915 | 0.615 → 0.559 | 0.893 → 0.819 | 1.512 → 1.015 | 640 → 340 |
| 64 regions, 8 actors, 100 history entries | 7,500 | 0.020 → 0.026 | 2.751 → 2.632 | 8.076 → 9.997 | 9,848 → 6,759 |
| Streaming, 256 regions | 2,100 | 0.229 → 0.217 | 0.412 → 0.394 | 0.763 → 0.906 | 1,710 → 890 |

All runs validated; history, client memory, and saved byte counts were unchanged.
The eight-actor median increased by six microseconds, and maxima rose in two
cases; these measurements establish reduced derived work and lower sampled p95,
not improvement in every timing statistic. Server cache memory has not yet been
measured directly. Raw samples remain local; publication requires separate
maintainer authorization under the performance policy.
