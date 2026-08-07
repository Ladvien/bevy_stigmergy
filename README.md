# bevy_stigmergy

Stigmergic influence fields over a fixed cell grid: `N` scalar channels that agents deposit into,
which evaporate, diffuse between floor cells, and can be sampled or climbed — plus a vectorial rally
pheromone for tracking a moving target.

> **This repo is a read-only mirror.** It is split out of
> [`Ladvien/foundation_vs_slop`](https://github.com/Ladvien/foundation_vs_slop) with `git subtree
> split`, history intact. Issues and PRs belong upstream.

## The idea

Stigmergy is coordination *through the environment*. Agents write into a shared field and read it
back, so a crowd converges on a trail, disperses from a threat, or recruits toward an alarm without
anybody negotiating — the field is computed once and shared by all of them, and the group behaviour
falls out.

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

**It owns no resources, registers no systems, and has no schedule.** When deposits drain, when
evaporation runs, and where both sit relative to the agents reading the field are gameplay decisions.
A crate that made them would be unusable in any game that wanted different ones.

**It does not name your channels.** A channel table is game content. Channels are `usize` indices and
`N` is yours to pick, so "what does channel 3 mean" stays a question about your game.

**It works in cell space, not world space.** Every entry point takes an `IVec2` cell. If you already
own a world↔cell mapping, this crate keeping a second copy of it would only give the two a chance to
drift.

## Two gotchas worth knowing

- **A deposit masks its destination, not its path.** The kernel is a Euclidean disc that skips
  non-floor cells — not a flood fill and not a line-of-sight test — so a radius wide enough to span a
  wall still reaches the floor beyond it. That is deliberate (a smell or a sound goes round a corner).
  *Diffusion* is the pass that respects walls.
- **Deposits accumulate with a non-associative `+=`.** If you produce them from an unordered source (an
  ECS query, a hash map), sort the batch before submitting or your field is not reproducible. This
  crate never sees the batch, only individual deposits, so it cannot do it for you.

## Determinism

Written to be bit-reproducible, because a simulation that hashes its state cannot afford otherwise:
the neighbour sum keeps a fixed E/W/S/N order (float addition is non-associative); the diffusion is a
pure stencil over disjoint output slots, so its result is identical at any thread count — which is what
makes the `rayon` pass safe; and skipping rock cells is exact rather than approximate. `channels()` and
`cells()` expose the **full** grids so you can fold the rock-cells-stay-zero invariant into your own
fingerprint.

## References

- Holland & Melhuish, *Stigmergy, self-organization, and sorting in collective robotics* (1999)
- Tang, Liu & Pan, ACO review, IEEE/CAA JAS (2021) — deposit, evaporation, positive feedback
- Lewis, *Escaping the Grid*, Game AI Pro 2 Ch.29 — placement / diffusion / query
- Mark, *Modular Tactical Influence Maps*, Game AI Pro 2 Ch.30
- Tang et al., *Dynamic target searching and tracking with swarm robots based on stigmergy*, Robotics &
  Autonomous Systems (2019) — the vectorial pheromone
- Dourvas, Sirakoulis & Adamatzky, IEEE Access (2019) — parallelising the diffusion stencil

## License

MIT OR Apache-2.0, at your option.
