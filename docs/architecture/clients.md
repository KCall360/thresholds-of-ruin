# Disclosed-state clients and narrative presentation

`tor-client-common` owns the shared connection and validated disclosed state.
Text, native ASCII and headless clients depend on that boundary, not world or
simulation crates. The backend owns rules, perception, actor knowledge and
semantic events; clients turn those facts into commands and presentation.
They do not simulate hidden outcomes or reconstruct private topology.

Player syntax belongs to [text](../play/text.md) and [ASCII](../play/ascii.md).
Wire ordering, limits, target references and delta formats belong to
[protocol](../reference/protocol.md). This page describes client responsibilities
and the presentation invariants needed when extending them.

## Validated state and four kinds of memory

Shared state validates ordering, stream context, exact delta bases, disclosure
and history consistency before mutation. Invalid updates change neither state
nor narration. Shared immutable current views can be retained without later
updates mutating their bases. Connection resynchronization uses explicit snapshot
boundaries; it is not permission to skip an intermediate update and continue with
an invented base.

Keep these memories distinct:

1. **Authoritative character knowledge:** backend navigation, known item facts
   and learned place names. It survives disconnect/restart and rewinds with the
   chosen game boundary.
2. **Last-seen client memory:** historical disclosed cells keyed by opaque cell
   identity. Hidden changes do not refresh them. Stored offsets describe the
   last sighting, not a current global map.
3. **Aligned presentation chart:** a bounded coordinate chart derived from
   successive views. It can be discarded without forgetting authoritative names
   or routes, or deleting the separate historical sightings.
4. **Text conversation/presentation state:** pronouns, clarification, pending
   command chains, unhinted place identity and visited-description detail. It is
   local to this client and not durable game state.

Cells refresh only when disclosed, including clearing removed items. If one cell
appears at several offsets in a view, key-indexed memory retains the last
occurrence in deterministic scene order. Current observations retain all distinct
occurrences. Creature occupants do not persist as ghosts in the remembered map.

### Aligning map memory

[`map_memory.rs`](../../crates/client-common/src/map_memory.rs) matches keys that
occur exactly once in both successive views. Every usable match must imply the
same translation. Elevation and rotated crossings can preserve a chart when the
displayed axes and offsets agree. Absent, ambiguous or conflicting overlap resets
the chart rather than guessing a global embedding. Teleportation and cyclic
non-Euclidean views can cause this deliberately conservative reset.

The chart retains at most 4096 occurrences, preferring nearby cells with stable
tie-breaking. Overflowing translated coordinates are discarded. Seeing a key
refreshes all retained occurrences of that physical cell, preserving their
aligned positions while clearing actors. A stationary aligned map avoids
rebuilding its coordinate index.

Aligned same-branch snapshots retain connection-local memory; branch changes
clear it. A fresh client begins from its snapshot, not a reconstructed tour of
saved history. Three-dimensional sight fills the bound with headroom and surfaces,
so tall rooms can retain less explored floor area. Increasing capacity or dropping
headroom requires measurement and care: headroom distinguishes low obstacles
from full walls in the merged map. The display window is not a promise that all
its possible cells fit in the cache.

## Text parsing and resolution

The deterministic pipeline is:

`line → parse → resolve → plan → execute → chronicle → compose → prompt`

- [`parser/token.rs`](../../crates/client-text/src/parser/token.rs) tokenizes
  words, numbers, punctuation and quoted literals. Directions are parsed from
  words; there is no separate direction token variant. Words are normalized to
  lowercase while quoted literals retain their content. Sentence boundaries
  include `.`, `;`, `!`, `?`, `then` and `and then`.
- `parser::lexicon` and `parser::grammar` recognize verb aliases and intransitive,
  directional, transitive, prepositional and multi-object syntax. Multiword verbs
  take precedence. Recognition is broader than implemented effects.
- [`parser/noun_phrase.rs`](../../crates/client-text/src/parser/noun_phrase.rs)
  retains head/adjectives, determiner, quantity, ordinal, pronoun, `all` and
  exclusions. Parsing stops before binding these phrases to game objects.
