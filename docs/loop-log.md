# bevy_stigmergy loop log

## Iteration 0 — 2026-10-30 — loop setup

By the owner's session, not the loop. The repository gets its Wiggum loop (`.omp/commands/build-loop.md`), on sib beside the bevy_gooey loop and bevy_emerge's w8. The crate is at `v0.2.0` on crates.io (2026-10-25: Bevy 0.20, no panicking index, `CellFrame`, restore).

### Ranked backlog

Take these in order, after any bug and any filed issue (section "Filed issues come before new features"):

1. **#6** (`consumer`, `consumer:sanctuary`, `performance`): `evaporate_diffuse` runs its diffusion on rayon's global pool, so a Bevy worker that calls it parks while another pool does the work; Sanctuary measured social-field ticks hitching up to 150 ms. Sanctuary's perf loop waits on this fix and a pin move. Prove the stall before and after with the caller's wall clock from inside a busy pool (section 2), and keep the stencil thread-count-independent.
2. `examples/bench.rs`: the throughput, caller-wall-clock and determinism-hash lines section 2 describes. #6 needs it first, so build it as that item's first slice.
3. A cost that scales with live cells, not the whole grid, for large mostly-empty maps.
4. Per-channel diffusion rates and anisotropy, if a consumer's issue or the research asks for them.
5. Capture knobs (`STIG_FRAMES`, `STIG_SHOT`) on the examples, and a fresh `docs/foraging.gif` when behaviour changes.

## Iteration 1 — 2026-10-30 — the tick runs on the caller's thread (#6)

Pass 6 of the driver (passes 2–5 failed at start-up with no model login; pass 1 was cut off). **Adopted pass 1's parked tree** (`loop/iter-1`, `fa78311`, `WIP(pass 1, parked)`): its research commits are the base of this branch (`loop/iter-1-pass6`, from `4b44569`), its `src/`, bench, test and doc drafts were brought in with `git checkout fa78311 -- …`, then reworked, built and gated here. The parked branch itself is not merged and is left in place.

**Triage.** Open issues: #6 only (`consumer`, `consumer:sanctuary`, `performance`) — the item, ahead of everything in the backlog. No bug issue was open. Found during the pass and filed: **#7**, diffusion creates and destroys mass at walls (below).

**What changed and why it ranked first.** #6 is a consumer telling us the field stalls its frame: `evaporate_diffuse` ran the stencil on rayon's global pool and parked the calling Bevy worker on rayon's latch. Runner-up was the backlog's "cost that scales with live cells", an enhancement nobody filed.

- `evaporate_diffuse` runs **inline on the calling thread**; `rayon` leaves `[dependencies]` (dev-dependency only).
- `evaporate_diffuse_with(dt, chunks, run)` + `DiffuseChunk` / `DiffuseChunks`: the same pass handed to the caller's pool as disjoint chunks; `evaporate_diffuse` is it with one chunk run in place. A chunk the runner forgets is run inline afterwards.
- Since the cost is now the caller's, the pass **writes each diffusing channel once instead of three times** (evaporation fused into the stencil read, chunks write straight into the double-buffer, 4-bit floor-neighbour mask per cell). Bit-identical to 0.2.0.
- The floor list is read off the floor mask: any order, repeats count once (0.2.0 evaporated a repeated cell twice per tick — fixed).
- `examples/bench.rs` (new): throughput, the busy-pool case from a real Bevy `App` system, determinism hashes.
- Assumed decision (owner may override), posted on #6 with label `assumed-decision`: inline default plus caller-run chunks; `rayon` removed rather than feature-gated (one path).

**Sources.** Model: `10.1109_access.2019.2927815` (Dourvas, Sirakoulis & Adamatzky 2019). Shipping game: `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` (Lewis), plus Gregory's *Game Engine Architecture* §8.6.4 on one job system. Technical: `10.48550_arxiv.2606.24937` (roofline: memory-bound kernels move fewer bytes), Lithe `10.1145/1806596.1806639` (abstract), VLCs `10.48550_arxiv.2512.04320` (pending conversion). Note: `docs/research/diffusion-scheduling.md`. Corpus: three model-domain additions from `catalog_recent`.

