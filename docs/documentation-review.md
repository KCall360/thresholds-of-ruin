# Documentation review — 2026-10-08

Reviewed the published main baseline, separately from the original interactions
checkout, and incorporated the accepted TOR/Rogue interview requirements.

## Scope and source of truth

Reviewed the root README, contribution/agent instructions, all current guides,
planning documents, historical refactor evidence and the new Rogue specification.
Checked runtime assertions against code, scenario manifests, tests, command-line
parsers and CI configuration. The roadmap remains the single current-format
source; guides describe implemented behavior, plans describe accepted future work,
and reference specifications describe their pinned external baseline.

## Corrections

- Distinguished implemented per-region generation/streaming from planned recipes
  and nine-region floor groups; removed stale claims that runtime streaming and
  generation do not exist.
- Reconciled sight implementation with its newly merged closeout, and marked the old sight
  model as motivation rather than current behavior.
- Consolidated the completed refactor guidance; preserved historical comparisons
  separately, including failed experiments and adverse measurement limitations.
- Updated simulation preparation/queue documentation, tutorial/victory hook intent,
  player-facing descriptions, package-validation outputs and resume options.
- Integrated shared TOR creature/effect/survival requirements and the staged
  exploration-first Rogue plan without publishing old-branch gameplay changes.
- Separated player documentation from engine internals and historical designs;
  extended file/heading-link coverage to nested documentation, with regression
  tests for nested discovery and duplicate/fenced heading handling.

## Validation and remaining boundaries

Run the repository documentation checks and publication tier under the
[testing policy](testing.md), then require full Windows/Linux CI before merging.
This review changes documentation and its checks; it introduces no gameplay or
format changes. The sight closeout is merged; the documentation review adds no sight changes. Script runtime selection,
creature balance formulas and floor-recipe schemas remain explicit design work,
not executable features. Documentation does not claim closure of performance tails.
