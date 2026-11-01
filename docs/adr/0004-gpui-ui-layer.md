# ADR 0004: GPUI Kit with a thin internal UI layer

**Status:** accepted
**Date:** 2026-11-01

## Context

GPUI is pre-1.0 and GPUI Kit pins an exact GPUI snapshot. APIs will change.
The application also needs a distinctive touch-first design that should not be
constrained by a generic component theme.

## Decision

- Use GPUI Kit 0.8.x for application scaffolding, dock layout, tables, virtual
  lists, dialogs, charts, and accessibility.
- Pin exact versions and commit `Cargo.lock`.
- Introduce `ghs-ui` at M2 with design tokens (spacing, type scale, touch sizes,
  colors) and wrappers for the components used by more than one panel.
- Keep GPUI-specific code out of `ghs-core`, `ghs-proto`, `ghs-engine`, and
  `ghs-viewer` geometry.

## Consequences

- Upgrading GPUI is a contained, scheduled task rather than a rewrite.
- The touch design can diverge from the default theme without forking
  components.
- Some duplication in wrappers is accepted to keep upgrade cost low.