- `engine::scene` builds referents from the current disclosed view.
  `engine::resolve` binds noun phrases; `engine::verbs` gives supported verbs
  meanings; `engine::turn` executes their goals.

Bearings use the observer frame, with diagonal sectors spanning ratios from
1:2 to 2:1. The scene has item, figure, door, surface, place and way referents. Equivalent
items with the same disclosed identity/appearance and holder form countable
groups. A multi-cell actor is one figure, and the observer is the `me` referent,
never a second person in the room. Repeated door/body occurrences do not create
extra objects. Surfaces use seen material cells only.

Every meaningful word must match, with a small synonym vocabulary for head nouns.
Verb domains narrow before ambiguity: take prefers ground items, drop carried
items, attack figures and open doors. Indistinguishable items in the same
situation do not force clarification; objects in different locations can remain
distinct choices. `all` and exclusions expand within the verb's domain.
Pronouns update from both player references and narration.

A clarification saves the command and the remainder of its chain. A number,
ordinal or identifying phrase selects a candidate; a new command abandons the
question. Clarification, examination and refusals do not consume simulation time.
A recognized unsupported verb resolves its object before refusing the operation;
it never narrates an effect that the backend lacks.

### Goals, admission and turn completion

Verbs are not backend actions. `take` may approach then pick up; `open` may
approach then manipulate; ordinary `attack` may approach then use basic melee.
Equipment/removal/drinking use the common item action builder. Granted techniques
use the disclosed ability builder and act from the current position, without an
implicit approach. `wait` can continue saved recovery rather than create a new
wait action. Diagnostic `--script` bypasses adventure planning and retains
immediate one-cell/interaction semantics.

`engine::turn` decides each step from the view then in hand. It rechecks target
presence, reach, state, control and readiness before submitting. Travel is never
asked to attack. Arrival does not authorize a follow-up if a previously unseen
actor appeared, including on the destination step. An approach blocked because
the target stepped into valid melee reach may still proceed to the attack after
current validation.

The async `Link` boundary wraps the real common connection; tests provide scripted
links whose observations pass through the real client state. Acknowledgement
means admission, not execution. The turn reader associates the receipt with
intention failures/cancellations and consumes readiness permissions as well as
observations. An ordinary action settles only after acknowledgement, a newer
observation revision, character readiness, its stopped execution lifecycle and
that lifecycle's later permission envelope (or terminal run). Permissions that
allow admission during timed preparation do not settle a later ready observation.
The settled execution boundary remains valid if recovery becomes ready afterward
without changing permissions. A journey also requires its own terminal travel phase. This
prevents a prompt from authorizing the next request between an observation and
its later readiness update. Advancement confirmations and other admitted tool
requests in both text modes wait for their exact intention to leave the queue
and for the following permission update before returning a prompt. Timed work
may still be preparing then; in the adventure interface an explicit server wait
for another player can return control with the intention still queued.
Immediate query/session replies
keep their acknowledgement boundary.

A refusal, failed recheck, interruption, clarification, control loss, terminal
run or replacement snapshot stops the chain. A server `waiting` response for
someone other than this player ends the current passage and stops remaining goal
steps and chained commands. Only observed effects are narrated; retained paid work
is not described as completed. If no effects were observed, the passage explains
whether another character must act, needs a controller, or play has stopped.
Subsequent activity is narrated between turns. A ten-second silent-answer guard is a
fallback, not a scheduling clock. Before acknowledgement, while an admitted
action or tool request still lacks its execution/permission boundary, or when
an ended journey lacks its final permissions, it reports an uncertain outcome and tells the player to inspect history after reconnecting.
A known failed action with missing permissions reports that failure and requires
reconnection instead of offering stale input authority. It never retries a request
automatically. Other acknowledged turn waits can
end on silence without inventing completion effects.

Between-turn observations are gathered after 150 ms of quiet and composed above
a fresh prompt. Presentation pacing changes when already committed activity is
shown, not when the simulation acts. Neither pending text chains nor approach
follow-ups resume after client restart.

### Chronicle and composition

`engine::chronicle` compares consecutive validated views plus explicit disclosed
events to produce structured beats: movement, travel outcomes, item operations,
doors, attacks, injuries, deaths, discoveries and run outcomes. Beats retain
referent identities instead of prose. `engine::narrate` combines them with the
turn's goals; `engine::prose` supplies articles, plurals, lists and agreement.

