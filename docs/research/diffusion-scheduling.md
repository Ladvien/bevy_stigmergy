# Where the diffusion pass runs

The diffusion stencil of `StigGrid::evaporate_diffuse`: what it computes is settled (a 4-neighbour blend
over floor cells, double-buffered); this note is about **which thread computes it**, because that is what
issue #6 found wrong. Sanctuary's social fields (≈175×175 cells, 5 channels) ticked from a Bevy system
parked the calling worker in `rayon_core::latch::LockLatch::wait_and_reset` until rayon's global pool had
run the pass: median 0.41 ms, p90 14.0 ms, max 150.4 ms at load 8–22.

## Sources

| Kind | Stem | What it gives |
|---|---|---|
| Model | `10.1109_access.2019.2927815` — Dourvas, Sirakoulis & Adamatzky, *Parallel Accelerated Virtual Physarum Lab Based on Cellular Automata Agents*, IEEE Access 2019 | The CA diffusion rule is a per-cell stencil on a current/next pair of buffers; parallelism comes from the cell-local rule, not from any particular runtime. |
| Shipping game | `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` — Mike Lewis, *Escaping the Grid: Infinite-Resolution Influence Mapping*, Game AI Pro 2 ch. 29 | Grid influence maps are decay + diffusion ("breadcrumbs"), and their propagation cost is the thing that hurts a frame. |
| Technical | `Game Engine Architecture, Third Edition … Jason Gregory` §8.6.4 *Job Systems* | A game engine owns one job system that all per-frame work is kicked into; a second private pool is the anti-pattern. |
| Technical | `10.1145/1806596.1806639` — Pan, Hindman & Asanović, *Composing parallel software efficiently with Lithe*, PLDI 2010 (abstract via `paper_search`; not in the corpus) | Names the failure: libraries that each own a pool oversubscribe the cores. |
| Technical | `10.48550_arxiv.2512.04320` — Yan et al., *VLCs: Managing Parallelism with Virtualized Libraries*, 2025 (downloaded; conversion pending) | The same failure, measured on modern runtimes. |

## Craft principle

Dourvas et al. parallelise the reaction-diffusion CA by giving each cell its own output slot and swapping
buffers between steps:

> "The basic idea is to exploit the inherent parallelism of CA as modeling tool and CA ABM as a parallel
> modeling technique." … "5) At the end of each time step, assign other kernels to make the device to
> device memory copy operation. This procedure updates the CA current block with the CA next block."

The parallelism is a property of the **rule** (each output reads only the previous grid), not of the
executor. Any partition of the output slots into disjoint chunks, run in any order on any threads, writes
the same bits. That is what lets this crate hand the partition to the caller without giving up
determinism.

## How shipping games do it

Lewis, Game AI Pro 2 ch. 29, on the update a game runs every tick:

> "When the map is updated, rather than resetting the values and recalculating them based on the relevant
> placement and diffusion rules, the old values are decayed by a given proportion. The new influence
> values are then added "on top of" the old values."

and on what that costs:

> "Not only are large grids expensive in terms of memory, but they can also consume a vast amount of CPU
> time during propagation."

Gregory, *Game Engine Architecture* §8.6.4, on where that CPU time is spent in an engine:

> "A job system is a general-purpose system for executing arbitrary units of work, typically called jobs,
> across multiple cores. With a job system, a game programmer can subdivide each iteration of the game
> loop into a relatively large number of independent jobs, and submit them to the job system for
> execution."

In Bevy the job system is the `ComputeTaskPool`. A library that brings its own pool is a second job system
competing for the same cores — the Lithe abstract's "libraries [that] interfere with one another by
obliviously using the same physical cores, leading to destructive resource oversubscription", and the
stack issue #6 recorded.

## Model adopted

- `evaporate_diffuse(dt)` runs **inline on the calling thread**. No pool, no latch, nothing to wait on.
  At Sanctuary's size the serial pass is a fraction of a millisecond, so the default costs the caller's
  frame least.
- `evaporate_diffuse_with(dt, chunks, run)` is the same tick with the diffusion pass handed to the caller
  as `chunks` disjoint `DiffuseChunk`s, each a slice of floor cells with its own output slots. `run`
  receives them (as an iterator, so nothing is allocated) and executes them wherever it likes: a Bevy
  `ComputeTaskPool::scope`, `rayon::scope`, `std::thread::scope`, or a plain loop.
- The two are one path: `evaporate_diffuse` is `evaporate_diffuse_with(dt, 1, run-each-inline)`.
- `rayon` leaves `[dependencies]`; the crate is `bevy_math` alone (plus the optional derive crates).

## Parameters

| Parameter | Value | Source |
|---|---|---|
| default executor | inline, 1 chunk | issue #6 option 1; design default (the caller's frame) |
| `chunks` | caller's, `0` treated as `1`, capped at the floor-cell count | design default |
| chunk boundary | contiguous runs of the ascending floor list, sizes differing by at most 1 | design default |

## Validation targets

- Bit-identical fields for every chunk count and every executor (the bench's `determinism` hashes at 1, 2
  and the default thread count, and a permanent test over chunk counts 1…64 and a count above the floor
  size).
- `evaporate_diffuse` completes while rayon's global pool is fully blocked (permanent test; before the
  change it hangs).
- Busy-pool case (issue #6 acceptance): p99 ≤ 2× median for the 175×175 two-channel tick inside a Bevy
  `TaskPool` with every other worker busy.
- Throughput: the inline path no slower than the old `RAYON_NUM_THREADS=1` path; the rayon-run
  `evaporate_diffuse_with` no slower than the old default-pool path, within 5%.

## What great looks like

A caller writes `grid.evaporate_diffuse(dt)` in a Bevy system and never sees it in a profile as a wait.
A caller with a 1024² map writes four lines to fan the stencil over its own `ComputeTaskPool` and gets
the same bits as the inline call. The profile shows the tick as compute on the caller's worker, flat from
median to p99, at any load.

## Deviations

- Removed the internal `rayon` parallel pass (0.2.0). Its result was bit-identical; its latency was not
  bounded, because completion depended on threads the caller does not schedule (issue #6, Lithe). The
  default is now inline, with a caller-run chunked form for large grids. Design decision recorded on #6.

## Pending index

- `10.48550_arxiv.2512.04320` (VLCs): downloaded, `scribe_convert` timed out at the MCP's 30 s limit;
  convert and `distill_index` it in a later pass.
- `10.1145/1806596.1806639` (Lithe): no open PDF from the providers; quoted from its abstract.
