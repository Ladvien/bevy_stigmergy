# bevy_stigmergy — notes for agents

Stigmergic influence fields over a fixed cell grid: `N` scalar channels that agents deposit into, which
evaporate, diffuse between floor cells, and can be sampled or climbed — plus a vectorial rally pheromone
for tracking a moving target.

## Source of truth

The source of truth for this crate is [`Ladvien/foundation_vs_slop`](https://github.com/Ladvien/foundation_vs_slop)
at `crates/bevy_stigmergy/`. If you are reading this in a standalone `Ladvien/bevy_stigmergy` checkout,
that is a read-only `git subtree split` mirror — **changes made here cannot be pulled back**. Make them
upstream.

## Build and test

A leaf — `bevy_math` and `rayon`, nothing else — so it builds and tests on its own:
`cargo test -p bevy_stigmergy`.

## The non-negotiable: the caller owns the schedule

`tests/leaf.rs` is the ratchet. Allowed: `bevy_math`, `rayon`. Forbidden by name: `bevy_ecs`,
`bevy_app`, `bevy_render`, `avian`, `wgpu`, `winit`, `emerge`, `foundation_vs_slop`.

That list is not tidiness. A field like this is only reusable if the **caller** decides when deposits
drain, when evaporation runs, and where both sit relative to the agents reading it — those are gameplay
decisions. The moment this crate takes `bevy_app` it starts making them, and the moment it takes a game
type it stops being a library at all. Widening the list is a design decision; it should cost a
deliberate edit in `tests/leaf.rs`, not a passing `cargo build`.

**Diffusion must stay thread-count-independent.** It is a pure stencil over disjoint output slots: each
cell reads the *previous* grid and writes its own slot, with no cross-cell float reduction, so the
result is identical for any number of threads. (Dourvas, Sirakoulis & Adamatzky 2019, *IEEE Access*,
parallelise the same reaction-diffusion CA on exactly this basis.) Any change that introduces a shared
accumulator across cells breaks bit-reproducibility in every consumer downstream — including an offline
search whose archives are compared for equality.

## Rules

- **Bevy 0.19 is pinned.** Read the vendored `bevy_math-0.19.0` source, not bevy.org — that documents
  `main` and has been wrong for this pin more than once.
- **No `unwrap()`**, no `expect` on caller data, no panicking index. Deposits arrive from gameplay and
  cell coordinates arrive from callers; both must be handled, not asserted.
- **One path per feature.** No fallbacks, no legacy shims, no stub placeholders. A field with two update
  paths produces group behaviour nobody can trace back to a rule.

## In the monorepo

The game wraps this as `Stig(StigGrid<CHANNEL_COUNT>)` in `ai::field` (`src/ai/field.rs:134`), which is
where the dungeon's world↔cell mapping and `line_of_sight` live — this crate never learns about either.
Root `CLAUDE.md` and `TESTING.md` carry the project-wide rules; neither is part of this mirror.
