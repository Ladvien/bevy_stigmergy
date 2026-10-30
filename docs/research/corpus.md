# Reading list, by domain

Stems are home-still document ids (`distill_search`, `markdown_read`). A stem marked *(abstract)* is not in
the corpus yet; see the note that cites it.

## Model — stigmergy, pheromones, reaction-diffusion, cellular automata

- `10.1109_access.2019.2927815` — Dourvas, Sirakoulis & Adamatzky 2019, *Parallel Accelerated Virtual
  Physarum Lab Based on Cellular Automata Agents* (IEEE Access). The determinism basis: a per-cell
  diffusion stencil on double buffers.
- `10.1016_j.jtbi.2012.06.017` — Bonifaci, Mehlhorn & Varma, *Physarum can compute shortest paths*.
- `10.1038_s41598-017-06961-y` — Evangelidis, Jones, Dourvas, Tsompanas, Sirakoulis & Adamatzky 2017,
  *Physarum machines imitating a Roman road network: the 3D approach* (Sci. Rep.). Foraging transport
  networks over a relief, in the lab and in a model.
- `10.1038_s41598-020-80320-2` — *Substrate and cell fusion influence on slime mold network dynamics*
  (Sci. Rep. 2020). Not yet read; listed for slime-mould trail networks.
- `10.1038_s41598-018-23649-z` — *Diffusion-Limited Growth of Microbial Colonies* (Sci. Rep. 2018). Not
  yet read; listed for growth limited by a diffusing field.
- `10.1109_jproc.2006.887293` — Olfati-Saber, Fax & Murray 2007, *Consensus and Cooperation in Networked
  Multi-Agent Systems* (Proc. IEEE). Diffusion on a graph as a Perron matrix: symmetric exchange keeps the
  sum, the neighbour-mean form keeps only a degree-weighted average; the step bound `ε < 1/Δ`.
  (In distill, no catalog row.)
- `10.3133_tm6a55` — Langevin et al. 2017, *Documentation for the MODFLOW 6 Groundwater Flow Model* (USGS
  TM 6-A55). Control-volume diffusion: one conductance per cell pair, no-flow inactive cells.
- `10.1039_a702602a` — Maini, Painter & Chau 1997, *Spatial pattern formation in chemical and biological
  systems* (J. Chem. Soc., Faraday Trans.). Reaction-diffusion with zero-flux (impermeable) boundaries.
- `10.48550_arxiv.2212.07906` — *Flow-Lenia: Towards open-ended evolution in cellular automata through
  mass conservation and parameter localization* (2022). A continuous CA made mass-conserving. Not yet read.
- `10.48550_arxiv.2305.13784` — *Implementation of Lenia as a Reaction-Diffusion System* (2023). Not yet
  read.
- `10.48550_arxiv.2205.10463` — *Glaberish: Generalizing the continuously-valued Lenia framework to
  arbitrary Life-like cellular automata* (2022). Not yet read.

## Shipping games — influence maps, scent and trail systems

- `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` — Lewis, Game AI Pro 2
  ch. 29: placement, diffusion, breadcrumb decay, propagation cost.
- `GameAIPro2_Chapter30_Modular_Tactical_Influence_Maps` — Mark, Game AI Pro 2 ch. 30: propagation
  falloff curves, layered maps.
- `gameaipro4-ch06-flooding-influence-map-chase-dishonored-2` — Couvidou, *Flooding the Influence Map for
  Chase in Dishonored 2*: a heat flood bounded by walls, read as a value-weighted centroid.
- `10.48550_arxiv.2210.04624` — *WebCrowds: An Authoring Tool for Crowd Simulation* (2022). A crowd
  authoring tool, not a shipped game; listed for crowd-simulation practice. Not yet read.

## Technical — parallel stencils, job systems, nested pools, SIMD

- `Game Engine Architecture, Third Edition, 3rd Edition -- Jason Gregory -- …` — §8.6 job systems.
- `Rust for Rustaceans -- Jon Gjengset -- 2021 -- …` — worker pools and work stealing (rayon).
- `10.48550_arxiv.2512.04320` — Yan et al. 2025, *VLCs: Managing Parallelism with Virtualized Libraries*
  (downloaded, conversion pending).
- `10.1145/1806596.1806639` *(abstract)* — Pan, Hindman & Asanović 2010, *Composing parallel software
  efficiently with Lithe* (PLDI): oversubscription between libraries' pools.
- `10.1109/ipdps.2010.5470434` *(abstract)* — Iancu et al. 2010, *Oversubscription on multicore
  processors*.
- `GPU Gems- Programming Techniques, Tips and Tricks for -- Randima Fernando; … -- isbn13 9780321228321 --
  …` — Harris, GPU Gems ch. 38, *Fast Fluid Dynamics Simulation on the GPU*: grid stencils with
  arbitrary solid boundaries, pure Neumann walls.
- `10.48550_arXiv.1312.3300` — Revol & Théveny 2014, *Numerical Reproducibility and Parallel
  Computations: Issues for Interval Algorithms*: float non-associativity as the cause of irreproducibility.
