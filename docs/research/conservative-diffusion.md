# Conservative diffusion at walls (issue #7)

What `StigGrid::evaporate_diffuse` computes per floor cell, and why its old form made and destroyed value
wherever a cell had fewer than four floor neighbours. `diffusion-scheduling.md` covers where the pass runs;
this note covers what it computes.

## The defect, measured

0.2.1 blends each floor cell toward the **mean of its floor neighbours**:
`out_i = (1 − d)·v_i + d·(Σ_{j∈N(i)} v_j) / n_i`, with `n_i ∈ 0..=4` its floor-neighbour count. Cell `j`
gives up `d·v_j`, but its neighbours receive `d·v_j · Σ_{i∈N(j)} 1/n_i` of it, which is `d·v_j` only when
every neighbour has the same count. Beside every wall, rock cell and grid edge, value is created or
destroyed each tick. Measured on 0.2.1 (`/tmp/s2/before/findings-raw.txt`; evaporation 0, `d = 0.25`):

| Scenario | Floor total, start → end |
|---|---|
| 64×48 all floor, two deposits, 1000 ticks | 834.31 → 850.44 (**+1.93%**) |
| 64×48, wall with one gap + 8% rock, 1000 ticks | 696.45 → 647.67 (**−7.00%**, worst −7.99%) |
| room + 1-wide dead-end corridor, one deposit of 1.0 at the **dead-end tip**, settled | **0.269** |
| the same, deposit in a **room corner** | **0.538** |
| the same, deposit at the **room centre** | **1.076** |
| 20 ticks at evaporation 0.1, walled map: total vs `(1 − 0.1)^20 ×` start | **+0.78%** off |

So a scent laid at the end of a dead end is worth a quarter of the same scent laid in the open, and the
total on the map is not "what was deposited, minus what evaporated". That is a wrong result in a game
that reads mass: a value-weighted centroid (below), a "how much scent is here" threshold, a field
restored from a save and compared.

## Sources

| Kind | Stem | What it gives |
|---|---|---|
| Model | `10.1109_jproc.2006.887293` — Olfati-Saber, Fax & Murray, *Consensus and Cooperation in Networked Multi-Agent Systems*, Proc. IEEE 95(1), 2007 | Diffusion on a graph as a Perron matrix `P = I − εL`; the sum is invariant iff the exchange is symmetric; the neighbour-mean form `(1 − ε)I + εD⁻¹A` is the old rule and keeps only a degree-weighted average; the step bound `ε < 1/Δ`. |
| Model | `10.3133_tm6a55` — Langevin et al., *Documentation for the MODFLOW 6 Groundwater Flow Model*, USGS TM 6-A55, 2017 | The control-volume form: flow between two cells is one conductance on the **edge**, `Q_{n,m} = C_{n,m}(h_m − h_n)`, and inactive cells are no-flow. |
| Shipping game | `gameaipro4-ch06-flooding-influence-map-chase-dishonored-2` — Couvidou, *Flooding the Influence Map for Chase in Dishonored 2* | A shipped game reads an influence map's **mass**: the chase goal is the value-weighted centroid of the warm cells, chosen so the goal follows level topology rather than walls. |
| Shipping game | `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` — Lewis, Game AI Pro 2 ch. 29 | Grid influence maps propagate around obstacles; decay + diffusion is the per-tick update. |
| Technical | `GPU Gems- Programming Techniques, Tips and Tricks for -- Randima Fernando; … -- isbn13 9780321228321 -- …` — Harris, *Fast Fluid Dynamics Simulation on the GPU*, GPU Gems ch. 38 | A grid stencil with arbitrary solid boundaries decides, per cell, in which directions the boundaries lie; the solid wall is a pure Neumann (zero normal flux) boundary. |
| Technical | `10.48550_arXiv.1312.3300` — Revol & Théveny, *Numerical Reproducibility and Parallel Computations: Issues for Interval Algorithms*, 2014 | Non-associativity of float addition is the cause of irreproducibility; a reduction's result depends on its order. |

## Craft principle

Olfati-Saber, Fax & Murray state the invariant directly, for the continuous protocol
`ẋ_i = Σ_{j∈N_i} a_ij (x_j − x_i)`:

> "Assuming that the graph is undirected (aij = aji for all i,j), it follows that the sum of the state of
> all nodes is an invariant quantity, or Σ_i ẋ_i = 0."