Journeys collapse into destination/direction clauses; successful approach and
interaction join into one sentence. Interruptions retain the original purpose.
Combat preserves event order; changed HP follows the passage once. Arrival can
append a new place description. Repeated references use names/pronouns, and
narrated discoveries become usable pronouns. None of this infers an unseen cause
or treats leaving sight as death.

## Places, ways and atmosphere

`engine::place` surveys current and aligned remembered disclosed cells through
`engine::seen`. Remembered geometry can keep a doorway usable in language when
a new angle hides its floor; things and figures come from current perception.
This is an explicitly client-side heuristic, not backend room membership.

At the actor's level, an open column requires known open foot space, no doorway
and no seen wall above. A place floods through open columns without crossing a
door or narrow gap. A gap is at most two columns wide with closed or unknown
columns on opposite sides along an axis. Standing in gaps describes their
passage. With no seen walls, diagnostic/open ground uses nearby hint identity
instead of claiming enclosure from unknown cells.

Touching boundary openings group into doors, passages or archways. Their primary
names use the side of the place, with the actor-relative bearing as a fallback.
Destinations can extend up to three seen steps beyond an opening. Stairs underfoot
are vertical ways. When no opening matches, an authored hint visible in another
place is a fallback; otherwise an open frontier can use the farthest seen floor
in that direction. Hints are optional naming/navigation aids, never prerequisites
for recognizing physical exits.

Current form heuristics classify no-wall space as open ground; gaps or narrow
elongated places as passages; up to six columns as alcoves; up to forty as
chambers (small up to fifteen); larger areas as halls. These are presentation
choices, not world rules or durable discovered room extents. Unknown perimeter
columns can mean the place continues out of sight; they never become fabricated
walls.

A directional walk may issue several travel legs through empty ground or follow
a two-way corridor around bends. It stops for newly seen creatures, objects or
doors, a new room/hint, a junction, a door/opening ahead, lack of further route,
or its twelve-leg safety bound. The whole walk is composed as one intention.
The server's travel hazards are independently enforced.

Descriptions combine perceived proportions, materials, enclosure and ways with
current contents. Equivalent creatures/items group in prose; the observer never
appears as an external figure. `look` always describes fully. Arrival verbosity
and already-described places belong to the current engine instance only.

`engine::atmosphere` chooses material-linked mood, air, smell and sound by separate
hashes of a place key. It is cosmetic, deterministic and never evidence of a
hidden actor, mechanism or authored inscription. Hinted places use their opaque
anchor key. Unhinted places inherit a key from previously associated cells, or
initially the lowest open-cell key, so partial exploration can keep them stable
within a connection. This does **not** create a backend hint, durable renameable
anchor or cross-client identity. Authored/player names title places; backend
invented mnemonics are left unsaid by text. Scenario themes/zones are not used as
an unrestricted source of narrative facts.

These boundaries preserve four lessons from earlier spatial design work: one
actor is one narrative presence; exits must not require hints; prose follows
disclosed geometry; and deterministic flavor does not make a presentation cache
authoritative. Client-local caches are legitimate when their lifetime is explicit.

## Durable place knowledge

A place hint is a boolean on an authored cell, independent of region/portal
membership. It carries no extent, room membership or implicit explored area.
Authors/generators choose anchors explicitly; the basic room generator does not
infer them. Separate authored public names can label hints. These public names
are intentionally disclosed; private backend region labels and topology are not.

Only perceived cells reveal their hint, under the existing opaque cell key and
relative occurrence positions. Repeated occurrences identify one anchor.
Last-seen hint memory stays stale until revisited; refresh to `false` clears the
current fact. Making a hinted cell solid retains authoring but suppresses hint
disclosure; carving it can reveal it again. Editing terrain does not infer new
anchors. Authorized wizard place edits and package authoring use the same
validation boundary; public wizard summaries omit private coordinates.

The simulation learns places from authoritative perception. Names are owned by
the character, not shared between characters. First discovery uses an authored
name or a deterministic mnemonic based on seed/discovery order. The mnemonic
vocabulary has 256 combinations, then numeric suffixes. It asserts no physical
property. Name origin is `authored`, `invented` or `player`.

