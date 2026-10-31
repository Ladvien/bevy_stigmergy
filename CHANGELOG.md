# Changelog

## [Unreleased]

### Added

- `examples/foraging.rs` capture knob: `STIG_FRAMES=n` runs `n` steps and exits, `STIG_SHOT=prefix` writes `prefix-<step>.png` of the last step (and of every `STIG_EVERY`th). The example's clock is now its step count, so a run is the same every time; the wander phase used to read wall-clock time.
- `examples/bench.rs`: `gradient_cell_x10000`, a crowd's 10 000 queries timed as one batch, and `deposit_x1000` and `rally_deposit_x1000`, a crowd's 1 000 deposits through `StigGrid::deposit` and `RallyGrid::deposit`.

### Fixed

- Diffusion no longer makes or destroys value beside walls, rock and the grid edge (#7). Each floor cell used to blend toward the mean of its floor neighbours, which is not symmetric wherever a cell has fewer than four: with evaporation 0, a 64×48 room gained 1.9% and a walled one lost 7% in 1000 ticks, and a deposit at the end of a dead-end corridor settled to 27% of itself (one in a room corner to 54%, one in the open to 108%). Now every floor–floor edge exchanges `diffuse / 4` of the difference both ways and a wall face exchanges nothing, so the floor total changes only by deposits and evaporation, to float rounding: within 7e-8 relative over 1000 ticks on the same maps, and each of those deposits settles to 0.9999–1.0002 of itself. **Field values change:** a cell with four floor neighbours computes the same bits as before from the same input, but every cell beside a wall, rock or grid edge does not, and nothing downstream of one is bit-identical to 0.2.1. In a one-wide corridor `diffuse` now moves `diffuse / 4` per neighbour per call, the rate open floor always had, rather than `diffuse / 2`. The stencil no longer divides per cell; the inline tick is 3–11% faster (median of nine interleaved runs per side).
- `StigGrid::gradient_cell` no longer reports a slope off every wall (#8). It read a rock or off-grid neighbour as `0.0`, so on a uniform field it returned a slope pointing away from each wall, rock cell and grid edge (1 240 of the 2 749 floor cells of a 64×48 walled map, up to 1.41), and an agent fleeing a threat field was pulled into the nearest wall, harder the stronger the threat: with a source the tick had settled, 200 agents climbing greedily for 400 steps ended within 3 cells of it 20 times in 200 and now do so 200 times, and fleeing, 67 of 200 stood at rest with a farther floor cell beside them and now 9 do. A neighbour that is not floor now reads as the cell's own value, the boundary the tick already has (zero flux across a wall face, #7). A uniform field has no slope anywhere, a field flat across a wall has no slope across it, and across a wall the component is the one-sided difference from the floor side, so a ramp never reads downhill. **Values beside a wall, rock and the grid edge change**, so a caller that steers by the gradient there sees different numbers: a cell with four floor neighbours, and the component along a wall, are unchanged to the bit (random field: 1 479 four-neighbour cells, 2 061 x axes and 2 106 y axes hash equal). A rock cell, an off-grid cell however far (a saturated `CellFrame::world_to_cell` no longer overflows an `i32` here) and an undefined channel all read `Vec2::ZERO`. The grids keep one flag byte per cell (floor, and which neighbours are floor) in place of the `bool` mask, so the query is one load for its walls: 10 000 queries take 0.80–0.95× the time they did at the median (64² to 1024² grids, old and new interleaved in one process, three runs); a deposit and the tick cost what they did, within noise (medians within 5%).
- A NaN or infinite deposit no longer poisons a channel for good (#9). `StigGrid::deposit` and `RallyGrid::deposit` wrote `amount × falloff` into every cell of the disc unchecked, and nothing in the tick can remove a non-finite value: it spread one cell a tick (21 of a 64×64 field's cells after 1 tick, 303 after 10, all 4 096 after 200), `sample_cell` and `gradient_cell` returned it, and `hotspot_cell` and `saturation_stats` skipped it and reported an empty field. `+∞` at a whole-number radius wrote NaN on the first call (`∞ × 0` at the rim), the rally grid kept NaN through every `evaporate`, and a finite `f32::MAX` twice overflowed to `∞` in the cell. Now a deposit is a finite quantity or nothing: a NaN or infinite `amount`, a rally vector with a non-finite component, and any rally deposit while `RallyDef::accumulate` is not finite write nothing, as an undefined channel already does; and a finite deposit stops at `±f32::MAX / 16` (about `2.1e37`, the most the tick's four-neighbour sum carries with a factor of four to spare) instead of overflowing. A 10 000-tick soak of 160 000 deposits with 1 in 16 hostile left 5 496 of 6 144 cells non-finite, the first at tick 0, and now leaves none. Deposits inside the limit are bit-for-bit what they were (the soak, the conservation run, the bench's determinism and throughput hashes, and the fields after every deposit A/B run all hash equal before and after); a batch of 1 000 deposits costs 0.95–1.03× at the median for `StigGrid::deposit` and 0.96–1.02× for `RallyGrid::deposit` (64² to 1 024² fields, old and new interleaved in one process, three runs of 1 000 batches).

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