and in discrete time, for the Perron matrix `P = I − εL`:

> "If G is a balanced digraph, then 1 is the left eigenvector of L, or 1ᵀL = 0. This means that
> 1ᵀP = 1ᵀ − ε1ᵀL = 1ᵀ which implies the column sums of P are 1."

Column sums of 1 are conservation; row sums of 1 (`P1 = 1`, which every Perron matrix has) are "a uniform
field stays uniform". The old rule is the paper's alternative form `x(k+1) = [(1 − ε)I + εD⁻¹A] x(k)` —
the neighbour mean, row-stochastic but not column-stochastic — whose limit is the degree-weighted
average `α = (Σ_i d_i x_i)/(Σ_i d_i)`, and of which the paper warns:

> "the use of normal Laplacians does not render the average x̄ = (1/n)Σ_i x_i invariant in time, unless
> all graphs in Γ are d-regular (all of their nodes have degree d). This is hardly the case for various
> applications."

A floor grid with walls is never d-regular, and `α` weighting each deposit by its cell's degree is
exactly the measured dead-end (degree 1 → 0.27) and corner (degree 2 → 0.54) loss. MODFLOW's
control-volume form says where the coefficient has to live for the exchange to be symmetric:

> "The CVFD equations in MODFLOW 6 use equivalent conductances between nodes of adjacent cells— that is,
> "branch conductances"—rather than conductances defined within individual cells."

with the flow into cell `n` from cell `m` being `Q_{n,m} = C_{n,m}(h_m − h_n)`. The 0.2.1 weight `d/n_i`
is a conductance defined within cell `i`; the fix is one weight per floor–floor edge, shared by both ends.

The step bound comes from the same paper: with `Δ` the maximum degree, `P` is nonnegative for
`ε ≤ 1/Δ`, and

> "The condition ε<1/Δ in part iv) is necessary. If an incorrect step-size of ε=1/Δ is used. Then, P
> would no longer be a primitive matrix because it could have multiple eigenvalues of modulus 1."

## How shipping games do it

Couvidou, on how *Dishonored 2*'s chase reads its influence map:

> "The resulting chase destination is then determined by computing the mean position of the warm cells in
> the map (their centroid), weighted by their values. If that point is located outside the warm cells, we
> pick the nearest one. This gives the NPC a goal to chase that follows the level topology instead of
> hitting the nearest wall"

A value-weighted centroid is a mass reading. Under 0.2.1 a corridor's or a dead end's value is scaled
down by its degree every tick while open floor is scaled up, so the centroid of such a field drifts out of
corridors and toward open rooms — the opposite of following the level topology. Lewis, on the grid maps
this crate implements:

> "Observe how the obstacles (dark squares) interact with the propagation of the influence."

Obstacles should shape where influence goes, not how much of it exists.

Harris, GPU Gems ch. 38, on the same stencil problem in a shipped-technique setting:

> "Incorporating arbitrary boundaries requires applying the boundary conditions (discussed in Section
> 38.3.3) at arbitrary locations. This means that at each cell, we must determine in which direction the
> boundaries lie in order to compute the correct boundary values."

and the wall condition for a conserved quantity:

> "The correct solution of the Poisson-pressure equation requires pure Neumann boundary conditions:
> ∂p/∂n = 0. This means that at a boundary, the rate of change of pressure in the direction normal to the
> boundary is zero."

This crate already knows, per floor cell, in which directions the walls lie (the 4-bit floor-neighbour
mask). A missing neighbour has to mean "no flux across this face", which is the Neumann condition; 0.2.1
instead dropped the face from the average and renormalised, which is neither Neumann nor conservative.

## Model adopted

Per floor cell `i`, with `q = d/4` (`d` = `ChannelDef::diffuse`), `n_i` its floor-neighbour count and
`r` the evaporation retain factor applied to every value read:

`out_i = r·v_i · (1 − n_i·q) + q · Σ_{j∈N(i)} r·v_j`

- Each floor–floor edge exchanges `q·(v_j − v_i)` symmetrically: the Perron matrix `I − qL` of the floor
  graph. Rows and columns sum to 1, so the floor total is conserved to float rounding and a uniform field
  stays uniform. A wall face exchanges nothing (Neumann).
