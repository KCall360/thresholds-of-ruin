# Wire protocol

The `tor-server` executable serves authored dungeon scenarios over JSON WebSockets.
`tor-protocol` defines the wire types without depending on world or simulation
internals. `tor-client-common::ClientState` validates ordered updates and keeps
the current disclosed state plus a bounded recent history. The shared `Connection`
transport applies validated snapshots/updates for the [text client](../play/text.md)
and [graphical ASCII client](../play/ascii.md).
The [headless client](headless.md) uses the same transport and exposes
current state and local last-seen cell memory separately for scripted acceptance.

The examples marked `client-message` and `server-message` are complete envelopes
checked through the real protocol decoder. Placeholder identities illustrate the
schema only; copy live contexts, branches, targets and revisions from the server.
Decoder validity is not authorization or an assertion that a sample can execute
in a live session.

## Reading this reference

- [Connect and control](#connection-and-control): authentication, snapshots, admission and retries
- [Wire integers](#integer-representation) and [entity targets](#entity-target-references): exact client representations
- [Pushed updates](#pushed-updates): ordered state, deltas, repair, readiness and reply contexts
- [Detection and Mapping charts](#detection-and-mapping-charts): private, non-actionable samples
- [Palettes](#asset-palettes), [annotations](#annotations) and [history](#history-and-persistence): separate disclosure channels
- [Wizard operations](#wizard-commands-and-retained-branches) and [creature inspection](#privileged-creature-inspection): privileged scope
- [Gameplay projections](#durable-places): places, items, motion and combat
- [Owner advancement schema](#owner-advancement-schema): bounded DTO integration checkpoint

The [CLI reference](../reference/cli.md) owns launch flags and credentials. The
[headless guide](headless.md) gives a JSON-lines driver over this protocol.

## Owner advancement schema

Owner advancement uses exact purchase, replacement and future-growth-source
operands, an actor/branch/stream/reset/build-epoch basis and correlated read-only
pages. Source selection is `choose_growth_source` with `basis` and `source`;
its preview proposal is `growth_source` with the same source. A source is either
`{ "kind": "racial" }` or `{ "kind": "class", "class": "warrior" }` (using a
validated class ID). It accepts no client XP, seed, threat, price or build.

Overview contains a required `growth` record: `highest_earned_tier`, `peak_threat`,
`held_xp`, `earned_xp`, `spent_xp`, `next_threshold` and `future`. Counts,
threat, XP prices and counters are unsigned decimal strings and satisfy earned = spent + held. Future policy
is explicitly tagged `disabled`, `selected` (with `source`), or `authored`.
Overview also has a required `locked` boolean. It is true when the scenario
disables owner advancement: the build grows only along its authored schedule,
`ready` is false, no respec is in progress, and the server refuses every
purchase, source, Intellect and respec request. These are owner-only descriptive facts. Source changes use the same retained
preview, ordinary admission, strict save adapters and receipt precedence as
purchases; they preserve authored restoration choices. Zero-based owners are
canonical unsigned decimal strings below the 64-bit maximum, so the corresponding
one-based ordinal is representable. Per-owner purchase indices, per-HD grants and
spending, resource amounts and page offsets remain bounded JSON numbers.

An Advancement query has no actor selector. It reads only the attached actor
under its current control lease; spectator attachment, foreign attachment and a
player without control grant no access. Authorized reads remain free while busy
and do not mutate time, randomness, knowledge, reservations or the journal.
Overview starts a current basis. Ledger, balances, catalog and exact proposal
sections carry that basis with expected_revision and the exact page offset.

Every new record uses named fields; nullable fields remain required. Pages have
at most 32 entries and a 32 KiB ceiling over the complete escaped reply envelope,
including context, request ID and query echo. A smaller negotiated response limit
also applies. The host adapts the row count before publishing; an individual row
that cannot fit returns ResourceLimit with no partial page. A nonempty page's
end offset is invalid client input. Empty result sets have offset zero and no
continuation. There is no candidate or page-handle cache: exact preview pages
rerun the same pure preview against current authorized state.

Continuations retain the exact basis, proposal, section and revision. Dependent
removals and gross refunds remain separately and completely traversable. The
conservative wire ledger capacity is 356; two immutable free starting ranks make
the maximum legal purchased ledger 354. Up to 256 HD times 16 grants produce
4,096 balance rows. Source metadata capacity still applies to combinations of
catalog, purchase and requirement bounds; their independent maxima are not a
claim that one package can contain every maximum simultaneously.

Talent catalogs have at most 4,096 root rows. Each row has a required
`form_choices` list of at most sixteen unique `{ id, name }` options; ordinary
talents and skill rows use an empty list. Options are never flattened into
65,536 rows. The exact `talent_form` purchase choice carries both `talent` and
`form` through purchase/replacement preview, confirmation, receipt and saved
replay. An option is an immutable acquisition choice, not a per-cast selector.
Names reject control characters; quote and backslash escaping counts against
the full-frame budget. A singleton exceeding a smaller negotiated budget still
rejects atomically with ResourceLimit.

After structural decoding and account authentication, an exact durable receipt
retry is resolved before fresh control, context or basis checks. It returns the
existing receipt only, without new private state. Changed operands conflict, and
a different authenticated user cannot retrieve that receipt. Fresh reads and
admissions require current actor/branch/stream/reset/build epoch. Previously
accepted ancestor work retains its original admission and lifecycle ancestry
through a retained rewind; it is not rebound to the current branch. Abandoned
future work remains invalid. Reconnect does not rewrite an accepted operation.

Purchase requires an active ready actor without active or paused preparation.
Respec adds the configured safe-place and received-loss quiet-time rules, finite
ration reservation and exact resume identity. An explicit pause retains remaining
actor-active work; fresh Action::Respec never acts as ResumeIntention. Progress
is available in authorized owner pages. Existing acknowledgements and intention
status supply command identity and phase. Private basis, choices, source indices,
replacement operands and refunds have no ordinary history/combat event payload;
the journal retains authoritative operands only for persistence and replay.


## Run locally

In PowerShell, generate a token for this terminal session and start the server:

```powershell
$env:TOR_SERVER_TOKEN = [guid]::NewGuid().ToString('N')
cargo run -p tor-server -- --listen 127.0.0.1:4000 --seed 42 --save saves/game.db
```

Clients need that token. Do not put it in a URL or save it in the repository.
The server prints one JSON readiness line containing its address and protocol
version, never the token. Port 0 chooses an available port for tests or launchers.
The seed only applies when creating a save. Existing saves retain their scenario.

This executable configures a player identity, `local`, authorized for the scenario's
actors. An optional spectator identity is described below. The library accepts
explicit accounts with different tokens, roles, and actor permissions; there is no account-registration API. Loopback binding and native
clients are supported now. Browser origins are rejected. Remote TLS deployment
and account administration remain future work; a secure tunnel can carry the
same protocol to a loopback server.

## Decode boundaries

The shared protocol codec decodes complete client and server envelopes directly
into typed messages. It checks UTF-8 byte limits before deserialization: 16 KiB
for requests and 16 MiB for responses, including the entire JSON envelope and
trailing whitespace. WebSocket limits also apply to the complete assembled
message, so fragmentation cannot bypass the byte ceiling.

Both directions permit at most 64 nested objects/arrays, with the root container
counted as one. An allocation-free scan enforces this bound even inside ignored
fields; quoted delimiters and escaped strings do not contribute to nesting.
Serde still checks syntax, escaping, duplicate/unknown fields according to each
DTO's schema, and trailing content. The scan does not build a JSON value tree or
replace semantic snapshot/delta validation.

Only the current wire shape is accepted. Combat views require nullable `sequence`,
ability events require nullable `strike`, and cancellation commands require
nullable `step`; omitting any of these fields is not equivalent to `null`.
An `anonymous_ability` combat event requires nullable `caster` and `target`
references and an exact `applied`, `unaffected` or `miss` outcome. It has no ability,
item, strike or diagnostic operands; extra fields and positional forms reject.
Its neutral narration does not classify the activity as an attack or infer a source.
Private inspection uses typed natural/weapon selections and authored strike sources.
There is no fallback from older attack, inspection or cancellation shapes.
Actions, finite-supply selections and ability reach/range values require named
JSON objects with their declared tags and fields. Positional tag-first arrays
are rejected by the complete-message decoders.

Malformed wire input terminates that connection. Clients preserve their last
accepted model and reconnect for a fresh snapshot; malformed input does not
start stream repair. Valid decoded messages with a recoverable stream gap follow
the existing bounded repair policy. Request rejection precedes command admission
and publication; an unrelated healthy connection remains usable.

## Read-only spectators

Before starting the server, optionally set `TOR_SPECTATOR_TOKEN` to a separate
random token (for example, another `[guid]::NewGuid().ToString('N')` in PowerShell).
It must differ from `TOR_SERVER_TOKEN` and pass the
[credential byte rules](../reference/cli.md#environment-variables).
Omitting it disables spectator login. Invalid configuration fails before opening
or creating a save. Keep both credentials out of URLs, arguments, and source control.

In a spectator's client terminal, set **`TOR_SERVER_TOKEN` to the spectator token**,
then launch either frontend normally. No `--observe` flag is needed. The server
assigns this credential the `spectator` role and identity, authorized for the
scenario's actors. Only share this credential with someone who should watch;
the player credential continues to allow control and annotations.

Spectators may attach, request snapshots, palettes, explored-map pages and private
charts for their authorized attached actor, and browse permitted
current or abandoned-branch history. All other
requests are rejected with `Unauthorized`, before receipt lookup or mutation,
even when an actor has no controller. Changing frontend labels, retrying an
accepted command, or sending protocol requests outside the official clients does
not elevate access. The role is fixed for the authenticated connection. New
request variants are denied to spectators unless explicitly added to the read
allowlist after review; future wizard mutations must remain denied.

An attached spectator receives every accepted action and result for their actor,
plus the same disclosed state that a controlling client receives. This includes
pickup, movement, and wait; rejected commands and local UI inputs are not gameplay
history. Combat and autonomous actors publish actor-specific health, progress,
qualitative injuries, and perceived combat events through the same stream. This is an
actor-perspective view: hidden rooms and other actors' private commands and
numerical combat attributes are not exposed. Spectators cannot write even private annotations.

Annotation privacy still follows authenticated identity and audience. The built-in
`spectator` user sees actor-visible notes, not `local`'s private notes. The library
can configure read-only and player credentials for the same user; those credentials
share private-note visibility but have independent write authority. Actor allowlists
apply to both roles. The existing `--observe` option merely skips a player client's
initial control request and is not an access restriction.

A paid public-plan observation belongs to this same actor perspective. A player
or read-only spectator authorized to attach that actor receives the actor's
retained `combat.public_plan`, just as they receive its own statistics. Control
ownership is not an additional read permission. An account not authorized for
that actor cannot attach to retrieve the record. Records are never copied into
visible-actor rows or the combat summaries of other actors.

The protocol requires a backend-resolved observer-relative scene. Positions
are x/y/z offsets, with the actor at the origin. Each `visible_cells` entry has an
opaque `key`, `position`, `wall`, `stairs_up`, `stairs_down`, `place_hint`, and
required boolean `lit`, plus nullable `door` and required nullable `trap` facts. `lit` is current server
illumination: a cell disclosed through local body awareness can be dark. It
does not add cells to the scene or confer sight targeting. Remembered values
remain last-disclosed information until a fresh observation replaces them.
Cells also carry terrain `material` (empty for carved voids). Floors and
ceilings are ordinary seen solid cells, not separate facts; items include `description` and a
`reachable` flag, and actors carry perceived `name` and `description`. These
appearances are described in [the text interface](../play/text.md). The client receives no region IDs, bounds, names, portal links,
transforms, or visited-region list. Move events report the chosen direction.
The role in `welcome` and permanent wizard marker remain required. Old clients
must upgrade. Only the current save format and ruleset are accepted. See [geometry](../architecture/geometry.md).
Roles and credentials are startup/session configuration, never journaled.
Restarting requires supplying the desired credentials again.

## Integer representation

All 64-bit wire values use canonical decimal strings: authorized actor attachment
identities, quantities, ticks, revisions, epochs, stream sequences, durations
and completed travel steps; aggregate Health/injury, HD counts and owners,
aggregate advancement points, threat and XP prices. Unsigned values range from `"0"` through
`"18446744073709551615"`. Signed motion velocity components range from
`"-9223372036854775808"` through `"9223372036854775807"`. Numeric JSON tokens,
leading zeros, plus signs, negative zero, whitespace and exponent notation are
rejected for these fields. Optional quantities retain missing/null semantics.

Protocol versions, bounded collection counts, and 32-bit observer-relative
position components remain JSON numbers. Keep wire strings intact when returning
identities or contexts in requests; use exact integer arithmetic for comparisons
or calculations. This preserves values above JavaScript's exact Number range.
Persistence owns its numeric schemas and version identities independently;
canonical transport strings are not a save-schema alias.

## Entity target references

Disclosed actors, items, doors and traps use opaque typed references: `a_`, `i_`,
`d_` or `t_` followed by exactly 64 lowercase hexadecimal digits. Numeric IDs, uppercase
hexadecimal, extra whitespace and references of another entity kind are rejected.
Clients copy the reference from the current observation into an action; they
cannot derive it from an authored ID or infer allocation order from it.

A reference identifies an entity within one save and observing actor. It stays
stable through movement, inventory changes, restart and rewind in that scope.
Other saves and observers have different references. Multiple portal-relative
occurrences can share one entity reference while keeping distinct disclosed
positions. These references do not expose region identities or a global frame.

`observation.actor` remains the authorized actor attachment identity.
`observation.self_target` identifies that actor's body among disclosed entity
references, including repeated portal views. Delta observations require the
same self target as their base; a different target scope requires a snapshot.

Creature appearance uses a separate opaque `p_` token followed by 64 lowercase
hexadecimal digits. An actor's optional `appearance_seed` is shared across
observers, sessions and reloads within one save. `self_appearance` supplies the
observer's own `{seed, asset?}` even when its body is outside the eye scene.
These cosmetic tokens cannot be used as interaction targets and are derived
without simulation randomness. An absent appearance means a generic client
fallback, never permission to infer an identity from display text.

An identified corpse item can disclose `corpse: {seed, asset?}`, captured from
the creature at death. Its ordinary `asset` still describes the item itself.
Clients use the captured creature data to preserve the body's appearance;
item names and arbitrary authored properties do not identify its source. Other
item classes and unidentified items cannot disclose corpse metadata. Full views
and deltas carry the same appearance facts.

A disclosed creature's actor entries, and `self_appearance`, list what it visibly
wears and holds as `worn: [{slot, occurrence?, class, appearance, asset?}]`. That
is at most 64 entries in canonical slot order, and every cell of one body repeats
the same list. Each entry names the rules slot, which of that body's sockets of
the slot it fills (0 when omitted), and the item's class, unidentified appearance
and asset: no more than the item shows on the ground. Only weapons and armour
are listed. Rings, amulets and other classes never are, and neither is any
private fact such as enchantment, curse or true name. Anonymous contacts,
guises and hallucinated figures carry no list, and corpse appearances are always empty. An absent or empty
list means nothing is disclosed, not that the body is bare.

A discovered trap discloses exactly `id`, required nullable `family` (`rust` or
`sleeping_gas`), and `reachable`. The record never includes a discovery DC,
effect operands, hidden-feature counts or undiscovered neighbors. Presence and
family are separate actor-owned knowledge. Remembered cells retain only an
optional `{family}` appearance, never a trap action reference.

`{"type":"examine_trap","trap":"<t_ token>"}` is a normal admitted action.
Execution requires a discovered physically adjacent trap and no pending work;
it learns the family without advancing time. Snapshots, map hydration and client
lookups never mutate knowledge. Unknown closed secret entrances disclose wall
appearance and no door reference or approaches. Discovered entrances use the
existing door action; opening changes shared physical geometry.

References grant no authority. After authority and original receipt lookup,
a fresh action resolves only against the cached observer disclosure: attack and ability use
uses visible actors or self, doors and trap examination use visible cells, take uses ground items,
and drop uses inventory. The simulation still checks the admitted target when
executing the queued action. Unavailable references produce the same generic
rejection without revealing hidden entities.

A duplicate request compares against references reconstructed from its saved
backend command and original privacy scope before consulting current targets.
A completed pickup can therefore return its original admission receipt even
when the item is no longer on the ground. Reusing that request ID with different
facts produces a conflict. Saves retain private numeric target facts independently
of this wire representation.

## Connection and control

The first frame authenticates and declares a frontend label:

```json client-message
{"type":"hello","protocol":65,"token":"<session token>","frontend":"text"}
```

The server sends `welcome` with the authenticated user, authorized actor IDs, and
server-granted `role` (`player`, `spectator`, or `wizard`), and required typed
`capabilities`. These state `max_request_bytes`, `max_response_bytes`,
`max_retained_state_bytes`, `max_snapshot_bytes`, `max_connections` and `max_history_page_entries` as
bounded numeric counts. Clients validate capabilities before attaching and obey
the advertised request ceiling. The response ceiling follows the configured
frame limit; the connection ceiling is min(128, total/frame), 16 by default.
Existing borrowing may exhaust admission before that ceiling. Capacity rejection
uses `resource_limit`, distinct from `invalid_request`, and consumes no client ID.
Capabilities describe static limits, not available capacity or actor authority.
The retained state has a separate 16 MiB ceiling. Complete recovery snapshots,
including history and authority metadata, have an advertised 32 MiB ceiling.
When one response frame cannot hold a snapshot, ordered `snapshot_part` messages
carry its complete encoded envelope. Each part names the request and reset
context, its UTF-8 byte offset and the total logical byte length. Frame admission
checks each complete encoded part, including JSON escaping and header bytes.
Preparation stops at the client's remaining encoded-byte quota before allocating
another part; queue admission rechecks the complete transfer atomically.
The whole transfer occupies one ordered output queue entry and requires its
complete encoded byte budget before publication. Resource exhaustion rejects the
whole transfer and closes that connection; parts cannot bypass output quotas.
One write deadline covers the entire transfer, so individual parts cannot extend
the time a slow reader holds its memory lease.
Clients assemble within the advertised logical ceiling, reject gaps, repeats,
identity changes and interleaved responses, and validate the complete snapshot
before replacing state or enabling input. Assembly survives canceled reads;
partial contents never become client observations or authority.
The server rejects bad tokens, unsupported versions, and unknown request fields before
disclosing game state. Attach once per connection:

```json client-message
{"type":"request","request_id":"attach-1","request":{"type":"attach","actor":"1"}}
```

The response is a snapshot containing the actor's observation, action revision,
branch ID, stream cursor, control status, and recent visible history. A client can
observe without controlling the actor. `acquire_control` and `release_control`
transfer exclusive control; acquisition fails while another connection controls
the actor. Disconnecting releases control. Switching actors requires reconnecting.

An action uses the branch and revision from the latest observation:

```json client-message
{
  "type":"request",
  "request_id":"move-1",
  "request":{
    "type":"command",
    "context":{"stream":{"stream":"<attachment stream>","epoch":"1"},"readiness_revision":"1"},
    "branch":"<branch from snapshot>",
    "command":{"type":"act","expected_revision":"0","action":{"type":"move","direction":"east"}}
  }
}
```

Gameplay acknowledgements report admission, before simulation execution:

```json server-message
{"type":"ack","context":{"input":{"stream":{"stream":"<attachment>","epoch":"1"},"readiness_revision":"2"},"actor":"1","branch":"<current branch>","cursor":{"sequence":"3","tick":"0"},"revision":"0"},"request_id":"move-1","receipt":{"type":"admitted","actor":"1","branch":"<request branch>","intention":"<opaque intention>","entry_id":"<admission record>","phase":"queued"}}
```

The simulation chooses when the actor's intention executes. Observations disclose
its effects; ordered `intention` updates report lifecycle changes. Snapshots include
active `intentions`, separately from observation readiness. Each actor can hold
one queued or suspended intention, and the simulation bounds the global queue
at 4,096 entries. New admission requires that actor to have no queued intention
and the global queue to have capacity. An attack's `started` phase retains its preparation
identity through preparation and every strike in an ordered sequence. A resolved
prefix does not complete that intention; completion follows explicit terminal
sequence evidence. `paused` retains inactive preparation with its completed prefix,
remaining progress and original target; it can coexist with a separate queued action. Changed observations precede their lifecycle updates.
An execution-time simulation capacity failure leaves admitted work retryable and
reports a generic `resource_limit` error to its attached controller, with attached
scope and no request ID. Repeated checks of the same blocked work do not repeat
that notice. It discloses no private XP balance and is separate from save warnings.
Immediate operations acknowledge with a required
`receipt { type: "immediate", actor: ..., branch: ..., entry_id: ... }`.
Both receipt variants identify the original actor and branch. A committed
command's immediate receipt takes that identity from its journal entry; retrying
it after rewind or restart returns the same receipt, even when the live branch
has changed. Session operations without journal entries use the attached actor
and branch at completion and have a null `entry_id`. Receipt identity does not
claim that the client is currently synchronized or ready to act.
An admitted receipt retains its actor, branch, intention and admission-entry
identity on retry, while its phase reflects that intention's current lifecycle.
For example, retrying a queued admission after interruption reports `paused`
without admitting or executing another action.

Restart and controller loss suspend queued human intentions and pause running
preparation. Rewind restores the selected work and preparation; Session then
suspends human work until fresh input. Acquiring control preserves that state.
Explicit `resume_intention` and `cancel_intention` commands reference the original
opaque identity using the current branch and observation revision. The following
is a request-body fragment, to be wrapped in a `request` envelope with a unique ID:

```json
{"type":"command","context":{"stream":{"stream":"<attachment stream>","epoch":"1"},"readiness_revision":"4"},"branch":"<current branch>","command":{"type":"resume_intention","expected_revision":"7","intention":"<original admission identity>"}}
```

Resume returns suspended work or paused preparation to the simulation queue under
its existing identity. Continued preparation keeps its remaining progress and
revalidates the original target at execution. Cancellation requires that identity
in the current disclosed `readiness.cancel` permissions and an explicit `step`:

```json
{"type":"cancel_intention","expected_revision":"7","intention":"<original admission identity>","step":null}
```

`step: null` cancels in place. A direction such as `"west"` requests one atomic
cancel-and-move operation for that preparation. Cancellation can discard pending
work or eligible combat preparation, but item preparation rejects cancellation once
started, including while paused. Cancelling one identity preserves independently
queued work. In-place cancellation does not advance simulation time; after a
completed prefix it retains the disclosed outstanding sequence recovery debt.
A successful step adds ordinary movement cost to that debt and applies ordinary
movement physics. A blocked or otherwise invalid step rejects the entire command:
no cancellation, movement, cost, queue change, random draw or journal entry.
The step is part of the exact accepted receipt payload, so a retry cannot change it.
Both operations require controller authority. Their ordered lifecycle updates
precede the acknowledgement. ASCII exposes F8/F9; headless exposes explicit
`resume_intention`/`cancel_intention` inputs using the current disclosed context.
These default controls select queued work before independent preparation,
regardless of snapshot or update delivery order. The ASCII hint uses that same
selection; queued work must be suspended before it can be resumed. Explicit
protocol commands can reference either disclosed identity only when its relevant
resume/cancel permission is present. Escape-aware directional controls instead
select the exact active or paused preparation identity, never unrelated queued work.
They require disclosed own sequence progress with `completed > 0` and cancellation
permission; the server remains authoritative for the requested movement.

### Linked holds and Escape

Every current observation and delta requires `linked_holds`, an ordered list of
at most 128 own incoming live offers. Deltas replace that list in full. An offer
contains exactly `target` (an opaque `h_` hold token) and `holder` (a required
nullable actor target). `holder` is non-null only when the actor is already in
the ordinary visible-actor list. This list is separate from timed personal
conditions; it exposes neither protection records nor a hidden holder's location.

The action `{"type":"escape","hold":"<opaque hold token>"}` selects exactly one
current own offer under the ordinary action context, branch and revision gates.
A hold token cannot inspect an actor, reveal an endpoint or authorize another
observer. It remains stable when its holder enters or leaves ordinary sight.
A new ordinary human `move` action while held admits that same paid Escape
against the oldest active hold (the smallest monotonic internal hold identity).
Selection happens after ordinary receipt/context validation. The original Move
request remains the idempotent receipt; durable admitted work binds the exact
selected hold. No movement occurs, and success does not queue or retry the
requested step. Explicit Escape still selects another offer when desired.
Existing queued/preparing work keeps its normal busy/admission rules; travel,
autonomous decisions and directional cancellation are not converted.

Pending Escape discloses only that action and ordinary preparation progress;
its internal opposed check is not a public ability plan. History uses
`escape_started` with `hold`; completion uses `escape_resolved` with `hold` and
`success`. Neither record adds a holder name, actor target or check statistics.

Accepted exact receipt retries are resolved before current offer availability,
including after the chosen hold is released. Durable actions store the internal
hold identity, not a wire token. A stale or foreign token never selects a new
link. The separate directional cancellation described above retains its existing
meaning; it is not this Escape action.

Requests require unique IDs per authenticated user for accepted actions and
annotations. Retry the exact same command and ID to recover its original receipt,
including after reconnect or restart. Reusing an accepted ID for different
content is an error. Requests rejected before admission are not committed. An
execution-time failure consumes the accepted intention and records its failure,
without substituting another action. A duplicate successful
command from a player account is acknowledged without applying or broadcasting
it again, even after control has moved to another client. Spectator accounts
cannot submit commands, including receipt retries.

## Pushed updates

Clients receive `update` messages without polling:

| Update body | Meaning |
| --- | --- |
| `observation` | New disclosed state, its revision, and an optional actor action/event entry |
| `observation_delta` | The same as `observation`, with cells and ordered collections sent as changes to the previous state on this stream |
| `annotation` | A visible note was committed; game state is unchanged |
| `travel` | Travel status and optional accepted-request history entry; no future route |
| `control` | This connection gained or lost control |
| `readiness` | Authoritative admission and opaque resume/cancel permissions; its counter advances independently of observation revisions |
| `intention` | An opaque intention's lifecycle in this actor/branch context; no authoritative topology or queued action targets |

When play stops for input, the server sends `waiting { on }` after the updates
of that run, and again after a snapshot: `you` (this client controls the actor
that's next), `others` (another client's actor is next), `unclaimed` (a
character no client controls is next), `paused` (AI play waits for a
controller's action or `continue`) or `stopped` (nothing can act). It's sent
once per stop, and again after every answered request, so a client can end its
turn on it instead of waiting for play to go quiet.

Every update has actor and branch identities, a connection-scoped sequence, and
simulation tick. Multiple updates can share a tick. The stream sequence increments
for every delivered update; the action revision increments only when that actor's
disclosed observation changes. A private note neither advances another user's
sequence nor invalidates anyone's pending action revision.

Snapshots and updates require `context { stream, epoch }`. The host creates an
opaque `stream` for each attachment outside simulation randomness and saved
state. Each snapshot reset advances its checked `epoch` while retaining that
attachment identity. Clients reject updates outside their current context and
reject replacement snapshots from another attachment or an equal/older epoch
before changing presentation state. A new connection establishes a fresh model.
An exhausted reset counter closes the attachment rather than wrapping.

### View deltas

Most observation updates are `observation_delta`s. Scalars and optional combat
and motion fields are carried in full. `visible_cells` is replaced with `cells`:

- `shift` is added to the position of every cell in the previous state. Cell
  positions are observer-relative, so a step moves every retained cell.
- `removed` lists positions, after the shift, of cells no longer in view.
- `changed` lists complete cells that entered view or differ from the shifted
  cell at the same position.

The `inventory`, `ground_items`, `visible_actors` and `places` fields contain
ordered edit lists. Each edit is `{ start, remove, insert }`: `start` is an
unsigned 32-bit index into the original base collection, `remove` is the number
of original entries to remove, and `insert` contains complete replacement values.
An empty list retains the collection. Edits must have increasing distinct starts,
nonoverlapping in-bounds ranges and at least one removal or insertion. All ranges
refer to the original base, so applying an earlier edit does not shift later
indices. Result order exactly matches the next full observation, even when the
full collection is unordered. Unknown edit fields are rejected.

Ground items and visible actors are separate occurrences keyed by their relative
position and underlying identity; the same entity seen through multiple portals
remains distinct. Places use disclosed opaque keys and inventory uses item IDs.
These edits expose no private region coordinates or topology. Cell shifts do not
translate non-cell collections; changed projections are represented explicitly.

Historical disclosed aim records admit at most 61,619 distinct occurrences. This
bound covers every valid body/eye offset expanded by the capped physical sensory
range, plus the synthetic stair pair; portal images retain their observer-frame
offsets. It is a conservative geometry bound, not permission to exceed the
aggregate retained-state or complete-response byte limits below.

### Retained state and delta bases

A stream's retained full `StateView` is limited to 16 MiB of canonical encoded
JSON, independently of the complete-response limit. Shared semantic validation
checks this bound after snapshot or delta reconstruction; a fitting delta cannot
accumulate unlimited retained strings or entries. The host also rejects an
observation exceeding this bound before choosing its delta. A lower configured
frame ceiling may still admit a fitting delta for a retained state within the
protocol ceiling. Counting uses the same serializer with a constant-space byte
sink, without allocating another full JSON buffer. This bound covers the current
observation; remembered map cells and history have separate retention policies.

An `observation_delta` body requires `base { cursor, revision }`, identifying
the last observation within its enclosing context and branch. Its embedded
state delta also names that state's `base_revision`. Both must match: matching
a revision alone does not establish the correct base. Control, annotation,
travel and intention messages consume stream sequence numbers without changing
this observation base. A snapshot reset establishes a new base at its own cursor.

### Bounded stream recovery

Applying a valid delta yields a full observation with cells sorted by position.
`tor-client-common` validates the context, ordering, exact base and reconstructed
state before publishing changes; rejection leaves state, memory and cursors
unchanged. The shared connection requests one fresh snapshot after an invalid
update or snapshot. It retains that request across canceled waits, discards
old stream updates until the matching reset arrives, and blocks new requests
while unsynchronized. Its ten-second deadline includes sending, flushing and
waiting. A rejected or invalid matching recovery reply fails the connection.

Replies to previously sent requests retain their original identities. A recovery
snapshot without a correlated reply leaves the outcome uncertain, so clients
stop the affected input chain and direct the user to history; they never replay
gameplay automatically.
Normal authoritative resets remain distinct from internally requested recovery.
Queued native input carries its originating snapshot context and is refused after
a reset. Confirmed receipts and rejections are retained while recovery waits for
its snapshot. History, palette and creature-inspection contents are discarded while the stream is
uncertain, including messages whose headers still match the old state.
A known acceptance or rejection remains known after repair; a reset alone does
not prove request acceptance or gameplay completion. Clients distinguish an
unknown request outcome only when no correlated reply was received.
### Readiness and input freshness

Snapshots include required `readiness { revision, admission, resume, cancel }`.
`admission` describes available queue capacity for new gameplay work under current
control and run state; target validation and simulation timing still apply.
`resume` and `cancel` contain only disclosed opaque intention IDs. The simulation
owns their availability checks; the session applies authority and travel policy.
Permission changes arrive as ordered `readiness` updates after their causing
observation, control or lifecycle changes, and before the request acknowledgement.
Each update advances the readiness counter by exactly one without advancing the
observation base or simulation time. Counter exhaustion disconnects the client.
Clients validate permission structure and disclosed identities atomically.
An observation or terminal journey status can arrive before its following permission
update. Gameplay completion must consume that boundary before building a subsequent
command; an observation's `ready` flag alone does not authorize fresh input.
A known execution failure remains rejected if later permission delivery times out.

Every `command` requires `context { stream: { stream, epoch }, readiness_revision }`.
Build it from the current disclosed attachment and readiness generation. The host
checks authenticated role/actor access and resolves an existing receipt before
freshness validation. Fresh commands must match both the last published generation
and current permissions; old streams, reset epochs, ownership generations and
unpublished predicted generations receive `StaleContext` before admission.
A current generation does not authorize disabled gameplay: new action/travel
requests require `admission`, and resume/cancel requests require the intention ID
in the corresponding permission list. The host rejects disabled admission with
`ActorBusy` and unavailable recovery with `InvalidAction`, before any mutation.
Simulation still validates targets, revisions and execution independently.
Control changes advance this generation even if all permission vectors stay empty.
Capture context when constructing input; never restamp queued or retried requests.
An authorized retry returns its original receipt even after context abandonment.

### Reply contexts and errors

Acknowledgements, history replies and palette messages require a current reply
`context { input, actor, branch, cursor, revision }`. `input` has the same stream,
epoch and readiness-generation shape as command input. This context names the
client's last disclosed boundary at publication, after ordered permission changes;
it does not advance the stream. The receipt still names the original operation,
so a retry after rewind or reconnect can have a different branch or attachment in
its current context. Context is not persisted with the receipt.

Every error requires `scope`. `{"type":"transport"}` marks a transport failure
with no host ordering boundary, including handshake failures and malformed wire
input. `{"type":"unattached"}` marks a host error before a snapshot establishes
an attachment. `{"type":"attached","context":...}` carries the same disclosed
reply context as successful responses. Scope fields are strict and have no
implicit defaults. An attached client treats transport failures as connection
errors and rejects an unattached host scope; neither can confirm a pending host
operation. Scoped host rejections remain known through bounded stream repair.

The shared connection validates successful reply contexts against its exact
currently disclosed boundary. A mismatch within this attachment starts one
bounded snapshot repair. The reply does not install missing state or permissions;
receipts remain known while repair runs, and query payloads are quarantined.
Another attachment or actor is a connection error even during recovery.
Disconnect still requires a new attachment. The server compares complete encoded
`ServerMessage` envelopes, including context, cursor and event, and selects a
delta only when it is strictly smaller than the full response. Equal sizes prefer
a full `observation`. Size counting precedes allocation of the selected text;
only that selected text is encoded for outbound admission. A full response that
exceeds the response ceiling may use a fitting delta. If neither representation
fits, the connection closes without publishing a partial response.

### Publication and transport limits

Snapshots are always complete; reconnects and rewinds start from one, so a delta
never skips a state. Sequence, observation tick, reset context and disclosure base
advance only after the response enters the ordered output queue. The server then
retains the owned full state as the next base without copying it again. Output
rejection closes that stream rather than retaining an unpublished base.

Readiness is part of that disclosed state. A same-tick turn handoff advances the
revisions of the actors whose readiness changes, even when their geometry and
tick remain unchanged.

An action's history timestamp is when it took effect; the accompanying observation
reflects the next decision time. One ordered attack intention can produce multiple
primary strike results and intermediate observations before its terminal lifecycle
update. The transport preserves those boundaries between user decisions. Other actors can receive changed observations
without receiving the acting actor's private command details. Richer cross-actor
event descriptions still belong to the perception work.

### Ability projection

Ability commands use the `use_ability` action with an opaque actor target and
a validated content ID disclosed by the current ability catalog. Stock examples
include `rattling_blow`, `spark_lance_fire` and `falter`; this is not a closed wire enum.
The ability view supplies its ID, name, costs, reach and target kind. Basic melee retains the `attack`
action. Admission remains free; execution checks the current grant, target,
reach and resources before reserving preparation costs. An `ability_started`
history event identifies the technique and scoped target. Combat `ability`
events disclose an optional caster/target, the qualitative outcome `applied`,
`unaffected`, or `miss`, and required nullable `strike`. A non-null strike is the
zero-based ordinal of the primary strike that actually resolved. It is not a future
strike count. Single/first-strike narration keeps its existing wording; later visible
results, including misses after a target moves away, identify the later strike.
Rider and environmental resolutions do not invent an additional primary ordinal.
Hidden participants remain absent, and resistance and immunity share `unaffected`;
these events expose no rolls, damage totals, enemy resource balances, future sequence
lengths, plans, check operands or token claims. Current scope and remaining acceptance work are recorded
in [project status](../status.md).

Snapshot generation, persistence, control changes, and update publication are
serialized. Queries can request a fresh `snapshot` at any point. Reconnecting
starts a new sequence at zero and always provides a fresh snapshot; resuming an
old transport sequence is not implemented. Durable history IDs remain unchanged.

Each connection has a 256-message output queue. A client that cannot keep up is
disconnected and releases control instead of silently missing updates. It must
reconnect and rebuild from a snapshot. Handshakes and socket writes have deadlines;
client messages are capped at 16 KiB and server messages at 16 MiB, including
JSON envelopes, UTF-8 bytes and escaping. Both limits apply to complete messages
assembled from WebSocket fragments. Shared clients bound request encoding before
sending and reject oversized responses before JSON decoding. A locally oversized
request leaves the connection usable; oversized inbound traffic ends the transport
without starting snapshot repair. Host output limits may be lower than the protocol
ceiling. There are at most 128 connections.

### Output ownership and backpressure

Host defaults are 16 MiB per frame, 64 MiB per connection and 256 MiB aggregate.
[CLI limits](cli.md) require positive frame ≤ client ≤ total, with frame no larger
than the protocol ceiling; invalid limits fail before opening a save. The queue
retains one encoded representation, not a DTO to encode again. Encoding stops at
the frame ceiling, with at most one bounded temporary frame prepared before
admission. Queued and in-flight payloads retain their byte leases until flush or
socket destruction, including failed writes, close draining, timeouts and task
cancellation. Payload limits do not measure resident memory: cached observations,
message preparation, allocator overhead and socket framing/copies are separate.

- Play pauses when any attached client's queue has fewer than 16 free slots or
  less than one maximum frame's byte headroom. The simulation thread continues
  servicing its mailbox while waiting.
- Five seconds of sustained fullness disconnects that client. Socket writes also
  have a five-second deadline, so a slow spectator can delay but not indefinitely
  stop a journey.
- Resource failures and cancelled transport tasks abort upgraded TCP connections,
  discarding stale OS send-buffer bytes as well as WebSocket output. Releasing a
  descriptor through normal TCP close alone may keep draining old data. An
  intentional detach instead drains its final explanation under one shared
  five-second deadline. Successful authentication rejection and normal protocol
  closure preserve their output. In-flight leases remain owned until socket and
  write-buffer destruction, including reset and cancellation.
- Slot/byte exhaustion or frame-encoding failure during request handling closes
  that stream; it must reconnect, never silently miss an update. The shared pool
  limits admission rather than stalling an unrelated actor's simulation. Default
  per-client limits stop one stream retaining the whole pool, but aggregate
  pressure can still reject another stream. This is not a fair-allocation promise.
- Updates remain one per simulation step; deltas reduce size without losing the
  boundaries clients need for presentation.

A client whose attached actor leaves the loaded world (see
[region streaming](../architecture/streaming.md#engine-streaming); a spectator watching
an AI actor, say) receives an unsolicited `error` with code `not_attached`
and is disconnected. It may attach again once the actor is back in play.
Messages already queued for a client, such as that error, are delivered
before the connection closes.

## Explored-map pages

`map` is an attached, read-only request available to players and spectators.
Its query binds the stream/epoch, branch, exact observation base and inclusive
coordinate window. It cannot select a different actor. Initial requests have a
null cursor; later pages echo the opaque transfer, next cell ordinal and page.
The response uses the ordinary correlated reply context and appearance-only
`map_page` cells. There are no actor occupants, live targets, reach flags or
inferred action capabilities in remembered cells.

A page contains at most 512 cells and 128 KiB including its escaped correlated
envelope, or the host's smaller frame limit. A transfer allows at most 128 pages,
12,474 positions and 8 MiB total response bytes. `complete` certifies the bounded
projection; a null `next` separately marks transfer completion. `ambiguous` can
accompany surviving certified cells. A work-limited response is an empty terminal
page with `limited=true` and `complete=false`; an oversized appearance receives
an explicit resource error. No partial transfer replaces the prior chart.

The Service advances its continuation only after output admission. Stream,
branch, observation or knowledge-root changes invalidate it. Clients cancel old
assemblies on new observations and atomically install a correlated whole transfer,
then overlay current disclosure. Automatic hydration is independent of gameplay
readiness and does not issue actions, learn cells, or load distant world regions.

A subwindow clips the returned appearances, not their anchoring domain: the
server traverses saved witnessed geometry within the normal viewport and its
existing bounded halo, using current sight only as anchors. A thin off-sight
edge has the same certified appearances as that portion of the full projection
when both outputs fit their appearance limits. Appearance limits apply to the
requested output window. The same traversal/work limits apply; requesting a narrow window
cannot walk arbitrary historical knowledge or read unseen live world state.
Clients can retain certified coverage and request only uncovered face regions.
Each region remains an atomic context-bound transfer; this changes no wire or
save format.

## Detection and Mapping charts

Charts are a separate private disclosure channel for the attached actor, readable
by authorized players and spectators without a control lease. They are not
ordinary sight, explored-map memory or actionable targets. Queries have no actor
selector and cannot read another actor's chart through a guessed token.

Full observations require `charts`; every observation delta carries its complete
replacement list, not collection edits. At most four unique summaries appear in
kind order: `monsters`, `food`, `magic`, `mapping`. Each summary requires `kind`,
`supported`, `mode`, `status`, and the explicitly nullable `acquired_at`, `chart`
and `latest_attempt`. Modes are `live` and `historical`. Statuses are:

- `inactive`: no retained sample; acquisition tick and token are null.
- `empty`: a complete admitted sample with no occurrence rows; tick and token
  are present, and its component declarations remain pageable.
- `available`: a complete admitted nonempty sample, with tick and token.
- `unavailable`: no deliverable sample token; this must not be shown as empty.

Active Monster Detection describes the current committed state, not a retained
last-seen monster list. Food, Magic and Mapping show historical acquisitions.
A later empty or unavailable attempt does not erase their latest positive
sample: `latest_attempt { attempted_at, status }` separately reports `empty`,
`positive` or `unavailable`, even when no prior sample exists. `supported` is
current preparation support, not sample freshness or permission to act. Mapping
requires a certified current scope; a retained historical sample can remain
available while `supported=false`. Missing optional source certification is not
package rejection and does not permit fabricated paid Mapping.

`Request::Chart { query }` has wire type `chart`. Its strict `ChartQuery` requires
`context { stream, epoch }`, `branch`, exact observation `base { cursor, revision }`,
the advertised `chart` token and an explicitly nullable `cursor`. Begin with
null; thereafter echo exactly the previous page's nonzero `next` ordinal. This
base is the last observation base, not the latest readiness or unrelated stream
update cursor. The attachment actor, stream/reset epoch, current branch,
observation base, advertised token and continuation must all still match.
Reattach, rewind, reset or a changed observation invalidates the old transfer.
There is no arbitrary past-Game selector: rewind establishes a new current
branch, while retained acquisitions are explicitly historical.

`ServerMessage::ChartPage` has wire type `chart_page` and the ordinary correlated
`context` and `request_id`. Its `page` requires `context`, `branch`, `base`,
`chart`, `kind`, `mode`, `acquired_at`, `total_components`, `total_records`, `start`,
`records` and nullable `next`. Validate both outer reply authority and every
inner header, including the exact request ID. Ticks, counts and ordinals use
unsigned decimal strings; projected coordinate triples use signed decimal
strings. All nullable fields remain present and unknown fields are rejected.

Records begin with the complete ordered `component` declaration prefix, then
nondecreasing `row` records. Components disclose only an ordinal and `aligned`
or `inconsistent` alignment. Each row has a component, nullable projected
position and mark. An inconsistent component has no claimed position; equal
projected occurrences are retained, not deduplicated. Monster marks carry only
the disclosed name and nullable asset. Food and Magic marks are generic presence,
not item identities, stack quantities or inventory counts. Mapping marks contain
only `wall_face` (nullable oriented normal), `passage`, `doorway`, `stairs`,
`secret_door` or `trap_presence`. A wall normal shares the projected component
frame and is null when unaligned. No source region IDs, actor/item handles,
private transforms, stair destinations or action capabilities are exposed.

Canonical admission checks the entire candidate and all its page costs before
publishing a sample, never a successful prefix. Each page allows at most 512
records and 128 KiB including its escaped correlated envelope; the complete
transfer has a 64 MiB ceiling. A smaller host frame limit also applies and can
return `ResourceLimit`, not a truncated success. The host reuses the committed
revision's immutable prepared chart and advances a continuation only after output
admission. Page queries neither recensus the floor nor advance time, sample RNG,
learn map cells or execute actions. Clients validate declarations, ordering,
headers, totals and final completion before atomically installing the whole
chart; partial or stale assemblies never replace a complete display. Text and
ASCII present this separate channel without granting movement, targeting or
ordinary-map knowledge from its rows.

## Asset palettes

A scenario can name assets: dotted lowercase identifiers such as
`creature.rat` (see [scenario packages](scenario-packages.md#assets)). Two
things then reach clients:

- **Assets on disclosed things.** Cells, doors, items and actors in an
  observation carry an optional `asset`. It's only ever attached to something
  already disclosed, and a concealed item carries the asset shared by
  everything that looks like it, never its archetype's.
- **A palette per client:** the assets its actor may soon see. It's the
  union of the asset lists of the themes of every region within one portal
  hop beyond the load radius around the actor, plus all declared characters'
  assets and every canonical archetype's portable item appearance. Canonical
  archetypes are dual-use, so this conservatively includes some actor assets
  globally. Concealed archetypes contribute only their appearance pool asset.
  This covers carried loot, starting kits and retrieval items across theme
  boundaries without reading lazy regions or current inventory. It comes from
  the package's structure alone, never from what
  regions hold: two games that differ only in hidden contents send identical
  palettes, and because themes belong to zones, entering a room doesn't
  signal what the next one holds.

Palettes have their own revisions and are separate from the observation
stream. A complete envelope includes its required reply context:

```json server-message
{"type":"palette","context":{"input":{"stream":{"stream":"example-stream","epoch":"1"},"readiness_revision":"1"},"actor":"1","branch":"example-branch","cursor":{"sequence":"0","tick":"0"},"revision":"0"},"request_id":null,"palette":{"revision":"1","body":{"type":"full","assets":["terrain.floor"]}}}
```

The context and revision above illustrate shape; real responses carry the current
disclosed attachment boundary.

- Attaching (so also reconnecting) sends the whole palette, unasked.
- When a client's palette changes, it gets
  a delta body with `type`, canonical decimal-string `base`, `added` and
  `removed` assets; this body is inside the same context-bearing envelope.
- The `palette` request returns the whole palette again, with the request's
  id; spectators may send it. A client that sees an asset missing from its
  palette, or misses a revision, should fall back to its own look and ask.
- Nothing is acknowledged. Palettes aren't saved; they're recomputed after a
  restart.
- A scenario that names no assets sends no palettes (a `palette` request
  still gets an empty one), and its observations carry no assets. A game
  that doesn't stream keeps every region loaded, so its palette covers the
  whole package.
- The server recomputes a client's palette only when its actor's region
  changes, since a palette depends on nothing else.

### Palettes in the clients

Every client's connection (`tor_client_common::Connection`) keeps a
`Palette`: the revision and assets it last heard, applied from full palettes
and deltas. It asks for the whole palette itself when:

- a delta doesn't follow the revision it holds (or arrives before any full
  palette). The palette is then *stale*, and nothing is drawn from it until a
  full palette replaces it. While a request is outstanding, nothing more is
  asked.
- an observation names an asset the palette lacks. Each such asset is asked
  about once per connection, so an asset the answer still lacks can't cause a
  loop. Nothing is asked before the first palette, since attaching sends one.

Clients resolve assets through `Palette::resolve` and a built-in `AssetTable`, falling back
through dotted prefixes: `terrain.floor.cave`, then `terrain.floor`, then
`terrain`, then the client's own look. An asset the palette doesn't currently
hold always gets the client's own look, even when the table knows it.

- **Text:** the adventure interface describes floors, walls and ceilings, and
  figures that have no disclosed name, by the shared
  `tor_client_common::presentation` vocabulary, so a `terrain.floor.cave` floor is
  "packed earth". Disclosed names of actors, items and doors stay as they are,
  since commands match them. The `--script` interface doesn't use assets.
- **Headless:** every output line reports the palette held; see the
  [headless client](headless.md).
- **ASCII:** the network worker passes palette revision/currentness changes to
  the UI. Map, look and inventory use the shared vocabulary; palette-only
  changes and stale/recovered transitions redraw without an observation change.
  Disclosed names remain unchanged. Geometry, door state, stairs and memory
  override cosmetic terrain choices; unknown assets use safe class/figure defaults.

The minimal built-in vocabulary contains semantic IDs, stable English noun-group
message keys with singular/plural/indefinite forms, and default ASCII glyph/RGB
values. The [registry](../../crates/client-common/src/presentation.rs) is the shared
source, with dotted-prefix fallback: `creature.equine.quagga` uses `u`, an unknown
equine descendant can say "horse-like beast", and an unknown creature can say
"creature". These are common defaults, with no scenario-name matching or Rogue
letter exceptions. Traits, scenario extensions/overrides, appearance seeds,
player choices and locale switching remain deferred. Cosmetic text atmosphere
and unseen extents do not infer lighting from an observation's visibility limit.

`crates/client-common/tests/it/palette.rs` drives a `Connection` against a
scripted server through a missed revision and a missing asset; real servers
with the checked-in fixtures send neither, so the process test
(`test_streaming_process.py`) covers the attach palette, a request, and the
text client's words. `test_asset_presentation_process.py` additionally covers
native map/look/inventory, repeated screens and a resumed spectator attachment.

## Annotations

Annotations are explicit plain-text history entries, not actions, queries, or
commands embedded in prose. They never enter `Game::act`, consume a turn, alter
the seed, or use simulation randomness. Routine activity remains ordinary events;
the protocol does not generate automatic commentary.

```json client-message
{
  "type":"request",
  "request_id":"note-1",
  "request":{
    "type":"command",
    "context":{"stream":{"stream":"<attachment stream>","epoch":"1"},"readiness_revision":"1"},
    "branch":"<branch from snapshot>",
    "command":{
      "type":"annotate",
      "anchor":{"type":"state","revision":"0"},
      "text":"Return here after exploring the gallery.",
      "source":"user",
      "category":"bookmark",
      "audience":"private"
    }
  }
}
```

`source`, `category`, and `audience` default to `user`, `note`, and `private`.
The text must contain non-whitespace content, occupy at most 4096 UTF-8 bytes,
and exclude control characters other than newline and tab. Frontends should render
it as text, not HTML, terminal control sequences, or executable instructions.

Sources and authors:

- User notes receive `Author::User` with the authenticated user ID.
- Frontend notes receive `Author::Frontend` with that user and the connection's
  declared component label. The label is descriptive, not software attestation.
- Backend notes use a trusted `Service::annotate_backend` API and receive
  `Author::Backend`. Client inputs cannot specify this source or supply an author.

The user/frontend distinction expresses the client's intent; the server verifies
the account identity, not whether a human physically typed the text. Backend
producers must write from disclosed actor facts and use annotations sparingly.
The API validates the actor and anchor but cannot infer whether arbitrary prose
contains a spoiler.

Anchors identify either a disclosed state revision (a decision/observation point)
or a visible history entry ID. An action and its current single semantic event
share one entry ID. Notes can also reference earlier notes. Past state revisions
remain valid; future revisions, inaccessible entries, and another branch are
rejected. A shared note cannot reference a private entry and expose its identity.

Audience `private` means only the authenticated author, across their authorized
frontends. Audience `actor` means all authorized observers of that actor. Neither
scope publishes a note to unrelated actors. Backend annotations are actor-scoped.
Both live delivery and history queries apply the same audience rules.

Each record has an opaque UUID, branch ID, actor, creation tick, author, audience,
and content. These server-generated identities use system entropy outside the
simulation. Branch IDs survive replay, and notes keep their original attachments.
Wizard rewind creates distinct branches and retains existing entry anchors.

## Privileged creature inspection

A wizard command `creature inspect <actor>` returns a dedicated
`creature_inspection { context, request_id, report }` response. The reply context
belongs to the attached observer; `report.actor` identifies the inspected target.
It is sent only to the authenticated requesting wizard while server wizard mode is
enabled. Authority is checked before command decoding, metadata or target lookup.
The target must already be loaded and creature-backed. The query validates fresh
input context, branch and revision but requires no actor control and produces no
saved receipt, journal entry, simulation action, or ordinary observation update.

The independently defined report carries bounded owned HD records, retained health
seeds and cached rolls, template operations, source grants, exact Health/resources
and condition sources. Each disclosed condition has an explicit `phase` of
`active` or `protection`, its existing authorized source and required nullable
`remaining_ticks`. Null denotes an active until-removed condition; finite values
are positive decimal strings. Protection always has a finite remaining duration.
Protection renders as protection against the named effect, never as an active
restriction. This field does not broaden the report's authorization or ordinary
observer disclosure. Missing, unknown, duplicate and positional phase records
reject without an old-shape default.

Each HD contains `point_pools` with named pool identity,
`granted`, `spent` and allowed purchase kinds, plus owned `purchases` with a named
pool and a tagged `skill_rank` or `talent` selection. Attribute choices remain
separate. The report also carries exactly two distinct `starting_skills`, an
explicit `restoration` policy (`choose_again` or `authored`), and dormant talent
records with nonempty reasons matching the disclosed dormant talent list. Pool
spending and purchase ownership must agree: purchased talent IDs are unique and
match the combined active/dormant list, including their disclosed names. The total
ownership limit is 256 talents even when a ruleset grants multiple points per HD.
Each starting skill supplies one rank, and each skill-rank purchase supplies one
more. Their complete disclosed identities and total ranks must match `stats.skills`,
with at most five ranks per skill. Unowned zero-rank rows are allowed; clients do
not hardcode the server's skill catalog or recompute eligibility and modifiers.
Natural and selected attacks are separate. The report's required decimal-string
`revision` identifies the target's current state. `natural_attack` has species
`source`, common skill/bonus/timing fields and `strike_count`, with no authored
bodies or fabricated weapon damage. `selected_attack.attack` is tagged `weapon` or
`natural`; weapon retains its existing bounded damage profile, natural is the same
summary. Selected attribution includes item/socket, attribute/rank/check bonus and
existing per-ability timing/modifier rows.

Required `ability_strikes` contains `{ ability: <ID>, source: ... }` in exactly the
same order and membership as `stats.abilities`, without repeating ability names.
The source is tagged `authored { strike_count }`, `selected_melee`, or
`natural_melee { strike_count, additional_riders }`. Natural source counts require
one through sixteen strikes and zero through eight declared additional riders;
strike count agrees with the invoking creature's natural source, including dormant
unlocks. Other pre-existing main-report source/modifier rows and timing profiles
retain their own bounds; summaries do not remove their separate expansion cost.

### Private strike-source detail pages

`inspect_strike_source` is a typed read-only command with the ordinary current
input context and branch. Its fields are `expected_revision` for the attached
observer, `actor` and `target_revision` from the target report, `source`, and a
required nullable `cursor`. Source is tagged `natural`, `selected_melee`, or
`ability { id }`. Only a currently authenticated wizard in wizard mode can query
it, with authority checked before target or catalog lookup. Unknown and known
but ungranted abilities return the same unavailable result. No receipt, history,
actor control, RNG, active time or ordinary observation is changed.

The private correlated response is
`creature_strike_page { context, request_id, page }`. Its page carries an ephemeral
`inspection` ID, target actor/revision, requested source, resolved ability/species
or weapon/socket attribution, authored `strike_count`, typed `start`, `entries`,
and required nullable `next`. A continuation is `{ inspection, position }`.
The typed position and entry sections are `strike_header`, `success_effect`,
`failure_effect`, `strike_descriptor`, `rider_header`, `rider_effect`,
`rider_descriptor`, `weapon`, and `selected_melee`. Strike and rider indices are
zero-based authored ordinals. Headers retain decimal-string offset, required
nullable unlock/check, and exact branch/list counts. Rider headers require a typed
`trigger` (`success` or `failure`) relative to the primary roller, independent of
who rolled; a checkless primary cannot have a failure rider. They also require
explicit nullable `condition`: null retains branch-only behavior, or
`{ "kind": "sequence_successful_hits", "minimum": 2 }` describes a once-per-sequence
attempt after at least that many qualifying primary hits on the same target,
including the current hit. The minimum is an integer from 2 through 16 and the
condition requires `trigger = success`. Missing fields, unknown variants/fields,
out-of-range minima and failure-triggered conditions are rejected. Clients render
this disclosed condition without evaluating hit history, live eligibility or
hidden state. Compact page assembly retains the condition and rejects changes
when revisiting a header. The condition adds no effect/descriptor entries or
unbounded fields; existing page and frame budgets remain unchanged. Effect/check rows use
`CombatTraceStep`; descriptors and unlocks retain content identities and names.
Traversal is each strike's header, primary success, primary failure, descriptors,
then each ordered rider's header, effects and descriptors. Empty lists and
checkless/nondamaging sources remain explicit. These are descriptive source facts,
not executable rules or a live plan.

An ability whose source is selected melee returns exactly one `selected_melee`
marker, zero authored strikes and null continuation. Clients direct the wizard to
the separate selected-melee selector; they do not invent an authored body under
that ability. A selected weapon returns one typed `weapon` entry.

A natural-melee ability instead pages the species rows with its additional riders
appended to each strike. Its resolved attribution is `natural_melee` with species
and ability identities, `additional_riders`, and a `natural_riders` count for every
authored strike, including dormant ones. The boundary vector has one through
sixteen entries; each count plus the additional count is at most eight. A rider
ordinal below its strike's boundary belongs to the species; subsequent riders
belong to the ability, with the boundary subtracted from their declared ordinal.
Every continuation pins this attribution, so a page beginning in the middle of a
rider still has exact source provenance. Header counts must agree with that split.

Authoring limits are unchanged: 16 strikes, 8 riders per strike, 128 entries per
individual effect/descriptor list, and 256 effects and descriptors across a
source. Each detail page emits at most 16 entries and 128 fields/components.
Capacity is reserved conservatively before projecting each entry. A very small
host frame limit can therefore return a bounded resource-limit error even when
a tighter encoding might fit. If the correlated error fits the advertised frame,
the query returns it without disconnecting. Smaller budgets retain the ordinary
fail-closed output-capacity disconnect; neither case mutates game state or emits
a partial page. Native UUID context plus the maximum escaped request ID fits
that error within 1 KiB, while a 512-byte host budget may not.
Projected fields/components are bounded to 1,024 encoded bytes each, with 2,048
bytes of entry overhead and 8,192 bytes for the complete response metadata. The
conservative full-envelope proof, including array separators, is 172,048 bytes; the hard ceiling is the smaller
of 192 KiB and the host's advertised frame limit. Planning checks operands and
strings before allocating the next entry. An indivisible entry that cannot fit
returns a bounded error when that error fits, otherwise the ordinary clean
capacity close; never an empty/looping page, silent truncation or ordinary reply
chunking. Complete encoded response checks include the actual request ID,
context, source attribution and weapon primary/skill references.
Oversized private replies are identified by a root-kind probe before typed body
decoding, including escaped or reordered discriminators. Successful probing has
no input-proportional allocation; malformed input may allocate fixed-size error
storage.
Snapshot-part assembly admits only one unambiguous Snapshot root before decoding
its body; it cannot carry private pages through the separate 32 MiB recovery
budget. Legitimate large snapshots retain that independent allowance.

Each connection retains one random, ephemeral handle bound to input context,
branch, observer revision, target/revision, selected source and existing run/catalog
identity. Every page rechecks current grants and gear. Reset, rewind, reconnect,
source replacement or stale target/content invalidates it; a stale continuation
never silently restarts. Valid prior positions can be revisited for back navigation.
The server directly indexes the selected rows and catalog references; it does not
build other authored bodies or scan prior pages. Each request first obtains one
current authoritative creature projection, which can rederive expired conditions;
that existing refresh cost is counted separately from bounded page-entry work.

Clients validate role, reply context and pending source/page identity before
installation. They retain at most two rendered detail pages and at most 656
cursor-only history entries. Stale context clears detail and requires reopening.
Headless exposes typed JSON; Text and native ASCII render the same validated
fragments. Private query contents are discarded during stream recovery and never
enter ordinary combat observations. Ticks and seeds use unsigned decimal strings.

## History and persistence

Snapshots contain up to 100 recent visible entries in chronological order.
`history` requests accept `limit` (1–100) and an optional `before` entry ID.
`older_before` provides the next pagination cursor. Filtering happens before
pagination, so private entries do not create visible gaps or total-count leaks.

```json client-message
{"type":"request","request_id":"history-1","request":{"type":"history","before":null,"limit":50}}
```

The versioned journal stores scenario inputs, root branch identity, a permanent
wizard marker, actions, privileged inputs/results, annotations,
and accepted-command receipts. Replay applies actions to the deterministic
simulation and restores notes as metadata, checking the recorded results and
timestamps. Unknown format/rules versions and inconsistent journals fail to load;
they are never replaced with an empty game. Tokens and live connection ownership
are not saved.

Ordinary actions and notes publish after bounded in-memory journal admission.
A background worker saves atomic batches. Acknowledged unsaved play can be lost
on a crash. Explicit save, normal player-client exit, and graceful server shutdown
wait for persistence; enabling wizard authority also waits for its permanent
marker. A sidecar `.lock` file prevents concurrent server writers. See
[background saving](../architecture/persistence.md) for policy, failure handling, the save
format, and the tested durability boundaries.

```json client-message
{"type":"request","request_id":"save-1","request":{"type":"save"}}
```

The save acknowledgement covers the accepted prefix at request time. Play
doesn't wait for disk I/O, and saving doesn't stop a journey. Spectators cannot request saves.

## Validation

Tests cover real WebSocket connections, two observing frontends, control transfer,
authentication/version rejection, private and shared annotations, reconnects,
durable retries, all three annotation sources, anchor validation, pagination,
failed writes, save locking, corrupt saves, slow clients, and client-state ordering.
A process test launches the actual server, commits an action and note, terminates
it, and verifies both after restart. CI runs these in debug and release builds on
Windows and Linux. The text frontend additionally has actual process tests for
interactive input/output, annotation commands, control transfer, and restart
persistence. Graphical ASCII process tests launch real native windows, exercise
native keyboard events, transfer control between text and ASCII, and compare
disclosed state/history after server restart. Linux uses Xvfb; Windows uses a
native desktop. Both debug and release profiles run on both platforms.

Spectator tests send raw WebSocket mutations (including same-user receipt retries),
check actor authorization and privacy, compare each action/result and disclosed
state, verify pagination and reconnection, and reject old protocol handshakes.
Actual text and native ASCII processes additionally test spectator credentials,
read-only inputs, live gameplay, unchanged saves on denied inputs, and restart.
The server configuration tests cover absent, duplicate, and invalid spectator tokens.

## Wizard commands and retained branches

See [wizard mode](../development/wizard.md) for configuration, commands, limits, and testing.
`--wizard` and a distinct `TOR_WIZARD_TOKEN` permanently mark the save before
listening. Only server-granted `wizard` accounts can submit `command` with a
`wizard` payload; ordinary actor control is not wizard authority. These accounts
have game-wide developer authority, while ordinary actions still require control.
Spectators can also use `history_branch` to read permitted abandoned history.

Example payload inside a branch-checked `command` request:

```json
{"type":"wizard","expected_revision":"0","operation":"teleport 1 2 1 1 0"}
```

`operation` is opaque developer text: only the server parses geometry commands.
See [wizard commands](../development/wizard.md) and [wide joins](../architecture/geometry.md).
The server's private journal stores structured inputs/results for deterministic
replay. Public wizard history contains only a summary and a rewind flag, even
for its author. Setup consumes no action time and validates world invariants.

After setup, affected clients receive a `snapshot` with empty `request_id`; rewind
sends new-branch snapshots to all surviving attachments. Such snapshots establish
a new stream boundary and do not complete an outstanding request. Old branch
requests cannot mutate the new branch. Clients attached to removed actors
are disconnected. Normal action updates retain their monotonically increasing
stream rules between snapshots.

`history` returns only the current branch. To inspect an earlier branch:

```json client-message
{"type":"request","request_id":"past-1","request":{"type":"history_branch","branch":"<old branch>","before":null,"limit":50}}
```

Filtering remains actor/user scoped and precedes pagination. Notes never move to
a new branch; new entry anchors cannot reference another branch. Wizard parameters
are private to their author so normal spectators do not receive hidden setup facts.

See [unnamed place hints](scenario-packages.md) for anchor attributes and authoring,
[travel](movement.md) for travel requests and durable receipts, and
[doors](movement.md) for observations, actions, events, and privileged placement.
[Material volumes](../architecture/geometry.md) describe how clients derive floors and
ceilings from seen solid cells, and wizard chamber authoring. [Diagonal movement](movement.md)
describes the four diagonal directions and door reach.
Only the current protocol, save format, and ruleset are supported; there are no
historical rules implementations or save importers.

## Durable places

`observation.places` is the complete authoritative list of learned anchor keys
and the names the character knows them by, each with its `origin`
(`invented`, `authored` or `player`). It includes offscreen knowledge, without
positions, bearings, authored region identities or reachability metadata.
`rename_place { expected_revision, key, name }` requires control; names contain
1–80 UTF-8 bytes with no controls or edge whitespace. The journaled command
increments the actor revision without advancing time and produces a
`place_renamed { key, name }` history event in an ordinary observation update.
Retries, actor audiences, snapshots, rewind and strict replay use the existing
boundaries. See [place knowledge](movement.md).

## Item quantities and knowledge

See [items and character knowledge](items.md) for quantity-aware pickup/drop,
stack identity, randomized appearances, disclosed protocol fields, scenario
authoring, compatibility, and the versioned item profiling workload.

## Physics disclosure

Observations include optional own-body `motion`: velocity, fixed-point units per cell,
and displacement/impact sensations from the latest action boundary. Static
single-cell diagnostic views may omit it. Gravity fields, hidden collision targets,
and backend frames are never serialized. Visible actor cells can repeat an actor
target reference at different observer-relative positions; undisclosed body cells remain hidden.
Both clients narrate involuntary motion and impact, and the ASCII client draws
every disclosed height in one map. See [physics](../architecture/geometry.md) for numerical and persistence rules.

### Ordinary Hit Dice summary

Personal `own_stats.hit_dice` is a required named object with `total` and `sources`.
Each source row requires `source_kind` (`class` or `racial`), a named `source`
content reference and a positive `count`. Identity is the pair `(source_kind, ID)`,
ordered first by kind (`class` before `racial`), then by stable ID. Racial kind
requires the canonical ID `racial`; a class whose ID is `racial` remains a
distinct source. Changing a display label cannot create another identity. Counts
are canonical unsigned decimal strings and sum exactly to `total`, with checked
64-bit overflow rejection. Zero HD requires an empty source list. Actual actor
admission and detailed inspection remain limited to 256 rows in this stage. The source-list decoding envelope is
the 4,096-entry class catalog plus the independent racial source, rather than a
per-HD list bound. Missing, unknown, duplicate and positional fields reject;
the prior per-HD array is not a supported alias.

Ordinary observations contain no ordered HD records, Health seeds or purchases.
Text and ASCII render the disclosed counts without expanding them into records.
Authorized detailed creature inspection retains every ordered `hit_dice` row,
including its required typed source kind, exact seeds and choices. Its validation compares the detailed
composition against the summary by source identity, exact label and count.

## Dungeon combat

Attack actions carry a disclosed opaque actor target reference. Movement never implicitly
attacks. Combat observations carry own HP, preparation/recovery, qualitative
visible-actor injury (`healthy`, `wounded`, `badly_wounded`, `near_death`) and
hostility, the objective's kind when enabled (`retrieve_and_return` or
`reach_exit`) with `exit`, the opaque key of the cell where it's met (the cell
itself is disclosed only when seen), and durable victory/death status.

They also carry `events`: what the action the view follows did, as far as the
observer knows. An `ability` names its disclosed technique, optional `caster` and
`target`, qualitative `outcome` and nullable actual `strike` ordinal as described
above; a participant the observer couldn't see is `null`.
`interrupted` reports the observer's own wind-up being interrupted, and `died`
names a disclosed actor that died, after the blow that killed it. The server
sends facts, never prose: clients write their own sentences, and actor
descriptions and names are empty when nothing is authored. The observer's own
body, including repeated portal views, has `ActorView.id` equal to
`observation.self_target`. `observation.actor` is the separate authorized
attachment identity and must not be used for that comparison.

Combat observations don't carry enemy numerical attributes, AI memory,
internal coordinates, or RNG state. Non-combat diagnostic fixtures
omit the optional combat view.

Required nullable `public_plan` contains the attached observer's most recent
successful paid plan observation. It is historical, not a live inspection:
`observed_at`, the observer-facing `subject_name`, and `state` remain frozen
until a later paid observation replaces them. Preparation can include a nullable
licensed ability reference, the exact remaining work at observation, and either
an unavailable aim or an opaque cell key with bounded, ordered relative offsets
in the observer's frame at that time. No backend actor ID or world location is
included. A name not recognized then stays null even after later recognition.
Saved knowledge is learned only from ordinary disclosed events at a committed
final observer boundary; free queries do not teach names.
Merely being selected by an unseen source-only action is not a personal event
and teaches no name. Genuine attempted target effects and target check
participation retain ordinary personal-sensation disclosure, including misses
and immunity. Internal participation evidence and unavailable-target reasons are
not additional wire fields or narration branches.

Until recovery has separately licensed public provenance, idle, recovering,
and private/non-ability work all yield the same `no_public_plan` state. An
actual public ability preparation can still disclose authorized aim and work
with a null ability reference. This deliberately avoids distinguishing private
work through a special redaction flag, an absent record, or a timing read.
Repeated snapshots, pauses, occlusion and reconnects preserve the paid record;
clients present it explicitly as a past observation.

Required nullable `sequence` is present only for the attached actor's multi-strike
preparation. It contains small integer `completed` and `total`, plus decimal-string
`next_strike_remaining` and `cancel_recovery_remaining`. `total` is 2–16 and
`completed < total`; next-strike time equals `preparation_remaining`. Before the
first strike cancellation debt is zero. After a prefix it includes the outstanding
sequence tail and final recovery, and is at least the next-strike time. While
preparation exists, published `recovery_remaining` is zero; the separate cancellation
debt must not be confused with ordinary recovery. `preparation_active` distinguishes
an active gap from paused progress. These are remaining active-execution ticks,
not an elapsed wall-clock countdown while paused. Snapshot and delta validation reject inconsistent
progress atomically. No opponent sequence total or progress cursor is disclosed.

Personal `own_stats` also requires `restorable_losses`: a named `attributes`
object with all six attribute deficits (integers 0–1000), and `maximum_health`
(integer 0–1000000). These fixed-size aggregates carry no causer or expiry.
Unknown, missing, duplicate and positional fields reject. Ordinary opponent
sightings do not disclose them. Private loss diagnostics distinguish requested
amounts (up to 1000000) from actual attribute changes (up to 1000); clients
present the authoritative applied amount. See
[restorable losses](creature-rules.md#restorable-attribute-and-maximum-health-losses).

Creature-backed combat views include `own_stats`. Its required `health_recovery`
object carries string-encoded `period_ticks` and `elapsed_ticks`, `stopped`, and
`reset_percent`. Periods are 1–100000000 ticks (including nutrition slowdown),
progress is smaller than the period, and reset percentages are 0–100. Full/dead
Health requires zero progress. This personal disclosure is not a resource or an
enemy statistic. Its structural attribute object
requires six named fields: `strength`, `speed`, `intellect`, `willpower`,
`awareness` and `presence`. Its defense object requires `physical`, `cognitive`
and `spiritual`. These fixed DTO fields must not be confused with extensible
content collections. Type/subtype, owned HD source, binding, skill, resource,
talent and ability references use validated content IDs and names. Skill/resource
collections are not required to contain the stock twenty skills or three
resources: validation bounds the lists, checks unique valid IDs and enforces
numeric and pool consistency. Pools separate `balance`, `maximum`, `available`
and `reserved`; available plus reserved equals balance. Granted abilities describe
the build; invocation still validates current grants, readiness, funding and targets.
Supportive reach is `{ "kind": "self_or_ally", "range": { "kind": "touch" } }`
or a nested `sight` range from 1 through 16, paired with target kind
`self_or_ally`. Visible combat actors require the explicit `allied` fact; clients
must not infer alliance from `hostile: false`. Factions are never disclosed.
The observer's scoped `self_target` is the ordinary self-selection handle.
Every ability requires `cooldown_remaining` as a canonical unsigned decimal
string and `supply_costs` as a bounded list of at most eight rows. Each row carries
`supply`, `name`, positive decimal-string `quantity`, and `access` (`carried_only`
or `carried_or_ground_touch`). Only owned abilities receive this projection;
retained cooldown entries for lost grants are not an ability-discovery channel.

Every `use_ability` action requires `supplies`: `{ "kind": "carried" }` selects
from owned inventory, while `explicit` carries one through 32 `items` rows with
`cost_ordinal` and observer-scoped `item`. Rows are unique and sorted by ordinal
and opaque handle. Clients send no quantities. The server resolves only currently
disclosed exact sources, repeats access checks, and canonicalizes private IDs
independently of opaque ordering. Missing fields and malformed selections reject;
no historical action default is inferred.
`basic_melee` is an inspection technique, not a paid `use_ability` invocation.
Non-creature diagnostic actors with no combat state omit the combat view entirely;
there is no transitional flat combat-profile implementation. Enemy entries never
receive these personal numerical values; health seeds, reservation identities and
grant provenance remain private. Full views and combat deltas use the same projection.

After reconnecting during preparation, a journaled input boundary preserves
progress and waits for fresh input. Use the disclosed `resume_intention` identity
and current input context to resume the paused preparation. If a saved run
is in recovery with AI ready, `continue` resumes autonomous scheduling without
starting a new player action; text `wait` and ASCII Space issue it while unready.
Only the attached controller can continue. Spectators cannot request it.

Every item view requires nullable `known_supply` (`id` and `name` when known).
An unidentified item cannot expose a supply capability. Ground items separately
require `supply_reachable`: this is ordinary contact reach for a known supply,
not pickup reach. It is false without `known_supply`. Neither capability nor
reach depends on held quantities; claimants, free-unit counts and reservations
remain private. The server repeats exact-source checks when an action starts.

### Disclosed item interactions

An optional observation `interactions` describes the attached actor's anatomy
slots, actor-owned preparation and carried-item affordances. Prepared actions use
opaque actor/item targets, and remaining ticks use exact decimal strings. Slot
indices are bounded by the disclosed anatomy; duplicate kinds are distinct slots.
Equipment statistics are absent for unidentified items. Every interaction row
includes required-nullable `known_projectile_bonus`: either null or a strict named
`{hit, damage}` record of signed remembered instance enchantments, each bounded
from -1,000,000 through 1,000,000. Only known projectile identities with stored
numeric knowledge disclose this record; plain ammunition does not acquire an
equipment socket or full attack view. Hidden live condition changes never refresh
these remembered values. Spectator snapshots and delta baselines remove the
record, and another actor's observation does not inherit the owner's memory.
Clients label these as remembered bonuses, not resolved hit or damage totals.
Remembered `known_cursed` and `known_protected` are independent exact-instance
facts: they require neither an equipment slot nor numeric or type knowledge.
Actual `equipped_slot` and `known_equipment` still require a disclosed slot.
Consumable effect
primitives stay in the backend. Deltas replace this optional snapshot, including
removing it when no interaction facts remain. `equip`, `unequip`, `drink`, `eat`,
`read` and `zap` use
the same intention admission, authority and retry rules as other ordinary actions.
The interaction snapshot also carries `completed`, a list of the observer's own
completed item actions. These use opaque targets without item specifications;
the target may refer to a consumed final unit absent from the current inventory.
Clients resolve narration from their consecutive disclosed inventories. Other
actors' equipment and consumption completions are not included. This also works
for actors with anatomy and equipment but no combat state.

A required `refused` list separately records paid item attempts that completed
without consuming a charge or applying their effect. Each strict named row has
`action` (the captured opaque-target Zap action) and the closed string `reason`,
currently only `too_weak`. It is not an admission rejection, interruption or
successful consumption. The owner or a follower observing that owner can narrate
it, but unrelated observers receive no such row. It discloses no item family,
ability identity, Health amount or recipient count. Exact retries do not append
another refusal or pay again.

Read and Zap take an opaque carried `item`; Zap also takes a closed string
`direction`. Each item interaction row includes `readable`, `zappable`, and an
explicit nullable `charges` count. Unknown loaded and empty devices have identical
affordances. Charges are present only for the owner's current exact-instance
knowledge. They do not represent quantity, free units or reservations.

An explicit nullable `identification` carries one opaque `choice`, its physical
`class`, and at most 64 unique opaque current carried `items`. It appears only
after the identifying scroll has been consumed. `identify` submits that `choice`
and an explicit nullable `item` (null means decline without refund). Tokens are
observer-scoped and cannot be substituted across actors, branches or stale reply
contexts. The ordinary controller, receipt and retry contracts apply. Selection
and charge facts, learned equipment numbers, curse/protection knowledge, and
curse-blocked item choices are removed from spectator full snapshots, live action
updates and delta baselines, including when they are the only private facts.
This is server authority, not a client display convention. Headless exposes the
same actions as `act` JSON, including the explicit null for decline.

Private `combat_diagnostics` replies carry the requesting observer's reply context,
request ID and a validated `CombatDiagnosticsView`. Wizard commands `combat inspect
[through]` and `combat capture on|off` are session queries/runtime controls, not
journal commands. Account and global wizard authority precede decoding and retries;
input context, branch and revision must be current before reading or changing
capture. Neither operation advances time or changes ordinary observations/history.
Reports contain at most eight consecutive records from a 64-record runtime window.
The optional positive canonical decimal `through` is inclusive; expired or future
cursors fail. Clients reject private reports for Player/Spectator roles, validate
numeric semantics and discard report contents during stream repair. Headless emits
the structured message; Text and ASCII share a validated numerical formatter.


### Nutrition and eating disclosure

`eat` has only `type` and an observer-scoped carried `item` target; quantity is
always one. The item interaction record requires `edible` alongside `drinkable`.
A missing or false capability cannot be inferred from food/corpse class or name.
Only personal `own_stats` and authorized creature inspection include required
`nutrition`: `{ "kind": "exempt" }`, or `kind: "required"` with integer `reserve`,
`maximum`, `starve_at`, `hunger` (`nourished`, `hungry`, `weak`, `empty`), and
unsigned decimal-string `unit_ticks` and `elapsed`. Health recovery is validated
independently; hunger labels do not disclose its authored slowdown/stop policy.
Ordinary actor sightings have no nutrition field.
Nourish diagnostics carry the actual added amount through the existing private
resolution report. Clients present these facts; they never advance reserve or
apply meals themselves.


### Personal condition disclosure

Personal `own_stats` requires `conditions`, an array bounded to 128 records, each
with named `condition` (`id`, `name`), `phase` (`active` or `protection`) and required
nullable `remaining_ticks`: a decimal string from 1–1000000 for finite time, or
JSON null for an active until-removed condition. Missing fields, finite zero,
string `"null"` and null protection durations reject. Source actors, opponent qualifications,
reservation owners, stacking internals and private preparation operands are absent.
Only Timed lifecycle entries appear here. One-use conditions remain governed by
their existing authorized consumable-token contracts, so a hidden attacker reserving
a token cannot change this projection. Foreign sightings never gain this field. Saved/current snapshots and reconnects disclose the same live actor facts
without settling time or sampling RNG. Text and ASCII use the same active/protection
labels and render a null duration as “until removed”; protection does not mean
the actor is still frozen or fainted. Clients do not invent deadlines or count
down persistent conditions.

These lifetime shapes and the expanded timing/effect contracts require the
coordinated current identities in [status](../status.md). Canonical saved condition
records also require explicit nullable lifetimes. Saved execution retains frozen
timing and ray delivery, plus exact bounded resolved healing-improvement claims. Current
wire/save readers reject older shapes rather than supplying defaults or converting
finite zero into permanence. Physical save framing and compiled region-input
identity remain unchanged.

A personally experienced standalone condition can also produce a combat event
`{ "type": "condition", "condition": { "id": "rogue.faint", "name": "Fainted" } }`.
This event has no ability or hidden causer identity. Its scope is the effect's own
actor/recipient; it does not reveal another actor's private condition event.

## Private creature catalog

Wizard developer text `creature list [depth <N>] [after <selector>]` returns
`creature_catalog { context, request_id, page }`. The page requires `depth`
(null for authored base or a positive 32-bit depth), `entries` (at most 32), and
`next` (null or the final returned selector). Each entry requires `selector`,
`name` and `details`; selectors are unique and strictly ordered, with bounded
text lengths. Pages also fit the configured outbound frame budget, counting the
complete correlated envelope, escaped request/context fields and continuation.
A single row that cannot fit returns a short `resource_limit` error; it is never
silently omitted. The server returns only this requesting wizard's reply. Query
results grant no authority, alter no state, and are discarded during stream
repair like private creature inspection. Players and spectators are rejected
before parsing. See [named spawning](../development/wizard.md#named-creature-spawning)
for resolution and mutation semantics.


### Identified currency

Item views require nullable `known_currency`. The typed value `gold` is disclosed
only for identified coin items; its amount is the exact stack quantity. Null
means currency is not known, even if a label or asset resembles gold. Apparent
objects never disclose this capability. Attached spectators see only the same
ordinary identified item facts already available in their observation; this field
does not reveal private equipment knowledge or hidden item definitions.
