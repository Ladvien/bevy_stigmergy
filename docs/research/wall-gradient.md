# The gradient beside a wall (issue #8)

What `StigGrid::gradient_cell` reports at a floor cell that touches a wall, a rock cell or the grid edge,
and why reading the missing neighbour as `0.0` makes agents fight the walls. `conservative-diffusion.md`
covers what the tick computes at a wall; this note covers what the query reports there.

## The defect, measured

`gradient_cell` takes `(v(x+1) − v(x−1), v(y+1) − v(y−1))` and reads a rock or off-grid neighbour as
`0.0`. Beside a wall the missing side therefore contributes the cell's own value, pointed away from the
wall, whatever the field looks like. Measured on `cb5ebd4` (`/tmp/s3/before/findings-raw.txt`) on the
crate's own walled test map (64×48, a wall at `x = 32` with one gap, ~9% scattered rock, 2 749 floor
cells):

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
| Shipping game | `GameAIPro2_Chapter30_Modular_Tactical_Influence_Maps` — Mark, Game AI Pro 2 ch. 30 | Influence that crosses a barrier is a wrong answer (the tiger behind the fence). |
| Technical | `GPU Gems- Programming Techniques, Tips and Tricks for -- Randima Fernando; … -- isbn13 9780321228321 -- …` — Harris, *Fast Fluid Dynamics Simulation on the GPU*, GPU Gems ch. 38 | The grid-stencil recipe for a gradient at a Neumann wall: give the boundary the value just inside it, then take the ordinary central difference. |

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

Mark, on influence near barriers:

> "This helps solve situations where, for example, the tiger is on the other side of fence. Yes, it is
> close to the target location, but it is not a threat because it would take a while for the tiger to
> move around the fence."

A wall is not low influence; it is not part of the map. Reading it as `0.0` turns every wall into a
negative source — the shipped failure Couvidou describes, rebuilt inside the query.

Harris gives the stencil recipe for a Neumann wall on a cell-centred grid, where the wall lies on the
face between the boundary cell and the interior cell:

> "the pure Neumann pressure condition requires the normal pressure derivative to be zero at the
> boundaries. The boundary is defined to lie on the edge between the boundary cell and its nearest
> interior cell, but grid values are defined at cell centers. Therefore, we must compute boundary values
> such that the average of the two cells adjacent to any edge satisfies the boundary condition."

> "On solving this equation for p0,j, we see that we need to set each pressure boundary value to the
> value just inside the boundary."

after which the same chapter's gradient program takes plain central differences over interior and
boundary values alike.

## Model adopted

At a floor cell `c` holding `v_c`, each of the four neighbours reads as its own value if it is floor and
as `v_c` if it is not (rock or off the grid) — Harris's boundary value, "the value just inside the
after which its gradient program "computes the gradient of p according to the definition in Table 38-1"
(the central difference) over interior and boundary values alike.
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

## Pending index

- The home-still MCP bridge timed out on every call this pass (`catalog_recent`, `distill_search`); the
  sources above came from `hs distill search --server http://192.168.1.110:7434`. Whether
  `10.1039_a702602a` has a catalog row was not checked.