- **For a cell with four floor neighbours the formula, and its float operations, are 0.2.1's:**
  `1 − 4q = 1 − d` exactly, and `(Σ/4)·d` and `Σ·(d/4)` round the same real product. Only cells touching
  a wall, rock or the grid edge compute different bits. Checked per tick on a walled map at six `d` and
  three `r` (`/tmp/s2`, numpy float32): 0 of 1 815 interior cells differ, all 975 boundary cells do.
- The neighbour sum keeps its +x, −x, +y, −y order. The per-cell division `Σ/n` is gone.
- Still a pure stencil over disjoint output slots: the cell reads the previous grid, writes its own slot,
  and no accumulator spans cells. Chunk and thread independence are untouched.

Rejected: the explicit per-edge flux sum `v_i + Σ_j q·(v_j − v_i)` (the per-edge terms are exactly
antisymmetric in float, but the sum still rounds per cell, so it conserves no better — 1.0e−7 vs 7.6e−9
relative over 1000 ticks in the prototype — and it changes every interior bit); and the per-cell
`v_i + q·(Σ − n_i v_i)` (1.2e−7, also changes interior bits).

## Parameters

| Parameter | Value | Source |
|---|---|---|
| per-edge weight `q` | `diffuse / 4` | interior cells unchanged from 0.2.1 (design default) |
| wall / rock / grid-edge face | zero flux | Harris (Neumann); MODFLOW no-flow cells |
| `diffuse` range | `[0, 1]`: `1 − n_i·q ≥ 0`, so no negative value is produced | Olfati-Saber et al. `ε ≤ 1/Δ`, `Δ = 4` |
| `diffuse = 1` | allowed, as before, but `ε = 1/Δ`: an interior checkerboard does not decay | Olfati-Saber et al., Remark 3 |

## Validation targets

- **Conservation** (issue #7 acceptance): evaporation 0, no deposits, 1000 ticks — `|Δtotal/total| ≤ 1e−5`
  on the open map, the walled map with a gap and the sealed map. Permanent test.
- **Decay with diffusion:** after `k` ticks at evaporation `e`, the floor total is `(1 − e·dt)^k` of the
  start within `1e−5`, on a walled map.
- **Where a deposit is laid does not change how much of it exists:** the settled total of one deposit at a
  dead-end tip, a room corner, a corridor and a room centre all equal the deposit within `1e−5`. Permanent
  test (dead end, the case 0.2.1 got most wrong).
- **Uniform stays uniform:** a uniform field over a walled floor is unchanged after 1000 ticks within
  `1e−6`.
- **Walls:** rock cells stay exactly 0; across a sealed wall nothing arrives; with a gap, value reaches the
  far side only through it and the gradient on the near side points toward the source.
- **No negatives, no NaN** after 10 000 ticks of random deposits including off-grid ones, at `d` up to 1.
- **Determinism:** bench `determinism` hashes equal at caller, 1, 2 and the default thread count; the
  permanent chunk-count test passes unchanged.
- **Cost:** no case of the bench more than 5% slower (median of three medians); the division is removed,
  so the stencil should be no slower.

## What great looks like

A designer lays scent at the end of a dead-end corridor and in the middle of a hall, and both fade at the
rate the channel's `evaporate` says, no faster: the corridor carries its scent along at the same per-edge
rate a room does, and what reaches a wall stops there and stays. A game that sums the field ("how much
alarm is in this room"), takes its value-weighted centroid (Couvidou) or compares a restored save gets
deposits minus evaporation, whatever the floor plan. In `trail`, the scent spreads through the gap and
nothing ever appears in the wall column; a cell against the wall reads like the open floor beside it,
because the wall face neither adds nor takes value (zero normal flux).

## Deviations

- The diffusion rule changes from "blend toward the floor-neighbour mean" to "exchange `diffuse/4` of the
  difference with each floor neighbour" (Olfati-Saber et al.; MODFLOW). Interior cells are bit-identical to
  0.2.1; every cell beside a wall, rock or grid edge changes, and so does every field downstream of one.
  Release-noted.
- A cell's diffusion toward a wall is now zero rather than redistributed: in a 1-wide corridor `diffuse`
  moves `diffuse/4` per neighbour per call, half of 0.2.1's `diffuse/2`. That is the per-edge rate open
  floor always had; the corridor was the outlier.

## Pending index

- `10.1109_jproc.2006.887293` (Olfati-Saber et al.): searchable in distill and quoted from its chunks, but
  it has no catalog entry and `markdown_read` reports no markdown; repair the catalog row
  (`catalog_repair`) in a later pass.
