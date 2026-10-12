# Scenario compilation boundaries

The scenario format describes content and world structure. It does not implement a
second action, combat or scripting engine. The [package reference](../reference/scenario-packages.md)
owns supported syntax and limits; [status](../status.md) owns version identities.

## Source, compiled inputs and mutable state

1. **Source frontend:** strict TOML, bounded local modules, explicit namespaces and
   imported rule catalogs. Unknown fields, duplicate identities and unresolved
   references fail. Source locations and raw closure hashes are diagnostic and
   provenance data, distinct from simulation identity.
2. **One compiler:** resolve imports and stable-ID overrides, lower typed catalog
   views and generator presets, check references and resource bounds, and validate
   geometry through the existing world/simulation constructors. Python authoring
   and arena tools invoke the Rust resolver instead of reproducing its semantics.
3. **Immutable compiled inputs:** canonical resolved catalogs, authored region
   inputs, compact procedural descriptors and versioned generator contracts.
   Validation publishes a complete artifact before its certificate. Content export
   copies this input bundle; it does not create potential procedural terrain.
4. **Runtime:** resolve a stable logical address from the world seed, prepare a
   bounded group on demand, and publish through the normal region lifecycle.
   A named stream depends on seed, generator contract and stable identity, never
   exploration order or the source file's position. Revisit restores mutable state
   rather than regenerating it.
5. **Save:** retain versioned compiled inputs and mutable records. Unvisited
   authored inputs may remain in a matching immutable bundle; a development
   package without one captures its authored inputs once under a size bound.
   Original source filenames and rule import availability are not recovery inputs.
   Retain compiled bundles needed by existing saves: revalidation publishes a new
   identity and does not garbage-collect older bundles. Automatic retention policy
   is separate from compilation because deleting a bundle can strand unvisited
   authored content.

Explicit authored graphs remain supported. Grid-chain families are one bounded
procedural operator, not the universal shape of a scenario. Their logical ports
compose with authored chambers and other families. Random port ownership and
placement are separate from connection identity. A seeded finite extent is a
pure topology decision; an open family's operational budget is a resource limit,
not a secretly generated final floor or victory condition.

## Syntax decision

TOML remains the sole authoring frontend because the hard examples can be
expressed as named catalogs, tagged operators and references without executable
expressions. Keeping one frontend avoids divergent coercion and error rules.
This is a schema decision, not a commitment that TOML is a runtime or save format.

- JSON is appropriate for canonical machine-generated compiled inputs and tool
  output. Its lack of comments and repeated punctuation make it less convenient
  for these hand-authored catalogs.
- YAML anchors would add another implicit reuse/merge mechanism alongside explicit
  imports and overrides; they would not solve identity or domain modeling.
- A custom DSL or embedded language would require parsing, typing, tooling,
  determinism and resource contracts without evidence that current operators need
  it. Add a typed operator when a concrete general capability requires one; keep
  arbitrary scripting a separate design decision.
- Maintaining equivalent TOML, YAML and JSON authoring frontends now would expand
  the compatibility and diagnostic matrix without improving the compiler boundary.

## Validation and evolution

Validation proves bounded structural invariants and authored geometry, then records
sampled generator coverage. Sampling does not prove every seed. Each runtime group
still undergoes construction and invariant checks before publication. Limits apply
before expensive work: source closure bytes, metadata size, address arithmetic,
validation cell work, graph queries and retained preparation all have explicit caps.

The source, compiled-input envelope, generator contract and save archive have
separate version responsibilities. Older prerelease saves and scenario formats are
explicitly rejected in this transition. Bundled sources and golden save fixtures
move together. Rule-document serialization is a versioned compiled/save contract;
a change to that vocabulary requires compatibility review even if a Rust derive
would accept it automatically. No historical importer or gameplay expansion is
implied by the new format.

Required evidence includes source-order/comment identity tests; bad references,
unknown fields and path safety; integer overflow and work bounds; equal random
access under different exploration orders; authored/procedural composition;
source deletion and compiled-bundle relocation; corruption failing closed;
materialize-once mutation recovery; and real compiler/server/client processes.
See the [testing policy](../development/testing.md) for final gates.