Rename accepts 1–80 UTF-8 bytes, no controls or surrounding whitespace. Duplicate
names are allowed: UI list numbers resolve to the exact opaque key, not a name
match. List numbers are local, not durable IDs. A current floor occurrence with
the same key establishes `in sight`; remembered entries have no live bearing or
reachability guarantee. Text displays/travels by authored and player names;
ASCII displays every name.

Renaming requires control, checks revision, journals without advancing time and
cancels active travel at the command boundary. Spectators can read but cannot
rename. Exact retries retain the receipt. Learned locations/names survive
snapshots, restart and checkpoint/replay, including later removal or concealment
of the hint. Rewind restores the chosen boundary; abandoned discoveries disappear.
This is historical character knowledge, not persistent frontend map memory.

Backend maps share source-region ownership with navigation; refresh inspects only
perception and does not detach unchanged storage. Checkpoints deduplicate learned
maps across boundaries. Observations disclose names and actor-specific opaque
keys, never private region coordinates. See
[persistence](persistence.md) and [known-cell travel](../reference/movement.md#travel).

## Narration and stream recovery

Shared narration derives prose from consecutive validated observations and
explicit action/combat results. The server sends facts rather than sentences.
Repeated portal views count once; self-sight is not discovery. A figure leaving
perception is not assumed dead/moved. A door change is announced only if the door
was perceived in both views and never names an unseen cause. Hidden actions and
unchanged views produce no invented prose. Snapshot/reset clears comparison
baselines without replaying old notices. Narration is not durable history.

Ordered-strike presentation uses only the attached actor's disclosed `sequence`
progress and each perceived ability event's actual primary ordinal. It describes
active inter-strike gaps, paused prefixes, next-strike timing and cancellation debt;
it never infers an opponent's future strikes. The first primary strike retains the
ordinary wording, while later hits, unaffected results and misses identify the
later strike. An intermediate result does not terminate the intention or free a
client to submit another action without current readiness permission.

Plain cancellation keeps the ordinary queued-first selection and sends a null
step. Escape-aware movement requires an own completed prefix and exact cancellation
permission, selects the active/paused preparation ahead of unrelated queued work,
and sends one cancel-with-direction request. ASCII handles that before bump-attack
conversion; Text exposes explicit escape/cancel-direction syntax. Clients do not
predict movement success, spend recovery, or rebuild frozen strike sources. Private
inspection renders typed authored source rows separately from ordinary observations.
Its main report contains the target revision and lightweight natural, selected-melee
and granted-ability source summaries. Authored counts include locked strikes.
Selecting a source sends a read-only typed query through the current command
context; it requires wizard authority, but no actor control or action receipt.
Only one bounded detail page is rendered at a time, preserving authored primary
branches, checks, each rider’s explicit primary-success/failure trigger,
descriptors and unlocks without reconstructing rules.
The navigation contract retains at most two rendered pages and a bounded history
of typed cursor starts; it never concatenates an entire source. A selected-melee
ability is a marker linking to a separate selected-source query. Reset, changed
context, refusal and reconnect require reopening stale inspection state.
Unrelated main-report modifier/source-row expansion remains a separate limit;
strike paging does not make every private report component constant-sized.

ASCII filters and merges raw shared narration: it suppresses readiness, own
steps/waits and other facts already shown by map/status, combines a lethal hit
with the kill, reports underfoot objects and retains messages until the next
command. Its `narration` diagnostic array remains raw; `messages` records what was
actually displayed. `--More--` pages long turns without consuming a game command;
spectators show latest rows without blocking on paging. Text composes its own
passages from the same facts; diagnostic text retains detailed framing.

Bounded queues preserve observation order. A slow connection can pause progress
while nearing capacity; persistent pressure or overflow causes explicit closure,
not dropped truth. A slow spectator may delay a journey but cannot retain the
session indefinitely. Clients reject gaps, and a replacement connection gets a
fresh committed snapshot. Automatic reconnect and retained-stream resume are
not implemented. ASCII retains its final disconnected view; text exits. Uncertain
requests are never automatically replayed. Exact transport policy belongs to
[protocol](../reference/protocol.md) and server execution to
[simulation architecture](simulation.md).

## Native presentation and diagnostics

ASCII uses minifb for a native pixel-buffer window and font8x8 bitmap glyphs;
rendering dependencies stay out of simulation/protocol. A connection worker
feeds ordered disclosed updates through a 64-event channel, without copying full
historical memory into each event. Pressure blocks the worker rather than dropping
updates. The native loop targets 60 Hz, repaints on change, handles at most 16
network events per turn and stops starting another after four milliseconds.
A single update/draw/native call/OS stall can exceed that budget: it is not a
hard real-time deadline.

Every intermediate observation is applied even if several are combined into one
presented frame. Changed observations invalidate pending selections. Tile
preparation indexes cells/occupants once, preserves first-occurrence precedence
and stale coloring, and shares map layout with perceived/remembered foot-cell
hit testing. Window resize, cursor movement and redraw never advance time.
Only one request is in flight; gameplay input while pending is discarded rather
than silently queued into extra turns.

Diagnostic frames are emitted after successful native presentation and optional
PPM capture. `profile.version=1` reports update application, drawing, native
presentation/pacing, capture, prior report duration, turn interval and consumed
network events. Native time includes the library frame limiter. `presented_unix_ns`
is sampled just after presentation returns; `previous_report_encode_ms` and
`previous_report_write_ms` separate encoding and stdout/flushing for the previous
report. `previous_report_ms` also includes capture/JSON/report work. Scheduling
inside these intervals is included. Reporting is synchronous and can delay input;
it is not physical keyboard-to-photon latency. Deferring a harness's disk logs
does not disable the application's synchronous stdout work.

With native input and `--report-frames`, an acknowledged input frame also carries
`native_input_sequence`. Once that physical key is observed released across a
subsequent native pump, one separate `native_release` record identifies the same
sequence. This lets process tests distinguish a posted key-up from minifb's
completed held-duration reset before pressing the same key again. Release
records are not presented frames, do not trigger redraw/capture, and contain no
game state. Tracking is bounded to one pending sequence per physical key and
does no key sampling or output when reporting is disabled; automation does not
generate native sequences. These are local process diagnostics, not network,
save or ruleset formats.

## Extension and verification map

When adding an interaction, update grammar if needed, verb-to-goal semantics,
current-view step validation, disclosed-event chronicling and narration together.
Do not widen scope to hidden properties or mistake recognized grammar for rules.
A new persistent fact belongs in the backend/protocol, not an authoritative
client sidecar. A purely cosmetic cache should instead have explicit reset and
identity behavior.

Tests include parser pipeline, scene grouping/resolution, place geometry,
chronicle/prose and scripted whole-turn links in
[`client-text/tests/it`](../../crates/client-text/tests/it). Common-state and map
memory tests cover atomic rejection, repeated occurrences, snapshots, rewind,
translation conflict, bounded capacity, item refresh and actor exclusion. Native
model tests cover keys, modal screens, memory, running and message paging.

Actual-process coverage includes
[`test_adventure_process.py`](../../scripts/test_adventure_process.py),
[`test_ascii_memory_process.py`](../../scripts/test_ascii_memory_process.py),
[`test_stream_recovery_process.py`](../../scripts/test_stream_recovery_process.py),
and [`test_client_responsiveness_process.py`](../../scripts/test_client_responsiveness_process.py).
The recovery relay tests delayed delivery, omitted updates and fresh snapshots;
responsiveness tests native input while saving is blocked and during action
bursts, with exact final state/history/checkpoints. These complement native
controls and disclosure assertions, not replace them.

Older timing summaries remain in the
[original narration record](https://github.com/KCall360/thresholds-of-ruin/blob/59f48988f562d89ac9a4130f4df43a2dc2819912/docs/narration-and-recovery.md#verification)
and [place-knowledge evidence](https://github.com/KCall360/thresholds-of-ruin/blob/docs-history-2026-09/docs/place-knowledge-findings.md).
They are historical workload measurements; consult
[performance methodology](../development/performance.md) for current comparisons.
