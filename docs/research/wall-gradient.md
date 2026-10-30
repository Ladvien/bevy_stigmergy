# The gradient beside a wall (issue #8)

What `StigGrid::gradient_cell` reports at a floor cell that touches a wall, a rock cell or the grid edge,
and why reading the missing neighbour as `0.0` makes agents fight the walls. `conservative-diffusion.md`
covers what the tick computes at a wall; this note covers what the query reports there.

## The defect, measured

`gradient_cell` takes `(v(x+1) − v(x−1), v(y+1) − v(y−1))` and reads a rock or off-grid neighbour as
`0.0`. Beside a wall the missing side therefore contributes the cell's own value, pointed away from the
wall, whatever the field looks like. Measured on `cb5ebd4` on the crate's own walled test map (64×48, a
wall at `x = 32` with one gap, ~9% scattered rock, 2 749 floor cells); pass 8 first
(`/tmp/s3/before/findings-raw.txt`), and pass 19 again with a rebuilt harness, number for number
(`/tmp/s3p19/before/findings-raw.txt`):

| Scenario | Result |
|---|---|
| uniform field `1.0`, one tick (the field stays exactly uniform) | nonzero slope at **1 240 of 2 749** floor cells, max `|grad|` **1.414** |
| ramp `2x + y` | interior cells read `(4, 2)` (1 479 of 1 479); **635 boundary cells report a downhill component** |
| field that varies along one axis only (`sin(0.37y)`, `cos(0.29x)`) | a slope across the flat axis at **670** and **630** cells |
| the same, at each wall face the field is flat across | fleeing (`−grad`) points **into the face at 1 300 of 1 362** faces |
| 200 agents fleeing a settled source for 400 steps | mean geodesic distance 35.3 → **39.0** cells; **3** still within 3 cells of the source; **200 of 200** end against a wall |
| 200 agents climbing the same source | 35.3 → **25.8** cells; **29 of 200** within 3 cells; 134 end against a wall |

A fleeing agent is pinned to the nearest wall, harder the stronger the threat around it, and a climbing
agent is shoved off every wall it passes, including the walls of the corridor it needs to go down.

## Sources

| Kind | Stem | What it gives |
|---|---|---|
| Model | `10.1039_a702602a` — Maini, Painter & Chau, *Spatial pattern formation in chemical and biological systems*, J. Chem. Soc., Faraday Trans. 93(20), 1997 | The reaction-diffusion model's boundary is zero flux: impermeable to the chemicals. |
| Model | `10.3133_tm6a55` — Langevin et al., *MODFLOW 6*, USGS TM 6-A55, 2017 | Flow lives on the edge between two active cells; an inactive cell is no-flow (already the basis of the tick, `conservative-diffusion.md`). |
| Shipping game | `gameaipro4-ch06-flooding-influence-map-chase-dishonored-2` — Couvidou, *Flooding the Influence Map for Chase in Dishonored 2*, Game AI Pro 4 ch. 6 | A direction read off the world that runs into the nearest wall is the playtest failure; the influence map was adopted so the goal follows level topology instead. |
| Shipping game | `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` — Lewis, Game AI Pro 2 ch. 29 | Influence that meets an obstacle bounces off and flows around it; it does not vanish into it. |
| Technical | `GPU Gems- Programming Techniques, Tips and Tricks for -- Randima Fernando; NVIDIA Corporation -- GPU Gems 1, 1, 1, 2004 -- Addison-Wesley Professional -- isbn13 9780321228321 -- f043298dfdc9cc46544a7a1f89d51f82 -- Anna’s Archive` — Harris, *Fast Fluid Dynamics Simulation on the GPU*, GPU Gems ch. 38 (markdown pages 677–679) | The grid-stencil recipe for a gradient at a Neumann wall: give the boundary the value just inside it, then take the ordinary central difference. |

## Craft principle

Maini, Painter & Chau, on the model this crate's channels are an instance of:

> "A general reaction diffusion (RD) system has the form ∂u/∂t = D∇²u + f(u, p) … Boundary conditions
> are usually taken as zero flux, that is, the domain boundary is assumed impermeable to the chemicals."

With Fickian diffusion the flux is `−D∇u`, so zero flux through a wall is a zero normal derivative
there: the field has no slope *through* a wall. Since #7 the tick implements exactly that (a wall face
exchanges nothing). A query that reports a normal slope at a wall on a field that is flat across it is
describing a different boundary from the one the field was computed with — a cold sink at every wall.

## How shipping games do it

Couvidou, on why *Dishonored 2* stopped steering chase NPCs by a direction extrapolated from the world:

> "At some point, though, play testers reported that the chase experience was disappointing. In many
> cases, the extrapolation was hitting nearby navmesh boundaries (e.g. a wall or a cliff) causing NPCs to
> give up too early when there was still an obvious path to follow"

and what the influence map bought them:

