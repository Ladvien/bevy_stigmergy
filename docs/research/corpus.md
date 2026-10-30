# Reading list, by domain

Stems are home-still document ids (`distill_search`, `markdown_read`). A stem marked *(abstract)* is not in
the corpus yet; see the note that cites it.

## Model — stigmergy, pheromones, reaction-diffusion, cellular automata

- `10.1109_access.2019.2927815` — Dourvas, Sirakoulis & Adamatzky 2019, *Parallel Accelerated Virtual
  Physarum Lab Based on Cellular Automata Agents* (IEEE Access). The determinism basis: a per-cell
  diffusion stencil on double buffers.
- `10.1016_j.jtbi.2012.06.017` — Bonifaci, Mehlhorn & Varma, *Physarum can compute shortest paths*.

## Shipping games — influence maps, scent and trail systems

- `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` — Lewis, Game AI Pro 2
  ch. 29: placement, diffusion, breadcrumb decay, propagation cost.
- `GameAIPro2_Chapter30_Modular_Tactical_Influence_Maps` — Mark, Game AI Pro 2 ch. 30: propagation
  falloff curves, layered maps.

## Technical — parallel stencils, job systems, nested pools, SIMD

- `Game Engine Architecture, Third Edition, 3rd Edition -- Jason Gregory -- …` — §8.6 job systems.
- `Rust for Rustaceans -- Jon Gjengset -- 2021 -- …` — worker pools and work stealing (rayon).
- `10.48550_arxiv.2512.04320` — Yan et al. 2025, *VLCs: Managing Parallelism with Virtualized Libraries*
  (downloaded, conversion pending).
- `10.1145/1806596.1806639` *(abstract)* — Pan, Hindman & Asanović 2010, *Composing parallel software
  efficiently with Lithe* (PLDI): oversubscription between libraries' pools.
- `10.1109/ipdps.2010.5470434` *(abstract)* — Iancu et al. 2010, *Oversubscription on multicore
  processors*.
