# bevy_stigmergy

> ⚠️ **Vibe Coded** — written by an AI agent working from a human's direction. It is used in a shipping game and covered by tests, but it has had no line-by-line human audit. Read it before you trust it.

Stigmergic influence fields over a fixed cell grid: `N` scalar channels that agents deposit into, which evaporate, diffuse between floor cells, and can be sampled or climbed — plus a vectorial rally pheromone for tracking a moving target.

**This repository is canonical.** It is where issues and pull requests belong; the split-out copies elsewhere are the ones that are frozen. MIT OR Apache-2.0.

![A dark grid where ninety pale and amber dots move between a nest and a food source; a bright amber trail forms through a gap in a wall while older, dimmer trails elsewhere fade away](https://raw.githubusercontent.com/Ladvien/bevy_stigmergy/main/docs/foraging.gif)

That is `examples/foraging.rs`. Ninety foragers, a nest, a food source, and a wall with two gaps. **No agent has a map, a route, or any idea where the gaps are** — each reads three numbers at the cell it is standing on and steers by the gradient. The bright path is emergent, and the dim ones fading beside it are the point: evaporation is how the colony forgets a route that stopped paying.

## The idea

Stigmergy is coordination *through the environment*. Agents write into a shared field and read it back, so a crowd converges on a trail, disperses from a threat, or recruits toward an alarm without anybody negotiating — the field is computed once and shared by all of them, and the group behaviour falls out.

```rust
use bevy_math::IVec2;
use bevy_stigmergy::{ChannelDef, StigGrid};

const SCENT: usize = 0;
const THREAT: usize = 1;

let mut field = StigGrid::<2>::new(width, height, floor_cells(), [
    ChannelDef { evaporate: 0.4, diffuse: 0.10, deposit_radius: 1.5 },  // SCENT
    ChannelDef { evaporate: 1.2, diffuse: 0.05, deposit_radius: 3.0 },  // THREAT
]);

field.deposit(SCENT, cell, 1.0);        // placement
field.evaporate_diffuse(dt);            // decay + spread, once a tick
let here = field.sample_cell(SCENT, c); // query
let uphill = field.gradient_cell(SCENT, c);
```

## What it deliberately does not do

**It owns no resources, registers no systems, and has no schedule.** When deposits drain, when evaporation runs, and where both sit relative to the agents reading the field are gameplay decisions. A crate that made them would be unusable in any game that wanted different ones.

**It does not name your channels.** A channel table is game content. Channels are `usize` indices and `N` is yours to pick, so "what does channel 3 mean" stays a question about your game.

**It works in cell space, not world space.** Every entry point takes an `IVec2` cell. The crate does not decide how big a cell is or where the grid sits in the world.

[`CellFrame`] is an optional plain value type for the mapping nearly every caller writes anyway — an axis-aligned grid of uniform cells:

```rust
use bevy_math::Vec2;
use bevy_stigmergy::CellFrame;

// Cover a 120 x 80 metre field in 2-metre cells.
let frame = CellFrame::covering(Vec2::new(-60.0, -40.0), Vec2::new(60.0, 40.0), 2.0);

let mut field = StigGrid::<2>::new(frame.width, frame.height, frame.floor_cells(is_floor), [
    ChannelDef { evaporate: 0.4, diffuse: 0.10, deposit_radius: 1.5 },
    ChannelDef { evaporate: 1.2, diffuse: 0.05, deposit_radius: 3.0 },
]);

field.deposit(SCENT, frame.world_to_cell(agent), 1.0);
let uphill = field.gradient_cell(SCENT, frame.world_to_cell(agent));
```

`cell_centre`, `contains` and `cells()` (ascending row-major, the order `StigGrid::new` keeps its floor set in — though it accepts any order, and a repeated cell counts once) round it out. It is a convenience, not a requirement: a caller with a hex sheet, a rotated board or a camera-relative projection ignores it and keeps its own mapping, and nothing in the crate behaves differently either way.

## Units and time

The crate has no clock. `evaporate_diffuse(dt)` applies `ChannelDef::evaporate` **per unit of whatever `dt` you pass**: per second when you step in seconds, per game-day when you step in game-days. The same number means different things in each — `0.4` loses 40% of the field per second in the first and 40% per game-day in the second. `diffuse` is different: it is applied **per call**, not scaled by `dt`, so two calls at `dt/2` spread twice as much as one call at `dt`.

Diffusion moves value and never makes or loses any. Each call, every floor cell trades `diffuse / 4` of the difference with each floor neighbour — in open floor, a blend toward the neighbour average — and nothing across a wall, so with no deposits the floor total after `k` ticks is `(1 − evaporate·dt)^k` of the start, to float rounding, whether the scent sits in a hall or at the end of a dead-end corridor. Keep `diffuse` in `[0, 1]`; up to 1 no value goes negative.

## Which thread does the work

**Yours.** `evaporate_diffuse(dt)` runs inline on the thread that calls it and returns; the crate owns no thread pool and depends on none. A Bevy system that calls it shows up in a profile as compute on that worker, never as a wait on another pool's threads.

That is what `examples/bench.rs` checks in its `busy_pool` line: a 175×175 two-channel field ticked from a Bevy `App` system while every other `ComputeTaskPool` worker is busy. On a 16-core host at load average ≈65, 0.2.0 (which ran the stencil on rayon's pool) parked the system's thread on every tick, median 18.7–20.7 ms and max 97 ms; the inline tick parked on none of 6 000 ticks, median 0.25 ms, and p99 0.28–0.31 ms in five runs of six (the sixth, 1.9 ms, is the OS preempting a thread that never slept).

A large grid can spread the work over **your** pool instead. `evaporate_diffuse_with` hands you the pass as disjoint chunks and you run them wherever you like — here, Bevy's own `ComputeTaskPool`:

```rust
let pool = ComputeTaskPool::get();
field.evaporate_diffuse_with(dt, pool.thread_num(), |chunks| {
    pool.scope(|s| chunks.for_each(|chunk| s.spawn(async move { chunk.run() })));
});
```

`rayon::scope`, `std::thread::scope` or a plain loop work the same way. The result is bit-identical to `evaporate_diffuse` for any chunk count and any runner, and a chunk your runner drops instead of running is run inline afterwards, so a runner bug costs time, never a stale field. The spread costs a scope per diffusing channel, so it pays only on big grids: on the same host, quiet, one channel took 8 µs inline against 17 µs on the pool at 64×64, 229 µs against 88 µs at 256×256, and eight channels at 1024×1024 took 32 ms against 6 ms. While the pool is busy, the caller waits for whichever worker picked up a chunk; the inline call never waits for anyone.

## Features

- `serde` — `Serialize`/`Deserialize` on `ChannelDef`, `RallyDef` and `CellFrame`. Also turns on `bevy_math/serialize`, which glam's impls need.
- `reflect` — `bevy_reflect::Reflect` on the same types, for a host that stores them in a Bevy world.

Both are off by default, and neither adds a type to the dependency graph a consumer did not ask for.

## Two gotchas worth knowing

- **A deposit masks its destination, not its path.** The kernel is a Euclidean disc that skips non-floor cells — not a flood fill and not a line-of-sight test — so a radius wide enough to span a wall still reaches the floor beyond it. That is deliberate (a smell or a sound goes round a corner). *Diffusion* is the pass that respects walls.
- **Deposits accumulate with a non-associative `+=`.** If you produce them from an unordered source (an ECS query, a hash map), sort the batch before submitting or your field is not reproducible. This crate never sees the batch, only individual deposits, so it cannot do it for you.

## Determinism

Written to be bit-reproducible, because a simulation that hashes its state cannot afford otherwise: the neighbour sum keeps a fixed E/W/S/N order (float addition is non-associative); the diffusion is a pure stencil over disjoint output slots, so its result is identical however `evaporate_diffuse_with` splits it and on however many threads you run it; and skipping rock cells is exact rather than approximate. `channels()` and `cells()` expose the **full** grids so you can fold the rock-cells-stay-zero invariant into your own fingerprint.

## Used by

The rest of the family, so a host learns one shape:

- [`bevy_orca`](https://crates.io/crates/bevy_orca) — the crowd that deposits into this field and steers by its gradient.
- [`bevy_society`](https://crates.io/crates/bevy_society) — persons whose presence and commotion are channels here.

## References

- Holland & Melhuish, *Stigmergy, self-organization, and sorting in collective robotics* (1999)
- Tang, Liu & Pan, ACO review, IEEE/CAA JAS (2021) — deposit, evaporation, positive feedback
- Lewis, *Escaping the Grid*, Game AI Pro 2 Ch.29 — placement / diffusion / query
- Mark, *Modular Tactical Influence Maps*, Game AI Pro 2 Ch.30
- Tang et al., *Dynamic target searching and tracking with swarm robots based on stigmergy*, Robotics & Autonomous Systems (2019) — the vectorial pheromone
- Dourvas, Sirakoulis & Adamatzky, IEEE Access (2019) — parallelising the diffusion stencil

## Examples

```sh
cargo run -p bevy_stigmergy --example trail     # terminal: a scent trail spreads, fades, respects walls
cargo run -p bevy_stigmergy --example rally     # terminal: the vectorial pheromone tracking a target
cargo run -p bevy_stigmergy --example foraging  # the gif above. Needs a GPU.
```

`trail` and `rally` render the field as ASCII — no window, no GPU. `trail` lays a channel along a walked path across a map with a wall and a single gap, so you can watch diffusion go through the gap and not through the wall. `rally` has three fixed scouts marking a target that keeps moving, then stops marking so you can see the recruitment expire itself.

`foraging` is three channels at once — a HOME beacon, a FOOD beacon, and a TRAIL that only laden foragers lay — and it is the example to read if you are tuning this into a game, because the single hardest thing here is picking weights that are in scale with each other. Measured in that example, a beacon gradient averages about `0.09` per cell. The first wander term tried was `2.2`, which is *larger than the homing signal it competes with*, so laden foragers wandered the whole map laying trail everywhere and the "path" was an undifferentiated wash. Steering weights have to be set against `weight * |gradient|`, not by taste — and the way to find out is to print the gradient magnitudes rather than squint at the picture.

## License

MIT OR Apache-2.0, at your option.
