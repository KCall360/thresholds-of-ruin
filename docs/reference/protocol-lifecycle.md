# Protocol lifecycle and integration

This chapter explains how a client crosses connection, command, publication and
recovery boundaries. The [wire reference](protocol.md) owns exact payload fields,
bounds and decoder-checked examples. This is the ordering contract around those
fields, not a second schema. The [current-format registry](../status.md#implemented-scope)
owns supported version identities; the [simulation cycle](../architecture/simulation-cycle.md)
explains how admitted work actually executes.

## Transport and handshake

The server uses JSON over WebSocket text messages, not newline-delimited JSON on
a raw socket. One complete application message contains one complete JSON
envelope. WebSocket fragmentation does not turn its pieces into separately
admissible requests. The [headless driver](headless.md) has a separate JSON-lines
interface; its input is not the socket protocol.

1. Connect to the configured loopback endpoint. The native server rejects a
   WebSocket upgrade carrying an `Origin` header; it is not a browser endpoint.
2. Send `hello` first, with the exact supported `protocol`, the session `token`
   and a `frontend` label. Version mismatch and invalid authentication fail the
   connection before game-state disclosure. The frontend is attribution, not a
   requested user identity, role or permission.
3. Validate `welcome`. Its `user`, `role` and `actors` come from the authenticated
   account. Its capabilities are static ceilings, not reserved capacity and not
   permission to control an actor.
4. Send a correlated `attach` naming an actor from the authorized set. All other
   host requests require attachment. The attachment cannot switch actors;
   reconnect to choose another. Install the complete returned snapshot before
   issuing input.
5. If appropriate for the role, request `acquire_control`. Attachment grants a
   view, not the exclusive control lease. Another connection may hold the lease;
   scenario AI actors cannot be acquired as human-controlled actors.

The WebSocket upgrade and initial-message waits each have a five-second server
deadline. After `hello`, application messages must be `request` envelopes. A
second hello or malformed application message is not an ordinary recoverable
command rejection. Binary WebSocket messages are not another encoding of this
protocol. Ping/pong and close are transport control traffic, not game requests.

Request IDs and frontend labels must be nonblank, at most 128 UTF-8 bytes, and
contain no control characters. The shared client chooses UUID request IDs, but
UUID syntax is not a wire requirement. Keep command IDs unique across the
**authenticated user's command history**, not merely one socket or frontend.

Limits count complete UTF-8 JSON envelopes, including escaping and whitespace.
The shared decoder bounds nesting before typed JSON decoding. A small
`observation_delta` must still reconstruct a full state within the independent
retained-state ceiling. A large recovery snapshot can use ordered `snapshot_part`
envelopes, but the entire logical snapshot and encoded transfer remain bounded.
No individual part is an observation or input authority: install only after
complete assembly and validation. Exact limits and multipart rules belong to
[decode boundaries](protocol.md#decode-boundaries),
[connection and control](protocol.md#connection-and-control), and
[output ownership](protocol.md#output-ownership-and-backpressure).

Implementation: [`ClientMessage`, `ServerCapabilities` and `AccessRole`](../../crates/protocol/src/wire.rs),
[transport connection handling](../../crates/server/src/transport.rs), and
[`Service::connect` / `Service::process`](../../crates/server/src/session.rs).

## Identities and clocks are not interchangeable

| Field or identity | Owner and meaning | Do not use it as |
| --- | --- | --- |
| Authenticated user and role | Token/account authority; the server chooses them | Client attribution or a control lease |
| Attached actor | Authorized perspective fixed for this connection | Permission to command every actor named in private inspection |
| `request_id` | Client correlation; accepted command deduplication is keyed by authenticated user and ID | A simulation sequence number or proof of disk persistence |
| `branch` | Journal lineage; fresh commands name the current branch | A transport reset identifier |
| Stream ID and `epoch` | Attachment identity and reset generation; a new connection has a new stream | A saved replay position or intention ID |
| Update `cursor.sequence` | Order of published updates on this attachment | Global simulation order, action count or the latest picture base |
| Update `cursor.tick` | Simulation time associated with publication | A unique update identity; several updates can share a tick |
| State `revision` and `snapshot` | Issued picture generation and opaque picture identity | Gameplay permission or proof that no hidden work happened |
| `readiness.revision` | Generation of this attachment's input authority | Picture freshness, simulation time or permission by itself |
| Intention ID | Original admitted work, retained through its lifecycle | A fresh action, current target handle or client request ID |
| Entity target | Typed, observer/save-scoped opaque operand | A global entity ID or authority independent of current disclosure |
| Receipt actor/branch/entry | Original accepted operation and its journal identity, if any | The current attachment boundary |
| Reply `context` | Exact disclosed boundary at reply publication | A patch installing missing state, or evidence of execution |

Sixty-four-bit wire counters and identities use the canonical decimal strings
specified in [integer representation](protocol.md#integer-representation). Do not
round them through floating-point JSON numbers. Typed target references remain
opaque; see [entity target references](protocol.md#entity-target-references).

A client therefore tracks at least three separate things: its latest ordered
stream cursor, the exact base of its latest picture, and current input readiness.
A control or intention update advances the first without necessarily changing
the second. A new picture does not itself grant the third.

## Connection state and input state

These are independent state machines, not one `ready` boolean:

- **Transport:** disconnected → authenticating → welcomed/unattached → attached.
  Disconnect ends the attachment. Reconnect starts a new stream and a fresh
  snapshot; resuming an old socket sequence is not implemented.
- **Synchronization:** a validated snapshot establishes a model; valid updates
  extend it. A recoverable ordering/base inconsistency starts snapshot repair.
  New application requests are blocked while the shared connection is repairing.
  Invalid wire syntax, a reply belonging to another actor/attachment, or failed
  repair is terminal.
- **Control:** an attached observer may have or lack the exclusive lease.
  Gaining control does not implicitly resume suspended work. Losing it changes
  input authority even if the visible picture remains identical.
- **Admission:** `readiness.admission` permits offering new gameplay work under
  current run/control/queue policy. It does not promise that the selected target
  is valid or that the work will execute immediately.
- **Existing work:** `readiness.resume` and `readiness.cancel` authorize operations
  on those exact intention IDs. Membership, not a guessed phase or old snapshot,
  is the client-visible permission boundary.
- **Scheduling:** `waiting` explains why the run stopped (`you`, `others`,
  `unclaimed`, `paused`, `stopped`). It helps pacing but is not authority to act.

An observation's actor-ready status does not replace the following readiness
update. Consume ordered control/lifecycle/readiness changes before constructing
the next command. UI-queued input retains its originating context; do not silently
restamp it after a reset, control handoff or changed operand authority.

Source owners are [`Readiness` and `InputContext`](../../crates/protocol/src/wire.rs),
[`StreamContext`](../../crates/protocol/src/lib.rs),
[`Service::current_readiness` and request processing](../../crates/server/src/session.rs),
and the shared [`Connection`](../../crates/client-common/src/connection.rs).
Exact permission rules are in [readiness and input freshness](protocol.md#readiness-and-input-freshness).

## Complete message-family index

Names below are the serialized snake-case tags. Each request can also receive a
scoped `error`; transport failure can prevent any correlated reply. This index
lists every family and its authority, leaving full field schemas in their owners.

### Top-level requests

The complete definitions are [`ClientMessage` and `Request`](../../crates/protocol/src/wire.rs).

| Request | Successful result and purpose | Authority / canonical detail |
| --- | --- | --- |
| `attach` | Correlated `snapshot`, possibly as `snapshot_part`s; initial palette and waiting information | Once per connection, authorized actor; [connection](protocol.md#connection-and-control) |
| `acquire_control` / `release_control` | Immediate `ack`; ordered control/readiness effects as applicable | Player/wizard, exclusive lease rules; [connection](protocol.md#connection-and-control) |
| `continue` | Immediate `ack`; permits autonomous recovery, not admission of a new action | Attached controller; [pushed updates](protocol.md#pushed-updates) |
| `save` | Immediate-kind receipt after the requested accepted prefix is durable | Player/wizard, attachment required, no control requirement; [persistence](protocol.md#history-and-persistence) |
| `snapshot` | Correlated complete reset, optionally multipart | Any attached role; [recovery](protocol.md#bounded-stream-recovery) |
| `observe` | Ordered picture publication, not a correlated ack | Any attached role; [outcomes and pictures](protocol.md#ordered-outcomes-and-demand-driven-pictures) |
| `map` | Correlated `map_page` | Attached actor's appearance-only remembered projection; [map pages](protocol.md#explored-map-pages) |
| `chart` | Correlated `chart_page` | Attached view's immutable detection/mapping samples; [charts](protocol.md#detection-and-mapping-charts) |
| `palette` | Correlated `palette` | Any attached role; [asset palettes](protocol.md#asset-palettes) |
| `history` / `history_branch` | Correlated `history` page for current / selected retained branch | Authorized history of attached actor, filtered before pagination; [history](protocol.md#history-and-persistence), [retained branches](protocol.md#wizard-commands-and-retained-branches) |
| `command` | Receipt or family-specific query response below | Required input context and branch; command-specific authority |

Spectators have an explicit allowlist: `attach`, `snapshot`, `observe`, `palette`,
`map`, `chart`, `history`, `history_branch`. They cannot gain mutation or private
query access by wrapping it as a command, recovering a previous player-command
receipt, or supplying a player frontend label. See
[spectators](protocol.md#read-only-spectators).

### Commands inside `request.command`

The complete definition is [`Command`](../../crates/protocol/src/wire.rs), with
explicit normalization in [`decode_command`](../../crates/server/src/wire_adapter.rs).
All fresh commands use input context and branch; these are additional gates.

| Command | Result / behavior | Additional authority and detailed owner |
| --- | --- | --- |
| `act` | Admitted receipt; later execution and intention/outcome updates | Control and admission, typed action operands; [connection](protocol.md#connection-and-control), [gameplay projections](protocol.md#durable-places) |
| `resume_intention` | Receipt and lifecycle updates under original intention identity | Control and exact resume permission; [resume/cancel](protocol.md#connection-and-control) |
| `cancel_intention` | Receipt; cancel in place or atomic cancel-and-step | Control, exact cancel permission, required nullable step; [resume/cancel](protocol.md#connection-and-control) |
| `travel` | Immediate receipt for journey request, ordered travel status and admitted travel steps | Control, admission and applicable display guard; [durable places](protocol.md#durable-places) |
| `rename_place` | Immediate receipt and changed disclosed place metadata | Control and applicable display guard; [durable places](protocol.md#durable-places) |
| `annotate` | Immediate receipt and authorized annotation update | Valid accessible anchor/audience; no control requirement; [annotations](protocol.md#annotations) |
| `projectile_choices` | Correlated `projectile_choices` page; no gameplay admission | Control and display/query basis; [projectile choice integration](#projectile-choice-integration) |
| `preview_ability_weapon` | Correlated `ability_weapon_preview`; no gameplay admission | Control and display guard; [exact weapon previews](#exact-abilityweapon-previews) |
| `advancement` | Correlated `advancement_page`; no gameplay admission | Own attached actor under control, display/build basis; [advancement schema](protocol.md#owner-advancement-schema) |
| `inspect_strike_source` | Correlated `creature_strike_page`; no saved receipt | Wizard role plus permanent wizard mode, display/target/cursor basis; no control requirement; [strike pages](protocol.md#private-strike-source-detail-pages) |
| `wizard` | Operation-specific: journaled mutation receipt or dedicated private reply | Wizard role plus permanent wizard mode, applicable display guard; [wizard operations](protocol.md#wizard-commands-and-retained-branches) |

The `wizard` family includes developer text for creature listing, creature
inspection and combat diagnostics. These return `creature_catalog`,
`creature_inspection` and `combat_diagnostics`, respectively, not ordinary actor
observations. Their private access does not require the attached actor's control
lease. The family also contains privileged mutations: do not classify all wizard
operations as read-only queries. See [creature inspection](protocol.md#privileged-creature-inspection),
[creature catalog](protocol.md#private-creature-catalog), and
[diagnostic procedures](../development/wizard.md).

### Server envelopes and ordered bodies

The complete definitions are [`ServerMessage` and `UpdateBody`](../../crates/protocol/src/wire.rs).

- `welcome` establishes authenticated identity and advertised ceilings.
- `snapshot` / `snapshot_part` establish a complete validated reset, correlated
  through the snapshot request identity. Parts grant nothing independently.
- `update` carries one ordered body: `outcome`, `readiness`, `intention`, `travel`,
  `observation`, `observation_delta`, `annotation` or `control`.
- `ack` carries a correlated immediate or admitted receipt and current reply
  context. It is neither an observation nor blanket execution success.
- `projectile_choices`, `ability_weapon_preview`, `map_page`, `chart_page`,
  `creature_catalog`, `advancement_page`, `combat_diagnostics`,
  `creature_strike_page`, `creature_inspection` and `history` are correlated
  context-bearing replies. They do not advance stream sequence or install
  missing ordered state.
- `palette` has an optional request ID: it may answer a request or be pushed.
  Its asset revision is separate from picture and readiness revisions.
- `waiting` is uncorrelated scheduling information, not a new ordered state base.
- `error` has required scope and optional request ID. An unsolicited error can
  describe execution capacity or storage trouble rather than reject a new request.

## Command processing: order matters

For a well-formed command request, the host follows this order. Reordering the
gates can change authority or retry semantics.

1. **Envelope and identity validation.** Transport decoding and host request-ID
   validation reject malformed input before processing the command.
2. **Role and attachment authority.** Wizard-only families require both wizard
   role and permanent wizard mode. The role allowlist is checked before
   attachment, receipt lookup or mutation. The connection must be attached to an
   actor authorized at attachment. Unauthorized roles cannot use retries as a
   receipt-disclosure bypass.
3. **Typed normalization, then accepted-receipt lookup.** The lookup key is
   authenticated user plus request ID. A hit must also match original actor,
   branch and normalized command; otherwise it is `request_conflict`. Frontend
   labels and sockets are not separate receipt namespaces. Opaque targets are
   compared against the saved command's scoped encoding without requiring the
   old target still to exist in current disclosure.
4. **Return an exact accepted retry before fresh-input checks.** It does not
   execute or admit another copy. Its receipt identifies the original operation;
   its reply context describes this attachment now. An admitted receipt keeps
   the original identity but reports the intention's current lifecycle phase.
5. **Fresh generation, branch and control.** On a receipt miss, input context
   must match both the last disclosed generation and current authority. Then
   the current branch is required, followed by control when the family requires
   it. Returning to the same position or losing and regaining control does not
   revive an old input generation.
6. **Family-specific validation and resolution.** Queries check their display,
   build or inspection basis and return dedicated replies. Mutating commands
   resolve operands against the applicable disclosed boundary, validate current
   admission/resume/cancel permissions, and apply simulation/journal rules.
   Fresh authority does not waive target, cost, capacity or legality checks.
7. **Publish command effects and permissions before completion.** Ordered
   effects and readiness changes are admitted to output before the command's
   correlated reply, whose context names the resulting disclosed boundary.
   Output admission failure can close the connection even after work was accepted.

This retry guarantee belongs to accepted journaled commands. Do not assume every
read-only page query or session operation has a durable cached response merely
because it also has a request ID. Do not turn a retry into new work by changing
its operands, branch or ID; do not restamp its original input context. A
deliberately new command is constructed from current disclosed authority.

Implementation: [`Service::process` and `Service::handle`](../../crates/server/src/session.rs),
[`Engine::retry_decoded`](../../crates/server/src/wire_request.rs),
[`DecodedCommand::matches` / `requires_control`](../../crates/server/src/wire_adapter.rs),
and [`Engine::retry_matching`](../../crates/server/src/engine.rs).

## Admission, execution, display and durability

Successful action admission is not the final action result. The server accepts
an intention into a bounded queue; the simulation chooses when it executes.
Execution revalidates live conditions and never silently replaces a selected
target. Preparation and ordered strikes retain the original intention identity
until explicit terminal evidence. Paused preparation can coexist with independent
queued work. Control loss, disconnect and restart suspend human work rather than
treat a later control acquisition as consent to resume it.

There are four different observable boundaries:

1. **Acceptance:** the admitted or immediate receipt proves the corresponding
   request was accepted, with the meaning appropriate to that receipt type.
2. **Execution/lifecycle:** ordered outcome and intention updates describe what
   actually happened. One intention can produce several outcomes before it ends.
3. **Picture publication:** `observe` requests a fresh picture without resetting
   attachment or input authority. Its result is an ordered observation, with no
   request-ID-bearing ack. Clients complete visual demand when that publication
   is applied; narration comes from outcomes, not by diffing pictures.
4. **Durability:** ordinary acceptance is bounded in-memory journal admission.
   Successful `save` waits for the accepted prefix captured when that request
   was processed. It does not certify every later action, stop travel, or wait
   for all admitted intentions to finish.

Outcome facts include event-time disclosed information and complete nullable
owner status. Update that status even if no new picture has been requested.
`visual_changed` signals useful visual demand, not a hidden-world event counter.
Hidden-only work need not publish anything. Repeated observations must not repeat
narration, and coalescing pictures must not discard ordered outcomes.

A publication's cursor/base advances only after ordered output admission. That
is not socket delivery or disk persistence. A slow reader can cause the host to
pause simulation while still servicing its mailbox; sustained fullness or a
failed bounded write disconnects it rather than silently dropping outcomes.
Reconnect restores current state and authorized history, not unlimited replay
of transient animation events.

See [outcomes and pictures](protocol.md#ordered-outcomes-and-demand-driven-pictures),
[publication limits](protocol.md#publication-and-transport-limits),
[backpressure](protocol.md#output-ownership-and-backpressure), and
[background persistence](../architecture/persistence.md).

## Paging and side-channel data

A page's request correlation, outer reply context and inner data basis answer
three different questions: which request was answered, where this attachment was
when answered, and which immutable or revisioned data the page describes. Do not
replace an inner basis with the newest picture just because the outer context
is current.

| Family | Continuation/basis discipline | Completion does not grant |
| --- | --- | --- |
| Explored map | Exact stream/reset/branch/observation base and window; opaque transfer plus next position/page; whole validated transfer installed atomically | Live occupants, actionable targets or permission to load unseen regions |
| Detection/Mapping chart | Immutable sample and context-bound page cursor; preserve its current/historical distinction | Fresh perception or current action authority |
| History | Authorized entries first, then bounded `before`/`older_before` pagination; retained branch explicitly selected when requested | Private entry counts or a transient outcome replay feed |
| Advancement | Actor/branch/stream/reset/build basis and exact bounded offsets; proposal pages are recomputed, not an unbounded candidate cache | A purchased build, reserved price or future acceptance |
| Projectile choices | Display-bound query echo, phase and inventory offset / `next_offset`; empty filtered pages can continue | An admitted throw/shot or substitution of another projectile |
| Creature catalog | Wizard-only ordered selector continuation and optional depth | Current ordinary visibility or player spawn permission |
| Strike source detail | Wizard-only inspection identity, target revision, source and typed section cursor | An ordinary combat target or simulation mutation |

Map and chart paging are top-level reads. Advancement, projectile choices and
private inspection are command families, so they also follow command context
and branch gates. Queries requiring control remain reads: displaying a preview
while an actor is busy does not admit an action.

A newer picture does not silently retarget an issued immutable map transfer.
A reset or branch change invalidates its continuation. A shared client may cancel
an old assembly instead of displaying it, but must never combine different
transfers. During stream repair it quarantines query contents rather than treating
a plausible page as proof of synchronization. Specialized limits and bases remain
in the [map](protocol.md#explored-map-pages),
[chart](protocol.md#detection-and-mapping-charts),
[advancement](protocol.md#owner-advancement-schema),
[projectile choice](#projectile-choice-integration),
[catalog](protocol.md#private-creature-catalog), and
[strike-detail](protocol.md#private-strike-source-detail-pages) owners.

### In-flight advancement replies

An advancement query binds its issued picture and input authority, not a frozen
outcome cursor. The shared reducer permits its pending reply after ordered cursor
progress only when actor, branch, complete input context, snapshot identity and
picture revision are unchanged. The cursor must either equal the request cursor,
or have a strictly greater sequence and a tick no earlier than the request's.
Request ID, query echo, page revision, basis, section totals and envelope bounds
retain their existing validation.

For example, a query issued at cursor `(20, 100)` can receive its page after an
ordered outcome advances the connection to `(21, 100)` without changing that
picture or input authority. `Connection` must already have applied that progress,
and the reply's complete context must match its current context. The page is then
installed at the reply context. This is not permission to accept a reply ahead of
the connection, reuse a backward cursor, tolerate a changed readiness generation,
or substitute another picture. It changes no delta-base or stream-repair rule.

Retained **displayed** pages still require exact current context equality and no
invalidation. Cursor progress can stale a displayed page while preserving a
compatible pending query; a valid reply refreshes it. Control loss clears private
pages. Current page data does not itself authorize a purchase: previews and the
separate confirmation still pass the ordinary owner/context/readiness checks.
The supply chooser is an explicit overlay over retained page data, not another
catalog page and not a grant to dispatch the underlying catalog's choices.

Source: [`AdvancementPages`](../../crates/client-common/src/advancement.rs),
`observe`, `validate_reply`, `is_current`, `shows_supplies` and
`pending_context_matches`; [shared connection](../../crates/client-common/src/connection.rs).
This is client query-lifecycle behavior, not a wire schema/version change.

### Projectile choice integration

`projectile_choices` is an owner-only structural menu, not a hidden affordability
or numeric-weapon probe. The command carries `expected_revision` and a query with
`ability`, `phase` and `offset`. Phase is `sources`, or `ammunition` with the exact
equipped launcher selection. Offset indexes the disclosed inventory, **not** the
filtered result list. The response echoes the query and returns typed `throw`,
`launcher` or `ammunition` choices plus required nullable `next_offset`.

A page evaluates at most 128 inventory candidates, returns at most 16 choices,
and fits an 8 KiB complete correlated envelope or the smaller host limit. Exact
launcher-token resolution is separate from that candidate-scan bound. Filtered
rows consume scan budget: an empty page with non-null `next_offset` means continue,
not “no more choices.” Follow that returned offset instead of adding the visible
choice count. Validate query echo, request ID and current reply context together;
a changed display/query basis requires a new query sequence.

Recommendation eligibility uses disclosed type knowledge and exact source/socket
structure. A choice supplies operands for a later deliberately constructed action;
it reserves neither ammunition nor resource balance and promises neither numeric
capture success nor execution success. Source contracts:
[`ProjectileChoiceQuery` / `ProjectileChoicePage`](../../crates/protocol/src/projectile_choices.rs)
and [bounded host query/packing](../../crates/server/src/projectile_choice_query.rs).

### Exact ability/weapon previews

`preview_ability_weapon` carries `expected_revision`, `ability`, exact `invocation`
and `supplies`. Its `ability_weapon_preview` response echoes those operands and
adds `phase`, required nullable `cost`, `preparation` and `recovery`. Both times
are canonical unsigned decimal strings. The preview uses the issued display
boundary and the same work capture/resume path as ordinary preparation, but does
not mutate randomness, reserve resources or promise affordability.

Phase distinguishes `fresh`, `resuming` and `paid_prefix`. For a matching retained
plan, preparation is remaining time, not its original full duration. The cost
bundle describes captured work; a retained paid prefix must not be presented as
a second fresh payment. This is descriptive, not an admitted receipt or future
reservation. Submit a separate action or explicit resume only when its current
authority permits it. The complete response, including operand echo and reply
context, must fit the negotiated response ceiling; an oversized preview returns
`resource_limit`, not a partial result.

Source contracts: [preview DTO](../../crates/protocol/src/weapon_preview.rs),
[issued-boundary resolution](../../crates/server/src/wire_request.rs), and
[shared simulation preview](../../crates/simulation/src/abilities/weapon_selection.rs).

## Errors, repair and reconnect

Interpret error scope before the code or prose:

- **Transport:** no host ordering boundary. Terminate the connection; it cannot
  establish acceptance or rejection of an unrelated pending host operation.
- **Unattached:** host rejection before attachment. Receiving this scope after
  attachment is not a valid continuation of the shared client's model.
- **Attached:** exact reply context, like a successful correlated reply. A known
  host rejection remains known even if that boundary also triggers stream repair.

| Codes | Meaning and client response |
| --- | --- |
| `unauthorized`, `version_mismatch` | Authority or compatibility failure; neither a different frontend label nor snapshot repair grants missing authority/version |
| `not_attached`, `already_attached` | Connection lifecycle violation; use the correct attach/reconnect path. Unsolicited `not_attached` can announce that the observed actor left the loaded world before disconnect |
| `control_taken`, `not_controller` | Lease problem, not a stale picture; wait for or explicitly acquire available control |
| `stale_context`, `stale_revision`, `wrong_branch` | Input or display/branch basis is no longer usable for fresh work; consume current disclosed authority and deliberately reconstruct the request, not an automatic gameplay replay |
| `request_conflict` | An accepted command already owns this user/request ID with different actor, branch or payload; do not overwrite its meaning |
| `invalid_request`, `invalid_anchor`, `invalid_annotation`, `invalid_action`, `actor_busy` | Invalid request semantics, anchor/content, action or availability; correct the cause. Gameplay rejection is not automatically a corrupt stream |
| `resource_limit` | Bounded admission/execution/query capacity failed; the affected operation did not commit at that failing boundary. An earlier admitted intention can remain accepted and retryable when execution capacity fails |
| `storage_failure`, `invalid_archive` | Persistence or archive integrity problem; surface it. Do not claim durability, delete the save or substitute an empty game to continue |

Recoverable stream inconsistency is different from a scoped gameplay rejection.
A decoded delta with the wrong exact observation base cannot be partially applied.
The shared client retains its last accepted model and cursor, blocks new requests
and sends **one** snapshot request. Canceled waits retain the same repair request.
Sending, flushing and waiting share a ten-second deadline; a rejected or invalid
matching repair snapshot fails the connection.

While repairing, discard ordered updates and quarantine query payloads. Preserve
known correlated acceptances and rejections. The matching valid snapshot replaces
the model atomically in a newer epoch; it does not retrospectively answer every
pending request. With no correlated result, report that operation's outcome as
uncertain and consult authorized history or an explicit exact command retry.
Do not automatically replay gameplay. `observe` is not the repair primitive:
it publishes a picture without the full reset of control/readiness/lifecycle state.

Normal shared player-client exit requests a save before closing; abrupt socket
loss is not a durability barrier. Disconnect releases control and stops session
travel. A new connection authenticates and attaches again; it cannot inherit an
old lease, page continuation or transport cursor. Unsupported protocol and
save/rules identities are rejected, not negotiated down or silently migrated.
See [reply contexts](protocol.md#reply-contexts-and-errors),
[bounded recovery](protocol.md#bounded-stream-recovery), and
[persistence](protocol.md#history-and-persistence).

## Worked edge cases

### Accepted pickup, lost acknowledgement, changed world

1. An attached controller constructs a pickup with current input context, branch,
   opaque item target and request ID. The host admits it and records its receipt.
   The connection fails before the acknowledgement arrives. Admission may already
   have progressed; silence does not prove rejection.
2. The same user reconnects and attaches to the same authorized actor. The new
   snapshot has a new stream and may show the item gone, work suspended, another
   controller, or a changed branch. None alone resolves the old request's outcome.
3. An explicit retry sends the **original** request ID, original branch and exact
   command, retaining original context. Role/actor authorization still applies.
   If the accepted record survives, receipt lookup precedes fresh stream/control/
   target checks. It returns original admission identity with its current phase;
   it neither needs the old item to remain visible nor admits another pickup.
   The outer reply context describes the new attachment.
4. With no accepted record, there is no receipt hit: the old stream fails fresh
   input validation. Do not silently convert that failure into a new pickup with
   a new ID and current context. A crash can lose acknowledged but unsaved records;
   only an applicable completed durability barrier excludes that possibility.

Changing item or branch while retaining an accepted ID yields `request_conflict`.
A spectator cannot exploit this sequence to recover player receipts, because
role authorization happens before receipt lookup. Absence of a surviving receipt
is not proof that the original operation was never accepted.

### Wrong delta base while an action result is in flight

1. The client has a picture base at sequence 20, then applies control/readiness
   updates through sequence 22. The next picture delta must still name the exact
   picture base at 20, not merely the latest stream cursor at 22.
2. A decoded delta names another snapshot or base revision. Validation rejects
   it entirely without advancing state, map memory or cursor. The connection
   starts one snapshot repair and stops issuing new gameplay inputs.
3. An acknowledgement for a previously sent action arrives during repair. A valid
   same-attachment receipt remains a known acceptance; it is not replayed. Query
   contents and intervening ordered updates do not repair the model themselves.
4. The matching, fully validated reset arrives in a newer epoch. Install it
   atomically, then construct later input from its new readiness boundary. Old
   queued UI input remains stale. Without a correlated reply, that reset alone
   would not establish whether the previously sent action was accepted.

`observe` cannot substitute for repair in step 2: a picture alone lacks the full
reset of ordered authority, intentions and history. Malformed JSON would instead
terminate the transport rather than start this decoded-message repair.

### A remembered-map transfer overlaps a picture and a reset

1. A map query names picture A and starts a bounded immutable transfer. Its first
   page is staged, not partially installed as a finished map.
2. Picture B arrives. A later page may have current reply context for B while
   its inner map basis remains A. Do not reinterpret A's cells or continuation
   as B's data: accept only the exact requested transfer, or cancel the assembly
   and start an appropriate new query.
3. A snapshot reset advances the epoch. The old continuation is invalid even if
   actor, cells and coordinates happen to look identical. Discard the assembly;
   never stitch its prefix to pages from a new transfer.

Remembered appearances, historical chart samples and successful private queries
are therefore not fresh action-target authority.

## Implementation and verification map

- [Protocol DTOs](../../crates/protocol/src/wire.rs),
  [bounded codec](../../crates/protocol/src/codec.rs),
  [delta validation](../../crates/protocol/src/delta.rs), and
  [multipart recovery](../../crates/protocol/src/recovery.rs): exact shapes and
  bounded decoding/assembly, not authorization by themselves.
- [Transport](../../crates/server/src/transport.rs) and
  [outbound ownership](../../crates/server/src/outbound.rs): framing, deadlines,
  aggregate output admission and payload lifetime.
- [Session](../../crates/server/src/session.rs),
  [wire adapter](../../crates/server/src/wire_adapter.rs),
  [wire request bridge](../../crates/server/src/wire_request.rs), and
  [engine](../../crates/server/src/engine.rs): authenticated authority, exact
  retries, query disclosure, publication and persistence boundaries.
- [Shared client connection](../../crates/client-common/src/connection.rs):
  correlation, synchronization quarantine, bounded repair and close/save behavior.
  A new frontend must preserve these boundaries even if it renders or schedules
  pictures differently.

[Documentation validation](../development/testing.md#prose-only-documentation-changes)
checks links and marked wire examples; it does not establish all the ordering,
privacy or retry semantics above. Changes to those contracts require owning
protocol, Service, transport and shared-client behavioral tests, with boundary
witnesses described in the [testing policy](../development/testing.md).