**home-still.** distill healthy (MCP; one 30 s timeout then answered); `hs` CLI on this host has no `distill.servers` configured. Scribe: big's backend unreachable, bmb healthy and full (3 converting, 892 queued).

**Field checklist** (`/tmp/s1/findings.md`; before = 0.2.0, after = this branch; identical unless noted):

- conservation, evaporation 0, 1000 ticks: open room **+1.753e-2**, walled room **−8.256e-2** (worst −9.07e-2), before and after — not conserved; filed **#7**.
- walls: rock max 0; nothing crosses a sealed wall; with one gap the across-wall cell reads 3.56e-5 against 2.99e-2 at the gap, gradient points to the gap.
- decay: 0.121576592 vs (0.9)^20 = 0.121576655, rel err −5.1e-7.
- 10 000 ticks × 16 deposits incl. 118 518 off-grid / undefined-channel: no panic, 0 non-finite, 0 negative.
- gradient on ramp 2x + y: (4, 2), 0.0000 rad off.
- determinism: `d791495f2f937977` at caller, Bevy 8, rayon 1/2/16, before and after; every throughput hash equal before/after.
- rayon's global pool saturated: before **never finished (5 s timeout)**, after 0.000–0.002 s.
- floor set reversed + repeated: before **differs** (total 31.71 vs 49.25), after identical bits and hotspot.

**Bench** (3 interleaved rounds per set; set 1 load 5→75, set 2 load 58–77; medians of medians, µs; `/tmp/s1/bench*/`):

| Case | Before | After |
|---|---|---|
| busy_pool 175² ch2, Bevy system, 7 of 8 workers busy (set 2, rayon default) | median 18 733–20 688, p99 55 626–64 902, max 97 065, parked 1000/1000 | median 245–255, p99 282–297, max ≤3 591, parked 0/1000 |
| busy_pool, all 12 runs per side | parked 12 000 / 12 000 ticks | parked 0 / 12 000 ticks; p99/median ≤ 2 (1.04–1.93) in 7 of 12 runs; the other 5 (6.67–12.83) had 13–228 OS-preempted ticks at load 13–77 |
| inline vs 0.2.0 serial, 64²/256²/1024² × 1/4/8 ch | — | 0.33–0.46× (set 1), 0.12–0.49× (set 2) |
| caller-run rayon/Bevy vs 0.2.0 internal parallel | — | 0.21–0.73× (set 1), 0.03–0.48× (set 2) |
| 1024² ch8, quiet round (load ≈6) | 0.2.0 default 27 667 | inline 32 235 (**1.17×, logged trade**), Bevy 6 042, rayon 6 073 |
| rally_evaporate | 64² 1.2, 256² 63, 1024² 1 137 | bench binary +30% in 6/6 A/B pairs, but the same loop timed alone in one harness: 1.74→1.79, 30.18→19.66, 2039.9→2028.7 (bimodal in both; no code change in that path) |

**Critique and verdict** (`/tmp/s1/critique.md`): **GREAT**. The trade: on a quiet host the *default* call on a 1024² map is 1.17–1.23× slower than 0.2.0's parallel default; `evaporate_diffuse_with` is 0.22–0.35× there, and under load the inline default already wins (0.25–0.32×). Flaws kept: one `run` (one Bevy scope) per diffusing channel; the Bevy form can still wait on its own pool (2, 27 and 2 parked ticks in three of twelve `busy_pool_with_bevy` runs); `tests/leaf.rs` still allows `rayon` and its doc comment says the crate takes it — narrowing a protected test is the owner's edit.

**Gate:** build, build all-features, release, test, test all-features, examples, clippy `-D warnings`, rustdoc `-D warnings`, fmt, wasm32 — all green on `9641b32`. No visible behaviour moved (field bits identical), so `docs/foraging.gif` is unchanged.

**Commits:** `b26b359` `4b44569` (pass 1 research), `9e4cf38` corpus, `758f283` research update, `c20afea` implementation (`Fixes #6`), `9641b32` docs, `01f6119` this log; merged as `3027b5f`, gate re-run green on the merged tree, pushed. #6 closed as completed by the push; release and measurements commented on it.

