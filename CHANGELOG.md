# Changelog

## [Unreleased]

### Added

- `StigGrid::evaporate_diffuse_with(dt, chunks, run)`, with `DiffuseChunk` and `DiffuseChunks`: the same tick with the stencil handed to the caller as disjoint chunks, to run on its own pool (Bevy's `ComputeTaskPool`, `rayon::scope`, scoped threads). Bit-identical to `evaporate_diffuse` for every chunk count and runner; a chunk the runner leaves unrun is run inline afterwards.
- `examples/bench.rs`: per-tick cost at 64², 256² and 1024² with 1, 4 and 8 channels, inline and on rayon and Bevy pools; the tick from a Bevy `App` system whose `ComputeTaskPool` has every other worker busy, with the ticks in which the calling thread parked; and determinism hashes at 1, 2 and the default thread count.

### Changed

- `StigGrid::evaporate_diffuse` runs inline on the calling thread. It no longer runs the stencil on rayon's global pool, which parked the calling Bevy worker until rayon's threads finished — hitches up to 150 ms in Sanctuary (#6). Results are bit-identical to 0.2.0.
- The tick writes each diffusing channel once instead of three times: evaporation is applied as each value is read, chunks write straight into the double-buffer (no serial scatter), and each floor cell carries a precomputed mask of its floor neighbours, so the stencil tests no bounds. Bit-identical to 0.2.0.

### Fixed

- `StigGrid::new` and `RallyGrid::new` accept a floor set in any order, and a repeated cell counts once. 0.2.0 required ascending, repeat-free cells and evaporated a repeated cell twice per tick.

### Removed

- The `rayon` dependency. The crate now depends on `bevy_math` alone (plus the optional `serde` and `bevy_reflect`).

## [0.2.0] - 2026-10-25

### Changed

- Bevy 0.20 (`bevy_math` 0.20, glam 0.33).
- Interfaces 0.2: no panicking index, honest units, `CellFrame`, restore.
