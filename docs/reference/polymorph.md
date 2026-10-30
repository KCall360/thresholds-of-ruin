# Polymorph and body transitions

This is the current **indefinite baleful** implementation, not a temporary-form
specification. Transformation does not provide a second life: a transformed
creature killed at zero Health stays dead. Temporary expiry, saved original-HP
reversion and cross-mode replacement are not implemented; see the
[remaining design](../plans/baleful-polymorph.md).

## Availability and device execution

The live Rogue supplemental stock includes the Polymorph device. Its initial
charge range is 3–7; the eligible destination roster is still only **emu, snake,
and troll**. There are 27 body packages including the human player body and all
26 NPC types, not 27 live destinations. Item activation and provisional weights
belong to the [live content policy](scenario-packages.md#live-rogue-content-policy).
Neither activation nor package enrollment is final balance approval.

Directional Polymorph is a **first-body, UntilObstruction contact ray**, not a
six-cell effect and not a bouncing ray. A wall or closed door stops it; the first
physical creature intercepts even if it resists or receives no change. The target
rolls Agility against the source's Lore total; a tied target total resists. A known
device supports explicit self-use and waives only that user's own resistance.
Friendly non-self targets do not waive their check.

Ordinary device use follows [shared Zap timing](magic-items.md#zap-timing):
`consume_turns × actor turn_ticks`, then action-duration scaling, with no extra
recovery phase. The ability's authored 100 preparation / 100 recovery values are
ability metadata, not the execution timing of item Zap. Item identity is reserved
at preparation start, not mere queue admission. A released miss, resistance,
blocked fit or unchanged result remains paid under the ordinary device rules.

## Builder ownership

The [progression reference](progression.md) owns the general body/personal
ledger, drain and restoration contract. This section describes its body-change use.

The existing builder remains authoritative. An explicitly enrolled actor has a
body package prefix and a stable-ID personal advancement ledger. A package owns
its species, anatomy/geometry, racial HD and innate purchases; it does not import
an NPC's name, controller, faction, background skills, inventory or personal class.
Personal HD, XP costs/milestones, training and earned attribute improvements remain
personal. Total active HD still governs combat prerequisites. Unsupported purchases
remain owned but dormant, and reactivate in any eligible form, not one locked ID.
Identical body/personal talent grants apply once without merging their funding.

The new body supplies base Strength and Speed. Intellect, Willpower, Awareness and
Presence remain personal; acquired improvements apply once. Birth Intellect and
the two background skills are retained. Player characters begin with one body HD
and one class HD: only the first body HD is maximized, while the personal roll has
a stable seed. Body changes do not reroll that seed or inflate victim XP.

Retained physical gains remain recorded even when a body cannot express all of
them. The effective ordinary Strength/Speed subtotal caps at the bundled ordinary limit
(5), magical natural Strength caps at 7, and innate modifiers fold afterward.
Under custom rules, dormant magic never reduces an already-valid higher ordinary
subtotal; it contributes again when there is headroom.
Returning to a body with room reveals the retained gains again; ownership,
funding, milestone occupancy and magical acquisition counts do not change. Fresh
attribute allocation and magical acquisition still reject gains past their
admission caps: saturation is not permission to buy hidden improvements. Initial
authored/unowned validation and mental/skill caps remain strict. A body change or
Cancellation does not fail solely because retained physical improvements overflow.

Drain removes personal HD before package HD and retains exact ownership. Package
loss currently blocks a different-body transition rather than redistributing the
loss or restoring it for free. Restoration can unblock a pending return. Stable
purchase high-water marks survive respec, drain and repeated body visits. Personal
`ChooseAgain` restoration returns the exact HD with empty choices, retaining its
purchase high-water mark; `Authored` restoration returns its original source
choices. Package restoration returns authored package choices, not personal
purchases. These are distinct ownership policies, not a universal replay of the
last drained choices.

## Atomic transition and disclosure

The server samples once from the explicit eligible roster, excluding the current
form. It never rerolls for fit. A blocked/resisted/missed release is still paid.
Body, compatible equipment, recovery context and resource deficits are staged
before one final derived state is committed. Incompatible equipment remains carried,
even when cursed; a removed held light is extinguished only at commit. Existing
fall causal provenance is preserved. Absolute injury survives the transition, so a
frailer body can die; zero HP is real death, never a protective reversion.

Cancellation suppresses and reverses in the same final fold. A living actor whose
base body cannot fit stays suppressed with a persisted pending return. Outstanding
package-HD losses can also block return until their exact receipts are restored.
Legal later placement retries without resetting injury, resources, purchases, charges or RNG.
Destination footprint preparation is load-only, never population activation.
For a pending return after movement, **old-form placement and its complete contact
chain settle first**, then the base-body return reconciles any newly occupied
contacts. This order is identical with warm and cold destination geometry. A
post-contact phase of the existing private trap-residency continuation can load the
base extent before time or another actor advances; it never repeats old contacts,
changes the selected destination or draws again. Stale anchor/frame/dependencies
reject, a relocated trap chain uses its final anchor, and a dead actor does not
return. These private continuations cannot be checkpointed mid-boundary.

Directional use contacts the first physical creature, using target Agility against
caster Lore. A known wand also supports explicit self-aim without a resistance
check. This first baleful delivery does not waive another creature's resistance:
every non-self target keeps the opposed check, including friendly actors. An
explicit target-owned cooperative-consent mechanism is deferred to later work.
Unknown items retain the ordinary directional affordance. Identification
requires an immediate discernible change, not merely payment. The new
`discernible_effect` discovery policy is admitted only on supported Polymorph
devices, not on unrelated devices, scrolls or potions; existing discovery policies
keep their previous behavior. Ordinary replies
must not expose the selected private form ID, hidden anatomy, HP or advancement.
Saved private state retains exact form and random provenance for source-free reopen.
Visible body artwork comes from the current species while transformed, including
the eventual corpse snapshot. Returning restores the original custom birth artwork;
personal name is never replaced. Missing form artwork projects no asset instead
of incorrectly displaying the old body. Ordinary visibility and hallucination
projection still govern what any other observer receives.

## Rogue body classification

[`catalog/bodies.toml`](../../scenarios/rogue-exploration/catalog/bodies.toml) enrolls all **26 NPC types**. The existing 22 racial
packages retain their authored racial rows and geometry. Explicitly approved
Hobgoblin and Orc bodies each add one racial HD while retaining their one personal
Warrior HD (total 2 each); Centaur adds one racial HD while retaining four personal
Warrior HD (total 5). No personal class is converted into innate racial advancement.

Dragon retains its ten racial body HD and innate grants. Its original Willpower
choices at ordinals 4/8 (+2 total), and additional Breath selections, are personal
receipts with their original funding provenance; they do not create personal HD
or an additional points budget. A smaller active body does not suppress these
personal grants. Actual drain of a grant's original donor row does suppress that
grant; restoration and inactive-body provenance must not launder that loss. The
existing depth 15/20/25 authored purchase schedule remains personal growth,
preserving its trigger depths rather than treating its rows as body grants.

Only **emu, snake and troll** are presently enabled as destinations; complete
package enrollment does not make every package an eligible destination. Canonical
undead/construct types and incorporeal subtypes are rejected as destinations, even
when an author attempts to mark them eligible. Uniqueness and boss status have no
independent catalog metadata yet: eligibility is an explicit author attestation
that neither applies. Phasing alone is not proof of incorporeality. Ambiguous mixed
templates and conflicting fixed talent variants still require explicit authoring
classification rather than silent conversion.

## Durable records and privacy

Current-only save/content/protocol boundaries are listed in [status](../status.md),
not duplicated here. The owner-only advancement growth record includes
`body_package_hit_dice`: null means an unenrolled build; an integer is the full
authored active-package count, not just surviving package rows. It permits truthful
zero-personal-HD disclosure and bounded restoration accounting without exposing
form IDs or granting restoration authority. Missing fields, invalid package bounds
and overflowing counts reject rather than invoke a legacy ownership default.

Current private state retains stable body/personal ownership, exact Health roll
provenance, inactive body snapshots, losses, cancellation suppression and pending
return. Source-free reopen uses those records; it does not re-import a package or
resample a form. Public action replies remain narrower than private persistence.

## Implementation and contract witnesses

These links locate the current implementation and test assertions; they do not
claim that a test run was performed as part of a documentation edit.

| Boundary | Source and witnesses |
| --- | --- |
| Ownership, retained attributes, exact losses, cap-aware projection | [body composition](../../crates/simulation/src/creatures/body.rs), [derived attributes](../../crates/simulation/src/creatures/derive.rs), [body tests](../../crates/simulation/src/creatures/body_tests.rs) |
| Atomic fit/equipment/injury commit and pending-return ordering | [Polymorph lifecycle](../../crates/simulation/src/polymorph.rs), [movement lifecycle tests](../../crates/simulation/src/abilities/movement_tests/lifecycle.rs) |
| One selection excluding current; no fit reroll; known-only self use; Agility versus Lore; discernible-only discovery | [Polymorph delivery tests](../../crates/simulation/src/abilities/polymorph_tests.rs) |
| Paid private continuation and source-free reopen | [server session tests](../../crates/server/src/polymorph_session_tests.rs), [process package](../../scripts/polymorph_process_package.py) |
| Current art/identity projection | [body observation tests](../../crates/simulation/src/observation_body_tests.rs) |
| Body enrollment, explicit eligibility and device data | [body catalog](../../scenarios/rogue-exploration/catalog/bodies.toml), [item catalog](../../scenarios/rogue-exploration/catalog/items.toml), [ability override](../../scenarios/rogue-exploration/rules.toml) |

Useful named witnesses include
`one_uniform_form_selection_excludes_current_and_has_no_geometry_reroll`,
`explicit_self_polymorph_is_known_only_and_waives_only_its_own_save`,
`polymorph_ray_keeps_exact_agility_vs_lore_save_and_never_checkless_targeting`,
`paid_polymorph_identifies_only_immediate_visible_changed_form`, and
`paid_self_polymorph_keeps_personal_ownership_and_reopens_without_source_or_reroll`.
The builder witness for allocation while an Authored donor is drained does not
mean the ordinary advancement UI exposes arbitrary attribute allocation.