**Release:** `cargo semver-checks check-release` — 202 checks pass, no semver update required → patch. `f73e7d3` `release: v0.2.1`, published, tagged `v0.2.1`, `cargo info bevy_stigmergy@0.2.1` resolves from the registry: https://crates.io/crates/bevy_stigmergy/0.2.1. (`cargo-semver-checks` 0.51.0 installed into `~/.cargo/bin`; a first install attempt sat blocked for ~50 min behind other loops' cargo locks and stalled a gate run with it, so it was killed and rerun.)

**Cleanup:** removed `~/s1p6_base` (worktree, 1.7 MB) and `~/s1p6_base-target` (2.0 GB), the stray `checklist` artifacts a mis-targeted build left in `~/target/stigmergy-loop/release` (the env sets `CARGO_TARGET_DIR` there), and `/tmp/s1` (1.3 GB on tmpfs: `/tmp` 2.4 GB → 1.1 GB used). `/home` reads 681 GB free before and after: other loops built through the pass. **Left, from pass 1:** `~/s1_base` (1.7 MB, detached at `5b47d37`, which is on `origin/main`; not clean — untracked `examples/bench.rs` and `tests/caller_thread_before.rs`; untouched for over 2 h) and `~/s1_base-target` (2.3 GB). Also left: branch `loop/iter-1` (local and origin) holding the parked WIP commit `fa78311`, now superseded by `loop/iter-1-pass6`.

### Ranked backlog

