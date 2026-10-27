# Baleful Polymorph: ownership and opt-in integration

This change implements only indefinite **baleful** transformation. Temporary forms,
expiry, cross-mode replacement and final numeric balance remain future work.
The initial wand is opt-in: no weighted stock, starting kit or ordinary loot pool
is extended. Its timing and charge range reuse the existing wand defaults; neither
that fixture convention nor the three-form review roster is final balance approval.

## Builder ownership

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
them. The effective ordinary Strength/Speed subtotal caps at the normal limit
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
base body cannot fit stays suppressed with a persisted pending return. Legal later
placement retries without resetting injury, resources, purchases, charges or RNG.
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

`catalog/bodies.toml` now enrolls all **26 NPC types**. The existing 22 racial
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

Only **emu, snake and troll** are presently enabled as opt-in destinations; complete
package enrollment is not approval of a live weighted form roster. Canonical
undead/construct types and incorporeal subtypes are rejected as destinations, even
when an author attempts to mark them eligible. Uniqueness and boss status have no
independent catalog metadata yet: eligibility is an explicit author attestation
that neither applies. Phasing alone is not proof of incorporeality. Ambiguous mixed
templates and conflicting fixed talent variants still require explicit authoring
classification rather than silent conversion.

## Versioned boundaries and verification

Necessary save/content changes are authorized as strict current-only versions,
with no old-save importer or silent legacy ownership defaults. The protocol change
was separately approved and applied as its own protocol bump at checkpoint `3ad2ee66`:
accurate changed-only diagnostics and `DirectionOrSelf` affordance plus
common/Text/ASCII consumers. Old clients are rejected, not silently adapted. Binary framing and
unrelated generator/report formats do not change. Owner-only advancement growth
also requires `body_package_hit_dice`: explicit null identifies an unowned build;
an integer is the full authored active-package HD count, not surviving body rows.
This permits truthful zero-personal-HD advancement and bounds package plus personal
restoration losses without exposing form IDs or granting restoration authority.
Missing fields, invalid package bounds and overflowing counts are rejected.

Current-format fixtures are produced by genuine application and save writers, not
hand-edited compatibility conversions. All 44 authored packages and the protected
source-only baseline comparator validate with the current compiler. The current
protocol sample records real client/server traffic; the six-case save 61 writer is rerun without
recording enabled to verify byte-for-byte current serialization.

The ordinary controlled Wraith route covers paid purchase, personal-row drain and
XP-funded `ChooseAgain` restoration: exact HD/Health identity returns at earned XP
215 / spent XP 200, with an empty choice slot and retained purchase high-water
mark, not the drained Lore purchase. A separate explicitly privileged-builder
witness covers legal allocation while an Authored donor is drained, then restoration
of six retained ordinary receipts with effective Strength five. This is not a claim
that the ordinary advancement UI offers attribute allocation.

The regression suite also covers strict current-record rejection, zero-personal-HD
advancement disclosure, original Dragon funding/history across forms, cap-aware
projection and fresh acquisition, pending-return contact ordering, and unchanged
non-Polymorph actors. Cosmetic Hallucination vectors use the independently checked
26-creature/81-object roster; the new maple-wand label changes cosmetic selection,
not factual item class or action authority.

Publication still requires the repository's local aggregate and final-head hosted
gates. The explicitly approved task-scoped 100-method local graphical deferral
includes the genuine OS-key Polymorph direction-or-self test; those cases are
**deferred, not passed**. Required hosted Windows/Linux graphical checks remain
mandatory, and model tests are not native acceptance. Verification receipts must
distinguish selected checks, complete aggregates, failures and deferrals.

The preceding consumables PR #195 merged at `feae51f`; this separate Polymorph
branch is based on that merged main revision. Temporary Polymorph, cooperative
target-owned consent and live weighted form/item tuning remain outside this phase.
