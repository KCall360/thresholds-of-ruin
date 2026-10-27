# Observer-facing appearance composition proposal

Status: the maintainer authorized protocol and server revisions for worn and
equipped appearance, and protocol 64 implements the
[first slice](#implemented-first-slice-protocol-64) under the recommended defaults
below. Everything beyond that slice remains a proposal. Save and content formats are
unchanged, and the current [format identities](../status.md) and strict rejection
policy still apply.
This document makes the [procedural handoff](procedural-model-handoff.md) concrete;
it does not mark the broader refactor complete.

## Recommendation

Send a bounded, observer-projected description of the **apparent body and visible
components**, independent of inventory records and renderer implementation. Replace
the complete description atomically when its public facts change. Resolve local
geometry, fitting, attachment transforms and animation only in the rendering client.

Use one appearance entry per disclosed actor target in an observation, shared by
all of its visible cells. Retain a dedicated self appearance outside the eye scene,
and use the same description type for a corpse's currently disclosed appearance.
Do not introduce component patch streams, globally identifying wearable IDs, or a
second appearance revision counter for the initial implementation.

## Existing boundaries to preserve

| Evidence | Consequence for the refactor |
| --- | --- |
| [Wire appearance and actor views](../../crates/protocol/src/wire.rs): `CreatureAppearance` is seed plus optional asset; `ActorView` also has name, description and a position | There is no server-driven wardrobe today. Repeated actor cells must share one projected composition rather than multiply it |
| [Target scope](../../crates/server/src/wire_targets.rs): `appearance_seed` is shared across observers and reloads, while interaction targets include the observer | Keep cosmetic identity separate from permissions and command targets; never use simulation RNG for presentation |
| [Adaptation](../../crates/server/src/adapt.rs): only creature disclosure receives an actor appearance seed; self and corpse appearances have separate paths | Do not attach true-form equipment to anonymous contacts, apparent objects or other less-disclosed presentations |
| [Observation deltas](../../crates/protocol/src/delta.rs): changes reproduce a full observation at a stream revision | Appearance replacement belongs to the same atomic state update and reconnect/full-snapshot contract |
| [Server palette preparation](../../crates/server/src/session.rs) and [palette source](../../crates/server/src/engine.rs): availability is forecast from package/region and cached by region | A palette is an available-content catalog, not evidence that an actor wears an item. Runtime visual content cannot silently invalidate this cache contract |
| [Client asset discovery](../../crates/client-common/src/palette.rs): `observation_assets` now includes direct assets plus self and ground/carried corpse body assets | The nested-carrier regression failed before the fix; connection tests and a real-server/native-ASCII repair test now cover the boundary |
| [Connection](../../crates/client-common/src/connection.rs): remembered map pages walk terrain, door and item assets; their current wire type has no corpse-body appearance | Keep live and frozen-memory disclosure distinct; see the carrier audit below before extending either walker |
| [3D adaptation](../../crates/client-3d/model/src/observation.rs) and [scene cache](../../crates/client-3d/src/scene.rs): model and cosmetic seed currently drive actor/corpse reconstruction | A visible composition change must invalidate the resolved composition, even when body model and seed are unchanged |

## Proposed information model

Names below describe responsibilities, not approved serialized field spellings.
The initial record has no recursive children or arbitrary executable expressions.

| Part | Public data | Deliberately excluded |
| --- | --- | --- |
| Apparent form | Palette asset/content recipe, existing cosmetic identity where disclosure permits it, bounded semantic cosmetic parameters | True species beneath a disguise, private actor ID, simulation stats, Bevy `Look`/`Variant` serialization |
| Visible surface coverage | Only disclosed semantic regions needed to interpret the observed view; unknown regions remain unknown | A complete hidden inventory/slot list, private anatomy inferred from unused attachment ports |
| Visible component | Public recipe, semantic attachment role, public layer relationship, bounded cosmetic parameters | Inventory item target, curse/enchantment/modifier records, attack statistics, arbitrary mesh paths |
| Attachment role | Body-relative semantic role and a bounded occurrence where the disclosed form supports repetition | Bone indices, world transforms, rig entity IDs, an assumption that every creature has two human hands |
| Presentation vocabulary | Shared content keys for garment/category/color descriptions and supported fallbacks | Raw asset IDs treated as prose, renderer-selected names presented as gameplay identification |

Treat an outfit as defaults in content, not a separate source of truth competing
with explicit equipment. Resolve defaults and overrides into the publicly visible
component list on the server. A knight outfit's default helm and an equipped helm
must not produce two helms. A client may group components for construction/caching,
but must not restore a hidden default after the server removes or covers it.

A single authored component can declare several local geometry parts, such as a
robe's sleeves, skirt and collar. These are not separate network entities. Likewise,
a pair of boots can be one visual recipe whose attachment capability describes its
supported feet; unrelated boots can be distinct visible components if the content
and body support them. Repeated-limb ordering comes from the public body recipe,
not array insertion order or private item IDs.

Use typed, bounded semantic parameter values: booleans, small integers, enumerated
content values and palette/color tokens. Quantize authored numeric variations before
serialization. Do not pass arbitrary JSON objects, unbounded strings, unrestricted
floats or the 3D renderer's complete morph controls through the protocol. Text and
ASCII may ignore irrelevant visual detail, but must not reinterpret it as mechanics.

## Disclosure and indistinguishability

The projection must run after observer-specific visibility/knowledge filtering.
It must also respect the existing owner/spectator redaction boundary. Renderer
occlusion is not permission enforcement: a hidden ring must not be transmitted
merely because the 3D renderer promises to hide it beneath a gauntlet.

For two world states that differ only in an undisclosed fact, the observer's
appearance records must be byte-equivalent after canonical serialization. Examples:

- Changing a hidden curse or enchantment leaves appearance, ordering and seeds unchanged.
- Replacing an item with a mechanically different but visually identical item leaves
  the public composition unchanged.
- Putting a ring beneath a covering glove does not reveal a ring recipe, slot count,
  new cosmetic seed, per-item token or appearance-only revision bump.
- Removing the glove may disclose the ring if it is now observable. Removing the ring
  while the glove stays on must not disclose that change.
- A cloak covering armor suppresses the covered armor according to server-owned
  semantic coverage rules. A camera orbit cannot make the client discover secret gear.

Do not derive a wearable cosmetic seed directly from private item identity by
habit: even a cryptographic seed would distinguish otherwise identical substitutions.
Prefer explicit public cosmetic variation. If a uniquely patterned item needs stable
appearance through dropping, pickup and transfer, that visual identity is itself a
public fact and needs a deliberate disclosure policy. Existing body-seed semantics
are evidence to preserve, not automatic authorization to expose true identity through
a disguise or a less-disclosed contact.

An empty component list means **no components disclosed**, not "known to be naked"
or "wear the local default costume." The client needs a neutral covered proxy or
appropriate observed-form fallback for unknown surfaces. A genuinely observed bare
surface is a separate disclosed state. This distinction prevents absence from both
leaking hidden equipment and accidentally removing clothing in the renderer.

Partial visibility uses conservative semantic regions, not server-side 3D triangle
queries. If the rules cannot establish whether a component is visible, omit the
component rather than expose a full outfit from one visible cell. The exact region
vocabulary and treatment of known-but-currently-unseen apparel require content and
perception review before implementation.

## State, caching and lifecycle

| Transition | Required result |
| --- | --- |
| Full snapshot or reconnect | Rebuild the entire disclosed composition without reliance on earlier item events |
| Equip/unequip or change clothes | Replace the public description in the same observation update; preserve the apparent body's cosmetic identity |
| Cover/reveal | Remove/add only what the current observer may receive; do not retain removed components as current facts |
| Multiple visible cells | Every cell resolves to the same actor record; reject conflicting duplicate records and dangling references |
| Own body outside the eye scene | Self appearance remains available through its explicit carrier, without broadening spectator knowledge |
| Actor leaves sight | Remove current visible appearance with the actor; any remembered representation follows explicit memory policy |
| Death | Preserve disclosed body identity, but project the corpse's current visible components from authoritative state |
| Loot/remove corpse equipment | Update the corpse appearance; never resurrect an old living-actor outfit from an animation cache |
| Content changes or unavailable recipe | Apply the approved content revision/fallback policy without changing gameplay identity |

Use canonical ordering by public semantic attachment/layer/component keys. Equal
public records produce equal local cache keys; private inventory order must not
perturb them. The 3D client separates immutable recipe geometry from per-instance
fitting/motion history. Reuse unaffected body/head geometry while replacing affected
components; invalidate contact surfaces when equipped layers change. Do not carry
old collision constraints across a new composition without refitting.

For the initial protocol change, replace the actor-appearance collection as a bounded
unit inside the existing observation delta. This is simpler to validate than edits
against independent component revisions. Measure encoded full/delta bytes before
choosing a more complex representation. The collection must be applied atomically
with actor cells and corpse/self carriers; malformed updates cannot leave partial
appearance state behind.

## Catalogs, bounds and fallbacks

Prefer an immutable, validated appearance catalog for the package/region contract
already used by palettes. If runtime recipes can be created outside that catalog,
introduce an explicit catalog revision/invalidation design; changing equipment must
not depend on a palette refresh that currently only occurs on region changes.
Catalog availability grants no claim about a particular actor's equipment.

Before implementation approval, choose and test limits for components per form,
parameters per component, semantic key/value lengths, repeated attachments, total
appearance bytes per observation and total decoded allocation. Test supported
many-limbed content against those limits. Do not silently truncate a record, relax
the existing message limit, or allow recursive component expansion from the wire.
Catalog expansion also needs a bounded acyclic dependency check.

Valid but unsupported visual content uses the existing palette-gated dotted-prefix
fallback principle. Unknown attachments or recipes must preserve the disclosed facts
for text/headless output and use a safe visual proxy where possible. They must not
be silently treated as another sex-specific garment or assigned to an arbitrary limb.
Malformed types, invalid bounds, conflicting references and illegal combinations are
protocol/content errors, not fallback opportunities. The global format gate remains
a separate check from optional local visual-content support.

## Client responsibilities

| Client | Interpretation |
| --- | --- |
| Shared client model | Validate and retain the projected description, discover nested assets, apply replacement atomically and expose consistent semantic accessors |
| Text | Preserve disclosed actor names as identity/command authority; describe visible apparel with shared localized vocabulary, not raw recipe IDs |
| ASCII | Preserve player `@`, terrain and gameplay cues; use supported public cosmetic color/silhouette summaries and optional inspection text |
| 3D | Resolve body capabilities, fitting surfaces and attachment landmarks locally; render supported components without inferring hidden ones; key rebuilds on composition, not only body seed |
| Headless | Retain/report the same public record for replay and cross-client acceptance; do not require renderer-specific interpretation |

The existing [text actor consolidation](../../crates/client-text/src/adventure.rs),
[ASCII map rules](../../crates/client-ascii/src/map.rs) and
[shared vocabulary](../../crates/client-common/src/presentation.rs) are migration
anchors. None currently implements this complete apparel description.

## Appearance carrier audit

The current carriers are deliberately unequal. A reusable asset walker must follow
what each record actually discloses, not recover missing facts from live entities.

| Carrier | Current source and asset discovery | Obligation when adding visible components |
| --- | --- | --- |
| Visible actors | `Observation.visible_actors`, direct asset and appearance seed; `observation_assets` walks actor assets | Discover every disclosed component asset; keep the actor's public composition atomic across full/delta updates |
| Own body | `Observation.self_appearance`; the shared iterator walks its body asset | Preserve this carrier when the body is outside the eye scene and preserve owner/spectator disclosure |
| Ground and carried corpses | `ItemView.asset` and `ItemView.corpse.asset` are distinct; the shared iterator visits both ground items and inventory | Preserve the item presentation and disclosed body/components independently; do not reconstruct equipment from the living body's local defaults |
| Remembered ground items | `MapItemAppearance` contains only direct asset and descriptive fields, with no `corpse`, seed or component list | Retaining body/components in memory would require an explicit frozen record and policy change; today's direct-asset walk is complete for today's schema |
| Remembered actors | `RememberedMapCell` has no actor collection | Do not invent remembered actor equipment by consulting current actor records |

The [map wire records](../../crates/protocol/src/map_pages.rs) and
[server memory projection](../../crates/server/src/map_memory.rs) establish the last
two rows. Remembering a map copies disclosed values; loading that remembered map
converts the frozen values without consulting live entities or the current catalog. Therefore,
adding composition to live observations does not automatically authorize adding it
to remembered maps, saves or map-page replies.

If remembered components are approved, freeze their disclosed values when observed.
An unseen equip/unequip, corpse-looting operation or catalog change must not refresh
that stored knowledge. A subsequent observation may replace it. Preserve existing
map generation/cursor invalidation, page bounds and save/reload semantics; do not
reuse live interaction handles in memory. Test one unseen-change/revisit sequence,
plus a save/reload and stale-page case, instead of multiplying every item by every
memory transition. The existing
[client memory witnesses](../../crates/client-common/tests/it/memory.rs),
[server appearance-refresh witnesses](../../crates/server/src/map_memory_tests.rs)
and [returned-floor disclosure assertions](../../crates/server/src/map_returned_floor_tests.rs)
are starting points, not evidence that remembered component support already exists.

## Equipment capability planning matrix

This is a proposed content/fitting contract, not new protocol fields or implemented
equipment support. The local [equipment preview](../../crates/client-3d/src/creatures/outfit/equipment.rs)
currently selects helm, boots, cloak and breastplate. The
[outfit record](../../crates/client-3d/src/creatures/outfit.rs) supplies other authored
pieces. Having a garment or pigment in an outfit does not make it an independently
selectable item.

| Requested category | Current preview representation | Proposed capability and composition rule |
| --- | --- | --- |
| Armor | Independent breastplate plus preset armor regions | Fit to an eligible body surface; declare covered regions and retained underlayers, separately from attachment ownership |
| Weapons | No independent selection in this equipment path | Require a compatible grip role and explicit stowed/drawn presentation; a two-handed item may constrain two grips but remains one component |
| Shields | No independent selection | Distinguish a held grip, forearm mount and back stow; do not infer the mount from the word shield |
| Helms | Independent helm replaces preset headwear and covers earrings | Require supported head occurrence/shape; declare hair and ornament coverage without removing intrinsic anatomy |
| Gloves/gauntlets | Preset hand color, cuffs and arm geometry | Require eligible hand/forelimb surface roles; finger coverage and cuff overlap need recipes, not only recoloring |
| Shoes/boots | Independent boots and preset shoes/boots | Require compatible terminal-limb shape; paired recipes enumerate supported feet rather than assume two human feet |
| Capes/cloaks | Independent travel cloak and preset shoulder cloth | Require supported drape anchors, exterior layer and motion/contact policy; reserve lining clearance over armor and straps |
| Robes | Preset robe/skirt/sleeve geometry with dedicated motion | One recipe may span torso, arms and lower-body drape; maintain leg clearance without forcing it into one rigid attachment slot |
| Amulets | Preset neck ornaments | Require neck/chest support as appropriate; chain and pendant may have different local motion but one public recipe |
| Rings | No independent selection | Require a supported digit occurrence and a disclosure decision when covered by gloves; do not transmit hidden ring existence |
| Shirts/undershirts | Preset tunic/bodice and authored layers | Require compatible torso/arm surfaces; explicit layering/coverage decides whether any part is publicly visible |

A body capability describes what can be fitted or attached, not what the actor owns.
Keep attachment role, fit surface, coverage region and layer order distinct. Multiple
items can touch the same surface without sharing a slot; a single item can span
several surfaces. Resolve incompatible occupancy in authoritative content/equipment
logic, not by relying on the order a client happens to build meshes.

For non-humanoid creatures, the body recipe supplies eligible occurrences and shape
families: a claw, hoof and human foot are not interchangeable merely because all
end a limb. A creature may support a cloak or amulet while lacking every humanoid
clothing capability. Unsupported renderers retain the disclosed semantic item and
use the approved fallback; unsupported body/item combinations are diagnosed during
content or equipment validation. Do not silently retarget to a different limb,
replace the wearer body, or treat missing capability as a request for a default.

## Concrete review defaults and migration boundary

These are recommended decisions for approval, not implemented behavior or approved
field names. The next developer should not have to infer them from renderer code.

| Decision | Recommended initial choice | Deliberately deferred |
| --- | --- | --- |
| Disclosure and memory | Project only currently disclosed surfaces/components; preserve the separate self carrier. Do not add actor apparel or nested corpse composition to remembered maps in the first change | Remembered apparel requires a separate frozen-knowledge and save-format decision |
| Repeated anatomy | Address an attachment by a public body-catalog role plus occurrence, stable within that apparent body recipe | Dynamic attachment namespaces or private limb/entity IDs |
| Record and updates | One shared appearance value for body, disclosed coverage and visible components; replace complete actor entries atomically in the existing observation transaction | Independent component event streams, component revision numbers and incremental mesh instructions |
| Catalog lifetime | Use immutable package/region catalog content under the existing palette lifecycle | Runtime recipe creation and independent catalog revisions |

### Implemented first slice (protocol 64)

Under the maintainer's authorization, the four defaults above are adopted for this
slice. Each disclosed creature carries a bounded, canonically ordered `worn` list on
its actor entry, and the observer's own `self_appearance` carries the same list.
Corpses carry an empty list. Each entry names the rules slot, its occurrence among
that body's sockets of the slot, the disclosed item class, the unidentified
appearance name and the palette asset. That is no more than the item would show
lying on the ground.

- **Visibility.** Only equipped weapons and armour are disclosed. Rings, amulets
  and every other class stay hidden, so wearing or removing them changes nothing
  an observer receives. There is no coverage rule yet: a cloak does not hide body
  armour. Visibility is derived from the item class, so neither rules nor saves
  gained a field. Per-slot visibility and coverage would need a content and save
  decision.
- **Disclosure.** The list exists only for the Creature disclosure. Anonymous
  contacts, guises, hallucinated figures and less-disclosed figures carry none.
  Private facts, such as
  enchantment, curse, true name or which of two identical-looking items is worn,
  cannot change the record.
- **Updates.** The list is part of the actor entry, so existing deltas replace it
  atomically with the actor. No component events or revisions were added.
- **Clients.** Text describes a figure's visible armour and weapons on
  examination, ASCII adds them to the look line, headless reports the record, and
  3D maps slots onto its independent equipment preview pieces. An empty list
  means nothing is disclosed: the 3D client then keeps the body's own costume
  instead of inferring absence.

Remembered apparel, coverage, unsupported-body capability errors and catalog
revisions remain deferred.

An occurrence is a semantic position in the apparent body recipe, not an index in
an inventory or in whatever meshes a renderer happens to create. For example,
`hand` occurrence 2 can refer to the third catalog-defined hand on a many-armed
body. Removing or concealing another hand must not renumber it. A component may
reference multiple occurrences: paired boots or a two-handed weapon remain one
component with a bounded list of supported attachment references. Content validation
checks that each reference is meaningful for that apparent body and recipe. A
renderer that lacks a recipe must preserve the disclosed semantic description;
it must not retarget the component to an arbitrary supported limb.

### Worked disclosure and replacement examples

The following records are conceptual, not accepted wire JSON:

```text
Observed appearance A:
  apparent body: public humanoid recipe and permitted cosmetic identity
  disclosed coverage: torso covered; hands covered; feet covered
  visible components: travel cloak; gloves; boots
```

A hidden ring is absent from A. Replacing that ring with another mechanically
different but equally hidden ring leaves A byte-for-byte unchanged after canonical
serialization. The owner's inventory observation may change independently; it must
not create a visual component token or an appearance-only revision for spectators.
Removing the glove produces appearance B only when that observation discloses the
hand: the visible glove component disappears, the ring may become visible, and hand
coverage is replaced in the same update. B never consists of "A plus an item event"
that a reconnecting client would need to replay.

A multi-cell actor has one composition entry associated with its disclosed actor
target. Its visible cells all refer to that actor, so a cloak replacement is not
copied as contradictory per-cell clothing. The same appearance value type serves
`self_appearance` and nested corpse appearance; those carriers retain their own
identity and disclosure rules. It is acceptable to repeat equal values across
carriers initially. Do not introduce cross-carrier alias IDs or reference counting
on the wire simply to deduplicate a few records. If consistency is required between
two carriers in the same observation, validate it before applying that observation.

When an actor becomes a corpse, project the corpse from the currently disclosed
state. Preserve only cosmetic identity that the observer is permitted to connect.
Subsequent looting replaces the corpse's visible component list. A client cannot
restore the living actor's cached outfit as a default. Anonymous or disguised
presentations never gain the true body's recipe or seed through this transition.

### Bounded admission and atomic application

The current [codec](../../crates/protocol/src/codec.rs) limits responses and retained
state to 16 MiB; the client also applies the response limit to WebSocket messages
and frames. These existing ceilings are not a proposed appearance budget and do
not prove acceptable decoded allocation or model-construction cost.

Before choosing new limits, measure one ordinary fully equipped humanoid, one
supported many-limbed body, one composition at each proposed component/parameter
limit, and a dense visible scene containing those records. Record encoded full and
delta bytes, decoded allocation, retained-state size, catalog expansion and client
construction/refit cost. This small boundary set replaces a cross-product of every
body, outfit, item and camera. Choose per-record counts/string lengths and aggregate
limits together: many individually valid records can still exceed the observation
budget. Do not silently truncate visible components, increase the global transport
ceiling, or let the client independently decide which disclosed items to omit.

The implementation sequence within one received update should be:

1. Bound sequence lengths, strings and aggregate allocation during decoding, before
   retaining or expanding nested values. Reject duplicate keys and malformed types.
2. Validate structural role/occurrence syntax, typed component parameters and
   canonical ordering. The server validates body/recipe compatibility against its
   authoritative catalog before projection. The client checks available catalog
   constraints now; absent palette content is a bounded pending dependency, not
   proof of either compatibility or malformed content.
3. Build the candidate shared observation, applying complete appearance replacements
   together with actor/corpse/self carrier changes. Validate all references and the
   retained-state budget before committing it.
4. Commit that shared observation once. Discover missing palette assets from the
   committed disclosed values. While loading, retain bounded semantic facts and use
   the approved pending presentation; do not retain obsolete worn components.
5. Before expanding a newly available catalog recipe, enforce dependency/count bounds
   and reject cycles. Resolve all affected fitting/attachment requirements before
   swapping the presented composition. Valid content unsupported by a renderer uses
   its approved fallback; an invalid catalog relationship is an explicit content
   error, not permission to retarget or improvise. Renderer failure must not partially
   rewrite the shared observation or manufacture an equipment fact.

The server needs the corresponding bounded projection before encoding. If legitimate
supported content can exceed the chosen observation budget, resolve that content or
projection-policy conflict explicitly; retries of the same oversized state are not
a recovery strategy. Tests should exercise each boundary once, plus malformed
atomic replacement and hidden-item indistinguishability, rather than enumerate all
compositions. No numeric appearance-specific limit is approved by this document.

## Acceptance scenarios and implementation order

1. Preserve the new baseline regressions for self and corpse appearance discovery.
   The shared iterator and connection repair are covered, including distinct item/body
   assets and no repeated requests for an unresolved asset. Audit any newly introduced
   carrier, including future remembered appearance, before extending the schema.
2. Approve the disclosure regions, repeated-attachment semantics, representation and
   catalog/budget decisions. Only then change protocol/content/save identities where
   the accepted change actually requires them; do not add a permissive compatibility
   parser or start a new milestone implicitly.
3. Implement one server projection with fixtures for two observers, owner/spectator,
   partial sight, anonymous contact, disguise, covered ring, visible cloak and visually
   identical items with different private mechanics. Compare canonical public records,
   not only screenshots. Preserve the existing equipment-redaction session regression.
4. Test full/delta equivalence, malformed atomic-update rejection, reconnect, region
   palette changes, repeated actor cells, actor death and corpse looting. Check wire
   bytes and retained client state for hidden facts.
5. Implement shared model adaptation and all client interpretations in the same change.
   Use one disclosed fixture to compare text descriptions, ASCII cues, 3D resolved
   composition and headless records; each client must retain its existing gameplay cues.
6. Extend the real server-to-3D process fixture to equip/cover/reveal and render the
   resulting update, alongside real text/ASCII/headless process tests. Offline review
   overrides are not evidence that server-driven composition works.
7. Measure bounded worst-case message/decoded sizes and release construction, refit,
   cached reuse and motion costs using the established comparison harness. Add focused
   known conflicts plus seeded samples; do not exhaustively multiply the catalog.

For review, the four open decisions are: conservative visibility regions and memory
policy; semantic attachment roles for repeated anatomy; exact shared record shape and
resource limits; and immutable catalog versus explicit runtime catalog revisions.
The recommendation above narrows these decisions but does not silently approve them.
