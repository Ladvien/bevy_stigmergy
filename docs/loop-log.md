# bevy_stigmergy loop log

## Iteration 0 — 2026-10-30 — loop setup

By the owner's session, not the loop. The repository gets its Wiggum loop (`.omp/commands/build-loop.md`), on sib beside the bevy_gooey loop and bevy_emerge's w8. The crate is at `v0.2.0` on crates.io (2026-10-25: Bevy 0.20, no panicking index, `CellFrame`, restore).

### Ranked backlog

Take these in order, after any bug and any filed issue (section "Filed issues come before new features"):

1. **#6** (`consumer`, `consumer:sanctuary`, `performance`): `evaporate_diffuse` runs its diffusion on rayon's global pool, so a Bevy worker that calls it parks while another pool does the work; Sanctuary measured social-field ticks hitching up to 150 ms. Sanctuary's perf loop waits on this fix and a pin move. Prove the stall before and after with the caller's wall clock from inside a busy pool (section 2), and keep the stencil thread-count-independent.
2. `examples/bench.rs`: the throughput, caller-wall-clock and determinism-hash lines section 2 describes. #6 needs it first, so build it as that item's first slice.
3. A cost that scales with live cells, not the whole grid, for large mostly-empty maps.
4. Per-channel diffusion rates and anisotropy, if a consumer's issue or the research asks for them.
5. Capture knobs (`STIG_FRAMES`, `STIG_SHOT`) on the examples, and a fresh `docs/foraging.gif` when behaviour changes.
