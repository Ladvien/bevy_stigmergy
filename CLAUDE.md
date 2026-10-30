# bevy_stigmergy — notes for agents

Stigmergic influence fields over a fixed cell grid: `N` scalar channels that agents deposit into, which evaporate, diffuse between floor cells, and can be sampled or climbed — plus a vectorial rally pheromone for tracking a moving target.

## Source of truth

This repository, [`Ladvien/bevy_stigmergy`](https://github.com/Ladvien/bevy_stigmergy), is canonical since 2026-10-23: it moved to Bevy 0.20 while `foundation_vs_slop/crates/bevy_stigmergy` stays frozen on 0.19. Do not mirror over it with `scripts/mirror_crates.sh`. MIT OR Apache-2.0.

## Build and test

A leaf — `bevy_math`, nothing else in the default build — so it builds and tests on its own: `cargo test -p bevy_stigmergy`. The optional `serde` and `reflect` features add only derive impls on value types (`ChannelDef`, `RallyDef`, `CellFrame`), and `cargo test --all-features` covers them. `rayon` is a dev-dependency only (bench and tests stand in for a caller's pool with it).

## The non-negotiable: the caller owns the schedule

`tests/leaf.rs` is the ratchet. Allowed: `bevy_math`, `rayon`, and the optional `serde` and `bevy_reflect` (value-type derives only). Forbidden by name: `bevy_ecs`, `bevy_app`, `bevy_render`, `avian`, `wgpu`, `winit`, `emerge`, `foundation_vs_slop`. The crate no longer takes `rayon` (issue #6); the test still allows it, and narrowing that list is the owner's edit.

That list is not tidiness. A field like this is only reusable if the **caller** decides when deposits drain, when evaporation runs, and where both sit relative to the agents reading it — those are gameplay decisions. The moment this crate takes `bevy_app` it starts making them, and the moment it takes a game type it stops being a library at all. Widening the list is a design decision; it should cost a deliberate edit in `tests/leaf.rs`, not a passing `cargo build`.

**The crate owns no threads.** A tick runs inline on the caller's thread; a caller that wants the stencil spread over workers runs `evaporate_diffuse_with`'s chunks on its own pool. Never call into a pool from inside the crate: a Bevy worker that blocks on a foreign pool's latch parks until threads it does not schedule finish (issue #6 measured 150 ms hitches).

**Diffusion must stay thread-count-independent.** It is a pure stencil over disjoint output slots: each cell reads the *previous* grid and writes its own slot, with no cross-cell float reduction, so the result is identical for any number of threads. (Dourvas, Sirakoulis & Adamatzky 2019, *IEEE Access*, parallelise the same reaction-diffusion CA on exactly this basis.) Any change that introduces a shared accumulator across cells breaks bit-reproducibility in every consumer downstream — including an offline search whose archives are compared for equality.

## Rules

- **Bevy 0.20 is pinned.** Read the vendored `bevy_math-0.20.0` source, not bevy.org — that documents `main` and has been wrong for this pin more than once.
- **No `unwrap()`**, no `expect` on caller data, no panicking index. Deposits arrive from gameplay and cell coordinates arrive from callers; both must be handled, not asserted.
- **One path per feature.** No fallbacks, no legacy shims, no stub placeholders. A field with two update paths produces group behaviour nobody can trace back to a rule.

## Where the boundary falls

Three things belong to the caller, and each is a thing this crate deliberately cannot do:

- **World space.** The grid thinks in cells. Whatever maps your world coordinates onto them — and back — is yours, and it is usually the same type that already knows where the walls are. `CellFrame` is a plain value type for the common case — an axis-aligned grid of uniform cells, `world_to_cell` / `cell_centre` / `cells()` / `covering` — offered because nearly every caller writes the same formula and two copies of it drift. It is not the mapping: a hex sheet, a rotated board or a camera-relative projection ignores it and everything still works. The entry points keep taking `IVec2`, and they always will.
- **The schedule.** When deposits drain, when evaporation runs, and where both sit relative to the agents reading the field are gameplay decisions. This crate registers nothing.
- **What a wall is.** Diffusion moves value only between floor cells, so the field routes around obstacles for free; but the floor set is something you hand in, not something it discovers.

A wrapper that holds those three and delegates the rest is the shape this is meant to be used in.
