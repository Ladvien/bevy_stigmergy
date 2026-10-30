# Changelog

## [Unreleased]

### Added

- `examples/foraging.rs` capture knobs: `STIG_FRAMES=<n>` runs exactly `n` fixed steps and exits, `STIG_SHOT=<dir>` saves the window as `<dir>/foraging-<step>.png` at the last step (and every `STIG_SHOT_EVERY=<k>` steps). The example's wander now runs on the step count rather than the wall clock, so a capture is the same picture at any frame rate.
- `examples/bench.rs`: `gradient_cell_x10000`, one crowd's 10 000 gradient reads (beside walls, on rock and just off the grid among them) at 64², 256² and 1024².

### Fixed

- Diffusion no longer makes or destroys value beside walls, rock and the grid edge (#7). Each floor cell used to blend toward the mean of its floor neighbours, which is not symmetric wherever a cell has fewer than four: with evaporation 0, a 64×48 room gained 1.9% and a walled one lost 7% in 1000 ticks, and a deposit at the end of a dead-end corridor settled to 27% of itself (one in a room corner to 54%, one in the open to 108%). Now every floor–floor edge exchanges `diffuse / 4` of the difference both ways and a wall face exchanges nothing, so the floor total changes only by deposits and evaporation, to float rounding: within 7e-8 relative over 1000 ticks on the same maps, and each of those deposits settles to 0.9999–1.0002 of itself. **Field values change:** a cell with four floor neighbours computes the same bits as before from the same input, but every cell beside a wall, rock or grid edge does not, and nothing downstream of one is bit-identical to 0.2.1. In a one-wide corridor `diffuse` now moves `diffuse / 4` per neighbour per call, the rate open floor always had, rather than `diffuse / 2`. The stencil no longer divides per cell; the inline tick is 3–11% faster (median of nine interleaved runs per side).
- `StigGrid::gradient_cell` reads a wall as the face nothing crosses, not as a cold cell (#8). A rock or off-grid neighbour used to read as `0`, so beside every wall, rock cell and grid edge the gradient pointed away from the wall whatever the field held: on a 64×48 walled map a uniform field had a slope at 1 240 of 2 749 floor cells (up to 1.414), and 200 agents fleeing a source were pinned to the nearest wall (mean path distance 35.3 → 39.0 cells, 3 still within 3 cells of the source). A non-floor neighbour now reads as the cell's own value (the zero-flux wall the tick already uses), so a field flat across a wall has no slope through it, and the same agents flee to 54.9 cells (0 within 3) and climbing agents gather at the source (199 of 200 within 3 cells, was 29). Cells with four floor neighbours give the same bits as before; beside a wall the component across it is now the one-sided difference from the floor side. **A rock cell and an off-grid cell now have no slope (`Vec2::ZERO`)**, where they used to point at the nearest floor that held value; an extreme cell such as a saturated `CellFrame::world_to_cell` no longer panics on an `i32` overflow in a debug build (the gradient half of #10). The query reads one mask byte per call and branches on nothing in the map; 10 000 queries take 0.77 / 0.93 / 0.95× as long as before at 64² / 256² / 1024² (median of 20 interleaved rounds). The floor mask both grids keep is now one byte per cell, the floor bit beside the four neighbour bits, in the memory the `bool` mask used.

## [0.2.1] - 2026-10-30

### Added

- `StigGrid::evaporate_diffuse_with(dt, chunks, run)`, with `DiffuseChunk` and `DiffuseChunks`: the same tick with the stencil handed to the caller as disjoint chunks, to run on its own pool (Bevy's `ComputeTaskPool`, `rayon::scope`, scoped threads). Bit-identical to `evaporate_diffuse` for every chunk count and runner; a chunk the runner leaves unrun is run inline afterwards.
- `examples/bench.rs`: per-tick cost at 64², 256² and 1024² with 1, 4 and 8 channels, inline and on rayon and Bevy pools; the tick from a Bevy `App` system whose `ComputeTaskPool` has every other worker busy, with the ticks in which the calling thread parked; and determinism hashes at 1, 2 and the default thread count.

### Changed

- `StigGrid::evaporate_diffuse` runs inline on the calling thread. It no longer runs the stencil on rayon's global pool, which parked the calling Bevy worker until rayon's threads finished — hitches up to 150 ms in Sanctuary (#6). Results are bit-identical to 0.2.0. A large grid that relied on that internal parallelism moves to `evaporate_diffuse_with`: on a quiet 16-core host a 1024×1024 eight-channel tick took 27.7 ms with 0.2.0's default, 32.2 ms inline, and 6.0 ms with `evaporate_diffuse_with` on Bevy's pool.
- The tick writes each diffusing channel once instead of three times: evaporation is applied as each value is read, chunks write straight into the double-buffer (no serial scatter), and each floor cell carries a precomputed mask of its floor neighbours, so the stencil tests no bounds. Bit-identical to 0.2.0.

### Fixed

- `StigGrid::new` and `RallyGrid::new` accept a floor set in any order, and a repeated cell counts once. 0.2.0 required ascending, repeat-free cells and evaporated a repeated cell twice per tick.

### Removed

- The `rayon` dependency. The crate now depends on `bevy_math` alone (plus the optional `serde` and `bevy_reflect`).

## [0.2.0] - 2026-10-25

### Changed

- Bevy 0.20 (`bevy_math` 0.20, glam 0.33).
- Interfaces 0.2: no panicking index, honest units, `CellFrame`, restore.