> "This gives the NPC a goal to chase that follows the level topology instead of hitting the nearest
> wall, unlike the basic dead reckoning described previously."

Lewis, on what influence does at an obstacle (section 29.7, "Handling Obstacles and Nontrivial
Topologies"):

> "As described, things quickly break down when considering influence propagation, because it is
> important to conserve “energy” as influence spreads outward. Influence that hits an obstacle should not
> simply vanish, but rather “bounce off” and appear to flow around the boundary of the obstruction."

A wall is not low influence; it is not part of the map. Influence that reaches it reflects, so the field
beside a wall is flat across it unless something on the floor side makes it slope. Reading the wall as
`0.0` turns every wall into the sink Lewis rules out — and, in the query, into the shipped failure
Couvidou describes: a direction that runs into the nearest wall.

Harris gives the stencil recipe for a Neumann wall on a cell-centred grid, where the wall lies on the
face between the boundary cell and the interior cell:

> "the pure Neumann pressure condition requires the normal pressure derivative to be zero at the
> boundaries. The boundary is defined to lie on the edge between the boundary cell and its nearest
> interior cell, but grid values are defined at cell centers. Therefore, we must compute boundary values
> such that the average of the two cells adjacent to any edge satisfies the boundary condition."

> "On solving this equation for p0.j, we see that we need to set each pressure boundary value to the
> value just inside the boundary."

after which the same chapter's gradient program "computes the gradient of p according to the definition
in Table 38-1" — the central difference — over interior and boundary values alike.

## Model adopted

At a floor cell `c` holding `v_c`, each of the four neighbours reads as its own value if it is floor and
as `v_c` if it is not (rock or off the grid) — Harris's boundary value, "the value just inside the
boundary" — and the gradient is the ordinary central difference over those reads:
`grad = (v_E − v_W, v_S − v_N)` with `v_n := v_c` for a non-floor `n`.

- **Interior cells are bit-identical:** with four floor neighbours the reads and the two subtractions are
  today's.
- Along an axis with one wall face, the component is the one-sided difference from the floor side
  (`v_c − v_W`, or `v_E − v_c`); with walls on both sides (a 1-wide corridor's cross axis) it is `0`. So the
  component is the sum of the two face differences along that axis, and a wall face contributes nothing —
  the same edges the tick exchanges value over, and the same zero-flux face.
- This is the read the tick already does: `own·(1 − n·q) + q·Σ_floor v_j = own + q·Σ_{4 faces}(v̂_j − own)`
  with `v̂_j = own` across a wall. One wall model for the stencil and the query.
- **A cell that is not floor — rock, or off the grid — has no slope: `Vec2::ZERO`**, the same empty answer
  an undefined channel gives. The field does not exist there (rock is held at `0.0` as a storage invariant,
  not as a value), and reading rock against its floor neighbours pushes a fleeing agent that stands in a
  rock cell deeper into the rock — the same defect one cell over.
- The cell is tested for floor before any neighbour is formed, and neighbours are addressed by row-major
  index offsets, so no `i32` sum is ever formed for an off-grid cell.

Rejected:

- **One-sided difference doubled at a wall** (`2·(v_c − v_W)`; the edge rule of `numpy.gradient`) keeps a
  ramp's magnitude at the wall, but it extrapolates the field linearly *through* the wall, which is the
  opposite of zero flux, and it doubles the push into a wall at a peak that sits against one.
- **Dropping any axis that touches a wall** never points into a wall, but it throws away the real slope
  from the floor side: an agent beside a wall with the threat source against that wall gets no push away
  from it.
- **Keeping the old reading for rock cells** (only floor cells change): see above.

## Parameters

| Parameter | Value | Source |
|---|---|---|
| value read across a wall face | the cell's own value | Harris (Neumann boundary value); the tick's own face rule (#7) |
| slope at a rock or off-grid cell | `Vec2::ZERO` | design default (assumed decision, #8) |
| scale | unchanged, `v_E − v_W` (twice the per-cell slope of a ramp) | #8 acceptance: interior unchanged |

## Validation targets

- **Uniform field:** `Vec2::ZERO` at every floor cell of the walled map (before: 1 240 of 2 749 nonzero).
  Permanent test.
- **Ramp `2x + y` on the walled map:** every interior cell `(4, 2)` exactly; every axis whose two
  neighbours are floor keeps its central difference exactly; no component points downhill (before: 635).
  Permanent test.
- **Flat across a wall:** for fields varying along one axis only, the other component is exactly 0 at every
  floor cell (before: 670 and 630 cells); fleeing or climbing points into 0 wall faces (before: 1 300 of
  1 362).
- **Agents:** 200 agents fleeing a settled source move away from it (geodesic distance up, none left
  within 3 cells); 200 agents climbing it gather at it (within 3 cells: more than 29 of 200).
- **Empty answers:** rock, off-grid (including `i32::MAX`/`MIN` coordinates) and undefined channels read
  `Vec2::ZERO`, with no panic in a debug build.
- **Determinism:** the tick is untouched, so the bench's `determinism` hashes stay `a446265d6a95cf51`.
- **Cost:** `gradient_cell_x10000` (new bench case) within 5% of before at every grid size, or a logged
  trade: the read adds up to four floor-mask loads per query.

## What great looks like

A guard fleeing a grenade's threat field along a corridor runs down the corridor, not into its wall; a
scent hound beside a wall on a flat trail stands still instead of shuffling away from the wall; in a
1-wide corridor the slope runs along the corridor and nowhere else; and in `foraging` the outbound
foragers slide along the wall to the gap instead of being shoved off it. The query and the tick agree on
what a wall is: a face nothing crosses, not a cold cell.

## Deviations

- Beside a wall, rock cell or grid edge, `gradient_cell`'s component across the wall face changes from
  "the cell's own value, pointed away from the wall" to the one-sided difference from the floor side (half
  a ramp's central difference); the component along the wall is unchanged. Interior cells are unchanged.
  Every caller that steers by the gradient beside a wall sees different numbers there. Release-noted.
- `gradient_cell` at a rock or off-grid cell returns `Vec2::ZERO`; before, it returned differences against
  zero-read neighbours, pointing at the nearest floor that held value.

## Measured in iteration 3

Harness `/tmp/s3/checklist.rs` (scratch), same map (64×48, 2 749 floor cells), `cb5ebd4` against the branch.
Its counts differ a little from the ones above (a different agent rule and fleeing-face definition); these
are the numbers the log quotes.

| Target | Before | After |
|---|---|---|
| uniform field, floor cells with a slope | 1 240 of 2 749 (max 1.414) | 0 |
| ramp `2x + y`: interior cells reading `(4, 2)`; boundary cells with a downhill component | 1 479 of 1 479; 635 of 1 270 | 1 479 of 1 479; 0 |
| field flat along one axis: cells with a slope across it | 658 and 630 | 0 and 0 |
| fleeing points into a wall face the field is flat across | 686 of 1 362 faces | 0 |
| random field: axes with two floor neighbours (4 167), four-neighbour cells (1 479), bit hashes | `a9fdeab01ccdeb7c`, `021a50d60f690860` | equal |
| 200 agents climbing a settled source, 400 steps: mean geodesic distance 37.2 →; within 3 cells | 12.6; 80 of 200 | 0.5; 200 of 200 |
| 200 agents fleeing: mean distance →; stranded short of a local distance maximum | 49.8; 52 | 51.8; 14 |
| bench `determinism`, 30 lines (caller, Bevy 16, rayon 1, 2, 24) | `a446265d6a95cf51` | `a446265d6a95cf51` |
| `gradient_cell_x10000`, median of 3 interleaved rounds, 64² / 256² / 1024² | 54.0 / 69.8 / 95.1 µs | 47.7 / 59.1 / 79.2 µs (0.88× / 0.85× / 0.83×) |

**Cost.** The first version (a `bool` mask, four floor-mask loads per query beside the cell's own) was
0.73× at 64² but 1.06–1.13× at 256² and 1.00–1.15× at 1 024² against the old query: more loads and
data-dependent branches on the rock pattern. A branchless form (`get` + select) was 1.3–1.8× *slower*. The
version shipped keeps one flag byte per cell (floor, and which of its four neighbours are floor, the bits
the tick's `FloorCell` already carried), replacing the `bool` mask in both grids: one load answers every
wall question, and the query is faster than before at every size. Results are bit-identical to the first
version.

**`foraging`.** Stills at steps 900, 1 800, 2 700 and 3 600 (1/60 s each) from the old and new query show
the same route through the left gap, forming by step 900 and holding to 3 600. The "slide along the wall to
the gap" above does not show here: the gap sits straight between the nest and the food, so foragers seldom
touch a wall. The README's picture still describes the example and was not regenerated.

## Pending index

- Pass 8: the home-still MCP bridge timed out on every call (`catalog_recent`, `distill_search`); its
  sources came from `hs distill search --server http://192.168.1.110:7434`.
- Pass 19: distill could not embed a query (`gpu busy: 4640 MB free < 5000 MB required to load bge-m3`),
  so every quote above was checked word for word against `markdown_read` instead. Pass 8's quote from
  Mark (Game AI Pro 2 ch. 30, "the tiger is on the other side of fence") is **not** in that chapter's
  markdown and was dropped; Lewis ch. 29 replaces it. `10.1039_a702602a` has markdown and is indexed.
- Iteration 3: distill healthy (`system_status`, `bge-m3`, 18 070 embedded). `catalog_recent` (200) held
  nothing in this crate's domains (one false hit on "diffusion": a mesh-generation model). Quotes from
  `10.1039_a702602a`, Lewis ch. 29 and Couvidou ch. 6 re-checked against `markdown_read`; Harris's two
  quotes are on page 678 of the GPU Gems stem (not 677 or 679).
