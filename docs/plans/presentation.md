# Presentation and atmosphere: design plan

Presentation is everything that changes how things look, sound or read without
changing how the game plays: environments, creature and item appearances,
atmospheric text, glyphs, animation choices. This plan sets one model for it
across the server, the scenario format and every client. It is unimplemented.
Approving the plan doesn't start a milestone, and the steps that change the
scenario package format or the protocol each need explicit approval.

## Goals

- Varied, interesting environments, where a scenario may have an opinion and
  otherwise gets deterministic defaults.
- Varied creature appearances that stay consistent per individual: the same
  creature always looks the same, on every client and after reloading, and its
  corpse matches it.
- One common vocabulary of TOR things, with a way for scenarios to describe
  their own (a "dawn seal" isn't a common TOR thing).
- Per-client overrides from a scenario: the Rogue scenario draws common TOR
  creatures with Rogue's letters in the ASCII client, where other scenarios use
  the NetHack-inspired defaults.
- Players choose their own character's appearance.

## Decisions

- The common vocabulary is **built in**, versioned like the general rules, not
  an importable package.
- Scenario descriptions and per-client overrides travel **over the palette
  channel**.
- Disclosed gameplay states (wounded, frozen, burning, status conditions) are
  **state cues**: facts, kept apart from cosmetic traits.
- Scenarios may add new trait **values with a fallback parent**.
- **User preferences** come first in override precedence.
- The built-in vocabulary describes places with **traits only**; scenarios may
  add authored atmosphere lines.
- Telling individuals apart by appearance is **acceptable**. It only reflects
  what the player has seen, much as NetHack lets players name individual
  monsters and recognise them later.
- Player appearance choices have **limits**, set per species.
- Words are **English for now**, structured so localisation can follow without
  rework.

## Existing foundation

Build on the current [asset palettes](../reference/protocol.md#asset-palettes)
rather than a parallel mechanism:

- Scenarios name assets with dotted lowercase identifiers
  ([scenario packages](../reference/scenario-packages.md#assets)). Cells,
  doors, items and actors carry an optional `asset`, attached only to disclosed
  things; a concealed item carries its appearance pool's asset, never its
  archetype's.
- Clients resolve assets through dotted-prefix fallback
  (`terrain.floor.cave`, `terrain.floor`, `terrain`, then their own look).
- Themes belong to zones. A client's palette is forecast from package structure
  alone, so it can't reveal what a region holds; the validator requires every
  shown asset to be in its region's themes.

Gaps today: assets are bare names with no meaning attached; the Rogue scenario
names none; the ASCII client keeps the palette but doesn't draw with it; and
no client yet varies an individual's look, so every creature of a kind looks
alike.

## Layers and invariants

Three layers stay separate:

1. **Identity and rules**, owned by the server.
2. **Appearance**: the asset, its vocabulary entry and scenario extensions,
   the individual's appearance seed, and disclosed state cues.
3. **Rendering**, owned by each client: words, glyphs, models, animation.

Invariants:

- Presentation never feeds the simulation and never draws on simulation
  randomness. Defaults may be derived from rules (a presentation size from the
  rules' displacement size); nothing flows back.
- Presentation is deterministic: the same inputs give the same look.
- Presentation never implies what hasn't been disclosed. Atmosphere may not say
  "you smell something" for a hidden creature. What a creature visibly holds or
  wears comes from disclosed equipment, not from its appearance.
- Lighting is gameplay (sight), so the server owns it. A client's mood may tint
  and decorate, but never shows a cell as lit that the server treats as dark,
  or the reverse.
- Every lookup has a fallback, ending at the client's own look.

## Common vocabulary

A built-in tree of asset identifiers. Each entry has:

- a **parent**, for fallback (`creature.equine.quagga` falls back to
  `creature.equine`, then `creature`);
- **traits** from the fixed trait list below;
- **words**: a noun with its forms (singular, plural, article) and adjectives,
  each as a message key with English text;
- a default **ASCII glyph and colour**;
- a **variation spec** saying which traits vary and how far.

A client that knows an entry draws it fully. One that knows only an ancestor
draws the ancestor, and the text client says "a horse-like beast" rather than
nothing.

### Localisation readiness

English only for now. To keep localisation cheap later: words are message keys
resolved through a locale table, never concatenated English fragments; nouns
carry their grammatical forms as data; client prose composes from keyed parts
with per-language templates; and assets and traits are identifiers, never
display text.

## Traits

### Tests for a trait

1. **Every client can express it.** Text needs words, ASCII a glyph and colour,
   3D geometry or paint. A trait text can't put into words is renderer detail
   and stays inside a client.
2. **It's cosmetic**, by the invariants above.
3. **It composes rather than enumerates.** A centaur is a humanoid upper body
   on an equine lower body, not a "centaur" body plan; a winged horse is an
   equine with wings. A fixed list then covers creatures nobody has written yet.
4. **Its kinds are fixed.** Trait kinds are built in and versioned. Scenarios
   add values with a parent value to fall back on ("dawn-gold" falls back to
   gold).
5. **It neither contradicts nor leaks** disclosed facts.

### Creatures

- **Form.** Body plan as composable parts (humanoid, equine, avian,
  serpentine, floater, arthropod; attachable wings or a fish tail). Size class,
  build (slender, stocky, hulking, gaunt), posture (upright, hunched,
  crouched, prowling).
- **Features**, each with count, shape and size. Head: horns, ears, beak, tusks
  or fangs, eyes (count; style such as cartoon, glowing, hollow, compound).
  Body: tail (whip, switch, bushy, stub), mane or crest, claws, tentacles.
- **Surface.** Covering (skin; short, long or shaggy fur; feathers; scales;
  chitin or plates; stone; metal; ice; flame; smoke; bark). Named primary,
  secondary and accent colours, so text can say "chestnut". Pattern (solid,
  stripes, bands, spots, dapple, patches, countershading) with a region
  (forequarters, back, whole body). Finish (matte, glossy, metallic, wet,
  translucent).
- **Glow.** Accent colour, where (eyes, horn, runes, core, whole body) and
  intensity.
- **Condition.** Cosmetic states such as ragged, scarred, rotting, frosted,
  smouldering, pristine. These are not state cues: a rotting zombie is always
  rotting.
- **Attire, for humanoids.** A garment per slot (hood, helm style, robe,
  jerkin, cloak, boots), material, trim colour, heraldry (emblem and colour).
  Worn equipment that the server discloses overrides attire in its slot.
- **Motion and temperament.** Gait (walk, lumber, slither, hop, float,
  scuttle, gallop) and demeanour (twitchy, calm, menacing, regal), which choose
  idles and antics. Abilities map to animation categories (strike, thrust,
  cast with an element, ward), replacing matching on ability names.
- **Senses.** Voice (hiss, grunt, neigh), smell and texture, for text and
  future audio.

### Items

Shape (blade, haft, vessel, disc or medallion, book, scroll, ring, garment,
gem), material, motif, glow and condition. A concealed item takes traits only
from its appearance pool.

### Places

Per theme: material family (natural cave, worked stone, crypt, ice,
root-bound), wetness, temperature cue, flora and moss, minerals and crystals,
mood tint, ambient effects (drips, mist, embers, motes) and sound beds.

### Variation spec

Any trait can be fixed, or given an allowed set or range with weights. A
scenario can narrow it ("every snake here is green") or widen it.

### State cues

A separate list for disclosed gameplay states: wounded (by disclosed
severity), dying, dead, frozen, burning, poisoned and other disclosed
conditions. Each cue has words and a suggested visual treatment, and clients
show cues only as the server discloses them.

## Scenario extensions

A package may:

- add entries under existing parents, with traits, words and a glyph (for
  example `item.seal.dawn`: parent `item.seal`; gold, sunburst motif, amber
  glow; "a gold medallion stamped with a sunburst");
- add trait values with a fallback parent value;
- narrow or widen variation specs;
- give themes atmosphere traits and authored atmosphere lines;
- give per-client overrides (below).

The validator checks parents, value fallbacks and the existing rule that every
shown asset is forecast by its region's themes.

## Per-client overrides and precedence

Resolution, highest first:

1. the user's own client preferences (for example a colour-blind ASCII palette
   or a personal glyph set);
2. the scenario's override for this client;
3. the scenario's own description of the thing;
4. the common vocabulary's default;
5. the client's built-in fallback.

The Rogue scenario keeps the common TOR creatures and adds an ASCII override
mapping them to Rogue's capital letters (`creature.equine.quagga` to `Q`,
`creature.bird.emu` to `E`). Other scenarios get the NetHack-inspired
defaults.

## Delivery over the palette channel

The palette channel already carries cosmetic, structure-forecast data separate
from observations. Extend it to carry, for the assets in a client's palette,
the scenario's descriptions, value extensions, variation specs, atmosphere and
the overrides for that client's kind. Everything stays derived from package
structure, never from region contents, so it reveals no more than palettes do
today. Clients cache by palette revision, and the existing missed-revision and
missing-asset requests cover recovery.

## Appearance seeds

Each individual's varied traits are picked from its asset's variation spec by
an appearance seed. Actor targets are observer-scoped, so they can't seed a
look shared between clients; the server discloses a separate opaque appearance
seed, derived from the actor's stable record identity and a package salt. The
seed:

- is the same for every observer, session and reload;
- passes to the creature's corpse, so the body matches;
- reveals nothing new: it is attached only to disclosed things;
- for a concealed item, comes from the appearance pool, never from the true
  item, so look-alikes stay alike.

### Pre-building from the palette

A palette names the creature kinds a client is about to need, so a client
can build their models before any appears. The individual's seed isn't in
the palette, but a graphical client can split a model into expensive parts
that vary along only a few traits (a head by sex, build and face shape, a
body by sex and build) and cheap ones (colours, hair, emblems). Building
every expensive part for each kind the palette forecasts, spread across
idle frames, means no individual costs a build when it appears; it is
assembled from the cache.

## Player-chosen appearance

- Each species' entry declares what a player may choose and within what limits
  (allowed colours, hair, attire, build).
- The choice is stored on the server as a disclosed cosmetic attribute, so
  every client and spectator sees the same character.
- Chosen traits replace seeded ones; anything not chosen is seeded.

## Atmosphere

- Themes carry place traits. With no scenario opinion, clients choose from the
  vocabulary's defaults by deterministic place seeds (region and cell keys), so
  a place always looks and reads the same.
- Text clients compose prose from traits and use scenario lines where given,
  deterministic per place.
- Graphical clients choose materials, decoration, effects and tint from the
  same traits.

## Shared client library

Vocabulary lookup, extension merging, precedence, appearance-seed selection,
state cues and place-seed helpers live in shared client code
(`tor-client-common` or a crate beside it). The text, ASCII, headless and any
graphical clients then agree on what a thing is.

## Testing

- The same seed and spec give the same traits on every client.
- A corpse resolves to the same appearance as the living creature.
- Precedence resolves user, scenario-client, scenario, common and fallback in
  order.
- Concealed items with the same pool get the same appearance.
- Nothing presentation-only reaches simulation inputs, and palette contents
  stay independent of hidden region contents.
- Unknown assets and values fall back without failing.

## Order of work

1. Name assets in the Rogue scenario's creatures and items, and have clients
   draw from assets instead of names. Client and content only.
2. Write the common vocabulary v1 and trait list as a built-in registry with
   reference documentation.
3. With format approval: scenario descriptions, value extensions and per-client
   overrides on the palette channel; then the Rogue ASCII letters.
4. Appearance seeds and variation specs, with corpse inheritance.
5. State cues.
6. Place atmosphere traits with deterministic defaults; authored scenario lines.
7. Player-chosen appearance within species limits.

## Open questions

- When can a player change their appearance: only at character creation, or
  later too?
- What are the limits for each Rogue species and for human characters?
- Should overrides for graphical clients (style, colours) use the same
  per-client override mechanism as ASCII glyphs?
- Is audio in scope for the sense traits now, or only reserved?
- How is the vocabulary versioned against saved games and scenario packages?