1. **#7 (bug)**: diffusion is not conservative — +1.75% / −8.3% per 1000 ticks with evaporation 0. A symmetric per-edge flux (`v + (d/4)·Σ(v_nb − v)`) conserves to rounding; it changes every consumer's bits, so it is a release-noted change. Research the CA diffusion form first.
2. **Bug, not filed:** `deposit` accepts NaN/∞ `amount`s and poisons the channel forever (`set_channel` already filters them). Refuse non-finite deposits.
3. One `run` call per tick for every diffusing channel (one scope instead of N), which needs a double-buffer per channel; measure the per-scope cost first (64²: 8 µs inline vs 17 µs on Bevy's pool).
4. A cost that scales with live cells, not the whole grid, for large mostly-empty maps.
5. Bench hygiene: run `rally_evaporate` in its own process or first, so the cases before it stop biasing it; then capture knobs (`STIG_FRAMES`, `STIG_SHOT`) on the examples.

## Iteration 2 — 2026-10-30 — diffusion conserves value at walls (#7)

**Triage.** Open issues: #7 only (`bug`), the item. No `consumer` or enhancement issue was open, so nothing was skipped. Parked work: no `wiggum/parked-*` branch; the only `parked` commit is pass 1's `fa78311` on `loop/iter-1`, superseded by iteration 1 and ignored. `git status` clean on `main` at start. Corpus re-scan: `catalog_recent` (200) held only forest-ecology and psychology embeds, nothing in this crate's domains. Filed during the pass: **#8** (`bug`), `gradient_cell` reads walls as zero (below).

**What changed and why it ranked first.** #7 is a wrong result in every consumer: the stencil blended each floor cell toward the mean of its floor neighbours, which is not symmetric wherever a cell has fewer than four, so value was made or destroyed beside every wall, rock and grid edge. The runner-up was the backlog's NaN-deposit refusal, also a bug but one only a host bug triggers; #7 fires on every map with a wall.

- `DiffuseChunk::run` now computes `own·(1 − n·d/4) + (d/4)·Σ neighbours`: every floor–floor edge exchanges `d/4` of the difference both ways and a wall face exchanges nothing (the Perron matrix `I − (d/4)L` of the floor graph). The per-cell division is gone.
- A cell with four floor neighbours computes the same bits as 0.2.1 from the same input (`d/4` and `1 − 4·(d/4)` are exact); boundary cells differ. Rust one-tick dump: 0 of 1 824 interior cells differ, 971 of 971 boundary cells do, rock 0.
- Three permanent tests in `src/lib.rs` (`diffusion_moves_value_without_making_or_losing_any`, `with_diffusion_on_the_total_decays_at_exactly_the_evaporation_rate`, `a_uniform_field_stays_uniform_beside_walls`); the first two fail on 0.2.1 (−9.8%, −11.7%, −35%; +6.0%), the third guards the other half of a symmetric exchange.
- `tests/caller_thread.rs`: its plain two-pass reference tick encoded the old rule; it now encodes the new one, assertion unchanged (bit-for-bit, inline and 5 chunks). **Assumed decision (owner may override)**, posted on #7 with label `assumed-decision`: update the oracle rather than delete the test or leave the bug; and the `own·(1 − n·q) + q·Σ` form over the per-edge flux sum (same conservation, keeps interior bits).
- Docs: `ChannelDef::diffuse` and `evaporate_diffuse` rustdoc, README "Units and time", CHANGELOG `Fixed`, `docs/research/conservative-diffusion.md` (new), pointer in `diffusion-scheduling.md`.

**Sources.** Model: `10.1109_jproc.2006.887293` (Olfati-Saber, Fax & Murray 2007: symmetric exchange keeps the sum; the neighbour-mean form keeps only a degree-weighted average; `ε < 1/Δ`), `10.3133_tm6a55` (MODFLOW 6: one conductance per cell pair, no-flow cells). Shipping game: `gameaipro4-ch06-flooding-influence-map-chase-dishonored-2` (Couvidou: the chase goal is the value-weighted centroid of the map), `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` (Lewis). Technical: GPU Gems ch. 38 (Harris, stem `GPU Gems- Programming Techniques, Tips and Tricks for -- Randima Fernando; …`: per-cell boundary directions, pure Neumann walls), `10.48550_arXiv.1312.3300` (Revol & Théveny: float non-associativity). All five new stems added to `corpus.md`.

**home-still.** distill healthy (MCP, `bge-m3`, 18 070 documents). Scribe: big's backend unreachable; bmb healthy and full (3 converting, 881 queued). Pending index: `10.1109_jproc.2006.887293` is searchable in distill but has no catalog row and no markdown (`catalog_read`/`markdown_read` fail).

**Field checklist** (`/tmp/s2/findings.md`; before = `e1b21f6` (0.2.1), after = this branch):

| Check | Before | After |
|---|---|---|
| conservation, evap 0, 1000 ticks: open / walled+gap / sealed | +1.932e-2 / −7.004e-2 (worst −7.988e-2) / −7.003e-2 | −1.3e-8 / −6.8e-8 / −8.1e-8 |
| 100 000 ticks, walled, evap 0 | −7.21e-2 | +1.28e-4 (float32; the per-edge flux form: −8.6e-5) |
| one deposit of 1.0, settled (20 000 ticks): room centre / corridor / dead-end tip / room corner | 1.0758 / 0.5378 / 0.2689 / 0.5379 | 1.0002 / 0.9999 / 0.9999 / 1.0001 |
| total vs `(1 − 0.1)^20`, walled, with diffusion | +7.763e-3 | −4.740e-7 |
| single-cell decay vs `(0.9)^20` | −5.1e-7 | −5.1e-7 |
| rock max; sealed wall far side | 0; 0 | 0; 0 |
| gap open: far-side total; near-side gradient toward source | 1.858; 31/31 | 1.235; 31/31 |
| soak 10 000 ticks × 16 deposits (87 952 refused) | 0 NaN, 0 negative | 0 NaN, 0 negative |
| gradient on ramp 2x + y | (4, 2), 0 rad | (4, 2), 0 rad |
| uniform 1.0 over walled floor, 1000 ticks | max dev 0 | max dev 0 |
| chunk counts 1–10 000 reversed; bench determinism (caller, Bevy 8, rayon 1/2/16, 9 runs) | one hash each (`c3b6…`; `d791495f2f937977`) | one hash each (`b408…`; `a446265d6a95cf51`) |
| foraging HOME beacon settled: total (analytic 6251.1) | 6371.2 | 6250.7 (max change 0.74% of peak) |

**Visible.** `trail` (`/tmp/s2/{before,after}/trail.txt`): the gap cell reads 0.474 (was 0.395) and the walked row through it `-----=+=--` (was `------==--`); the far-side wall cells (25,5) and (25,11) no longer light up. `docs/foraging.gif` not regenerated: the settled beacon moves ≤ 0.74% of its peak, the example has no capture knob yet, and the caption still describes the file.

**Bench** (`/tmp/s2/{before,after}/bench-r*.txt`; interleaved, 3 rounds `RAYON_NUM_THREADS=1` + 6 rounds default, load 6–10; median of medians, µs):

| Case | Before | After |
|---|---|---|
| inline 64² ch1 / ch4 / ch8 (9 runs) | 8.0 / 32.6 / 65.4 | 7.2 / 29.0 / 58.7 (0.89–0.90×) |
| inline 256² ch1 / ch4 / ch8 | 223.9 / 881.3 / 1747.1 | 218.1 / 849.2 / 1696.9 (0.96–0.97×) |
| inline 1024² ch1 / ch4 / ch8 | 3647 / 14918 / 29252 | 3483 / 14327 / 28234 (0.96×); p99 ch8 36 107 → 33 055 |
| Bevy pool 1024² ch1 / ch4 / ch8 (6 default runs) | 779 / 3258 / 6254 | 752 / 3114 / 6211 |
| Bevy pool 256² ch1 | 71.1 (6 default) / 73.3 (all 9) | 76.5 (1.08×) / 75.5 (1.03×); fastest 68.1 → 64.8: pool noise, logged |
| rayon default 1024² ch8 | 6066 | 6241 (1.03×) |
| busy_pool 175² ch2, Bevy system, 7 of 8 workers busy (6 default runs) | median 287, p99 347; parked 0/1000 in every run (10) | median 268, p99 286; parked 0/1000 in every run (9) |
| rally_evaporate (code untouched) | bimodal | bimodal (0.35–2.32× between rounds) |

**Critique and verdict** (`/tmp/s2/critique.md`): **GREAT.** Flaws kept: float32 rounding walks the total ~1e-4 per 1e4–1e5 ticks at evaporation 0 (form-independent); every boundary cell's bits change (release-noted); a 1-wide corridor now diffuses at the open-floor rate `d/4`, half of 0.2.1's; a narrow gap holds a walked trail longer and reads as a local maximum; `diffuse` outside `[0, 1]` or NaN is documented but not refused; `diffuse = 1` keeps the checkerboard mode (documented); `gradient_cell` still reports a slope off every wall on a flat field (filed as #8).

**Gate:** build, build all-features, release, test, test all-features, examples, clippy `-D warnings`, rustdoc `-D warnings`, fmt, wasm32 — all green on `34c0249`.

**Commits:** `31a338e` research note, `cf49bbe` corpus, `b0a157d` implementation + tests (`Fixes #7`), `29c6e4b` reference-tick oracle (assumed decision), `34c0249` docs, then this log.

**Release:** none this pass. `v0.2.1` was tagged at 14:11 today, 38 minutes before this pass reached the release step, and the rule is one release per 6 hours; this change rides in the next release.

### Ranked backlog

1. **#8 (bug)**: `gradient_cell` reads rock and off-grid as 0, so on a flat field it reports a slope off every wall (927 of 2795 cells, max 1.414); a fleeing agent is pulled into walls. Make the query agree with the zero-flux wall (a missing neighbour reads as the cell itself), interior unchanged.
2. **Bug, not filed:** `deposit` accepts NaN/∞ `amount`s and poisons the channel forever (`set_channel` already filters them). Refuse non-finite deposits.
3. **Bug, not filed:** a `ChannelDef` with `diffuse` above 1 (negative values) or NaN (a poisoned channel) is used as given. Decide clamp vs. refuse at `StigGrid::new`, and the same for `evaporate`.
4. One `run` call per tick for every diffusing channel (one scope instead of N), which needs a double-buffer per channel; measure the per-scope cost first (64²: 7 µs inline vs 11–17 µs on Bevy's pool).
5. A cost that scales with live cells, not the whole grid, for large mostly-empty maps; then capture knobs (`STIG_FRAMES`, `STIG_SHOT`) and a fresh `docs/foraging.gif`.
