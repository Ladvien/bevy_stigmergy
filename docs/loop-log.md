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

**Merged and cleaned up (iteration 2).** Merged as `3c8c608`. The gate was re-run green on the merged tree before `main` was pushed. #7 closed as completed when the push landed, and the results are in a comment on it. Removed: `~/s2_base` (worktree, 1.8 MB) and `~/s2_base-target` (2.3 GB). `/home` free space went from 670 GB to 671 GB, while other loops kept building. Also removed `/tmp/s2` (350 MB on tmpfs: `/tmp` used 1.9 GB → 1.6 GB). **Left from pass 1, as listed before:** `~/s1_base` (1.7 MB) and `~/s1_base-target` (2.3 GB). That worktree is detached at `5b47d37`, which is on `origin/main`. It is not clean (untracked `examples/bench.rs` and `tests/caller_thread_before.rs`) and has been untouched for over 2 h. Branch `loop/iter-1`, which holds parked WIP `fa78311`, is also still in place.

## Iteration 3 — 2026-10-30 — gradient_cell reads a wall as the cell's own value (#8)

The gating pass of an iteration that was cut off. An earlier pass had built the whole fix on `origin/loop/iter-3` (four commits: research, implementation, flag-byte rework, capture knob and docs), but it never reached its gate, its log entry or `main`. **Adopted by checking that branch out and finishing it** (`loop/iter-3` tracking `origin/loop/iter-3`, from `5705947`); the two parked branches (`wiggum/parked-pass-8-*`, whose research and bench draft were already adopted into it, and `wiggum/parked-pass-19-*`, a checkpoint of `src/scalar.rs` and `examples/bench.rs` that the branch's later commits supersede) are left in place and not merged.

**Triage.** Open issues: **#8** (`bug`, `assumed-decision`), **#9** (`bug`), **#10** (`bug`). #8 is the item: the oldest bug, and its implementation already existed. **Skipped, with reasons:** #9 (a NaN or ∞ deposit poisons a channel) and #10 (`deposit` overflows an `i32` on an extreme cell) are bugs of the same rank that cost one item each; they head the backlog below. #10's `gradient_cell` half is closed by this change (checked: `IVec2::MAX` panicked with an add overflow before, reads `Vec2::ZERO` now); its `deposit` half is untouched, so #10 stays open. No `consumer` or enhancement issue was open. Corpus re-scan: `catalog_recent` (200) held only psychology, plant-biology and language-model embeds; nothing to add.

**What changed and why it ranked first.** A rock or off-grid neighbour read as `0.0`, so beside every wall a flat field reported a slope pointing away from it and a fleeing agent was pulled into the nearest wall. The runner-up, #9, only fires on a host bug; #8 fires on every map with a wall.

- `gradient_cell` reads a neighbour that is not floor as the cell's own value (Harris's Neumann boundary value; the face the tick has had since #7): a flat field has no slope, across a wall the component is the one-sided difference from the floor side, a cell with four floor neighbours and the component along a wall are unchanged to the bit.
- A rock or off-grid cell, however far, and an undefined channel read `Vec2::ZERO`; the cell is tested before any neighbour is formed, so a saturated `CellFrame::world_to_cell` no longer overflows (#10's gradient half).
- Both grids keep one flag byte per cell (`IS_FLOOR | NB_*`, the bits `FloorCell` already carried) in place of the `bool` mask: the query is one load for its walls.
- `examples/foraging.rs` gets the capture knob (`STIG_FRAMES`, `STIG_SHOT`, `STIG_EVERY`) and runs on its own step count, so two builds can be compared frame for frame; `examples/bench.rs` gets `gradient_cell_x10000`.
- Four permanent tests in `src/lib.rs`; all four fail on `cb5ebd4` (checked by running them against its sources: 30 passed, 4 failed) and pass here.
- **Found and fixed by this pass:** the adopted tree failed `cargo clippy --all-targets --all-features -- -D warnings` (`manual_is_multiple_of`, `examples/foraging.rs:325`), `ad586ec`.
- Assumed decision (owner may override), already on #8 with label `assumed-decision`: a wall reads as the cell's own value; a rock or off-grid cell reads `Vec2::ZERO`; one flag byte per cell.

**Sources.** Model: `10.1039_a702602a` (Maini, Painter & Chau 1997: zero-flux boundaries), `10.3133_tm6a55` (MODFLOW 6: no-flow cells). Shipping game: `gameaipro4-ch06-flooding-influence-map-chase-dishonored-2` (Couvidou: the extrapolated direction that ran into the nearest wall), `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` (Lewis: influence bounces off an obstacle, it does not vanish into it). Technical: GPU Gems ch. 38 (Harris, `GPU Gems- Programming Techniques, Tips and Tricks for -- Randima Fernando; …`, page 678: "set each pressure boundary value to the value just inside the boundary"). Note: `docs/research/wall-gradient.md`; corpus adds Maini et al. plus three Lenia/CA stems and WebCrowds (not yet read).

**home-still.** `system_status`: distill healthy (`bge-m3`, 18 070 embedded, 488 794 chunks); scribe: big (`192.168.1.110`) healthy, 3 converting, 9 of 12 slots free; bmb healthy, 5 converting, 1 of 6 free; 870 queued. Nothing pending index.

**Field checklist** (`/tmp/s3/findings.md`; before = `cb5ebd4`, after = this branch; the tick is untouched, so every line not about the query is identical):

| Check | Before | After |
|---|---|---|
| conservation, evap 0, 1000 ticks: open / walled+gap / sealed | −1.4e-8 / −1.0e-7 / −6.0e-8 | same |
| rock max; decay vs `(0.9)^20` | 0; rel 0 | same |
| soak, 10 000 ticks × 16 deposits (91 905 off-grid or undefined channel) | 0 non-finite, 0 negative | same |
| uniform field, one tick: floor cells with a slope (of 2 749) | 1 240, max 1.414 | **0** |
| ramp `2x + y`: interior reading `(4, 2)`; boundary cells with a downhill component (of 1 270) | 1 479 / 1 479; 635 | 1 479 / 1 479; **0** |
| field flat along one axis: cells with a slope across it; fleeing points into a flat wall face | 658 and 630; 686 of 1 362 | **0 and 0; 0** |
| random field bit hashes: 1 479 four-neighbour cells, 2 061 x axes, 2 106 y axes | `a5ac…eb6`, `8857…6c4`, `231a…626` | equal |
| 200 agents, 400 steps, source settled by the tick: climbers within 3 cells (mean geodesic distance from 51.9) | 20 (40.2) | **200 (0.5)** |
| the same, fleeing: at rest with a farther floor cell beside them (mean distance, farthest cell 91) | 67 (66.2) | **9 (74.1)** |
| exact field `exp(−d/25)`: climbers within 3 cells; fleeing at rest short | 0; 76 | 182; 39 |
| `IVec2::MAX`, `i32::MAX` on one axis, rock, off-grid, undefined channel | `IVec2::MAX` panics (add overflow) | all `Vec2::ZERO` |
| bench `determinism`, 6 runs × 2 sides × 5 lines | `a446265d6a95cf51` | `a446265d6a95cf51`; every throughput hash equal except `gradient_cell_x10000`'s (new slopes, by design) |

**Bench.** The bench binary alone cannot gate 5% on this host: it is a macOS arm64 machine at load 5–13 on 14 cores, and one process reads the same 64² one-channel tick at 10 µs or 26 µs depending on the core (P or E) it lands on, on both builds (`/tmp/s3/before|after/bench-r1..6.txt`: median of six, 64² ch1 inline 10.7 → 25.8, an artifact: the tick is untouched). `busy_pool` medians: 663.2 → 663.8 µs; its `parked_ticks` is Linux-only (`n/a`). So the gate was decided by an **in-process, interleaved A/B** of the old crate (renamed) and the new one (`/tmp/s3/ab`, three runs of 600 batches):

| Case | New / old, median | p10 |
|---|---|---|
| `gradient_cell` × 10 000, 64² / 256² / 1 024² | 0.81–0.86 / 0.80 / 0.93–0.95 | 0.67–0.80 / 0.80–0.81 / 0.86–0.92 |
| `deposit` × 1 000, 64² / 256² / 1 024² | 0.955–1.007 / 0.983–1.007 / 0.996–1.004 | 0.99–1.01 |
| inline tick ch4, 64² / 256² (control: code untouched) | 0.999–1.000 / 0.985–1.012 | 1.000–1.001 / 0.992–1.038 |

One regression, logged: at 64² the query's p90 is 13–15% slower (142–148 → 163–167 µs) while its median is 14–19% faster.

**Critique and verdict** (`/tmp/s3/critique.md`): **GREAT** for `gradient_cell` at walls and for the capture knob. Flaws kept: across a wall the component is half a ramp's open-floor scale (a Deviation, documented); a climber beside a wall with a lower floor-side neighbour and tied along-wall neighbours reads a gradient into the wall and no along-wall component (18 of 200 on the exact tie-ridden field, 0 of 200 on a field the tick made; sliding is the agent's rule, not the query's); the 64² p90 above; `parked_ticks` cannot be read on this host; `tests/leaf.rs` still says the crate takes `rayon` (protected). **Stills** (`/tmp/s3/shots`, `cb5ebd4`'s query against the new one, steps 900 and 3 600 read, 1 800 and 2 700 captured): the same route through the left gap, the same dead-end spur at the right gap and wandering mesh beyond the wall at 900; at 3 600 both hold the left route, in different phases of the cycle. `docs/foraging.gif` not regenerated: the route is unchanged and its caption describes what the file shows.

**Gate:** build, build all-features, release, test, test all-features (34 unit tests, 4 caller-thread, 2 leaf, 1 doctest), examples, clippy `-D warnings`, rustdoc `-D warnings`, fmt, wasm32 — all green on `5b42dc7` (clippy red on `5705947`, fixed in `ad586ec`).

**Environment trouble, for the next pass.** The disk filled during the pass (`write() failed, errno=28` linking `foraging`; 209 MB free): two Bevy builds at once took 66–70 minutes each at load 11 and `~/target/stigmergy-loop` is 49 GB. Building a second worktree **into the same `CARGO_TARGET_DIR`** makes cargo treat the first tree's library as fresh (same package id, older mtimes): the baseline's `foraging` binary was byte-identical to the branch's until every `src/*.rs` was touched. Give a baseline its own target directory, or `touch` the tree before building it.

**Commits:** `a7eb6a0` corpus, `9a8133b` research, `9c42e62` implementation (`Fixes #8`), `4c1550d` flag byte, `3e1417b` capture knob, `5705947` docs (all by the earlier pass); `ad586ec` clippy fix, `5b42dc7` measured numbers (this pass), then this log.

**Release: not done.** More than six hours have passed since `v0.2.1` (14:11), and `src/**` changed, so one is due, but this host cannot publish: there is no `~/.cargo/credentials*` and no `CARGO_REGISTRY_TOKEN`, and `cargo-semver-checks` is not installed (a build of it on a disk with 1.8 GB free would risk the next ENOSPC). The change adds no public item and removes none, so it would be a patch (0.2.2) carrying #7 and #8; both stay under `## [Unreleased]`. `cargo publish --dry-run` on the merged tree packages 12 files (250.3 KiB, 68.4 KiB compressed), verifies and stops at the upload: the package is publishable the moment a host with credentials runs the release step.

### Ranked backlog

1. **#9 (bug)**: `StigGrid::deposit` and `RallyGrid::deposit` write a NaN or ±∞ `amount` unchecked and diffusion spreads it over the whole channel forever (`set_channel` already refuses non-finite values). Refuse non-finite deposits, the same way an undefined channel is a no-op.
2. **#10 (bug, deposit half)**: `deposit`/`deposit_disc` form `center + offset` in `i32` and walk an unclamped (2r+1)² box: overflow on an extreme cell in a debug build, and a huge radius walks off-grid cells. Clamp the box to the grid before the loop.
3. **Bug, not filed:** a `ChannelDef` with `diffuse` outside `[0, 1]` or NaN, or a NaN `evaporate`, is used as given. Decide clamp vs. refuse at `StigGrid::new`.
4. One `run` call per tick for every diffusing channel (one scope instead of N); then a cost that scales with live cells, not the whole grid, for large mostly-empty maps.
5. Bench hygiene: a P-core pin or a long single-core warm-up so macOS runs are comparable (and `parked_ticks` on a Linux host); a regenerated `docs/foraging.gif` from the capture knob when the behaviour next moves.

**Merged and cleaned up (iteration 3).** Merged as `806a449` (`Fixes #8`); the gate was re-run green on the merged tree (all ten steps, exit 0) before `main` was pushed, and #8 closed as completed when the push landed. The measurements are in a comment on #8, and #10 has a comment saying its `gradient_cell` half is fixed and its `deposit` half is not. Removed: `~/s3_base` (worktree, 1.8 MB; the two example files I had copied into it were restored first, then a plain `git worktree remove`), `~/s3_base-target` (1.9 GB, removed mid-pass when the disk filled, after saving its bench binary) and `/tmp/s3` (560 MB: harnesses, six bench rounds, both foraging binaries, the stills). `/` free space went from 1.9 GB to 2.4 GB; it was 14 GB when the pass started and reached 209 MB, so the shared `~/target/stigmergy-loop` (49 GB, 47 GB of it `debug/`) and other users of this disk are what is filling it. **Left, listed, not deleted:** that shared target (the loop's build cache), branch `loop/iter-3` (merged), and the parked branches `wiggum/parked-pass-8-20261030-151006` and `wiggum/parked-pass-19-20261030-162219`. `git worktree list` shows only the clone, so nothing is left from earlier passes on this host.

## Iteration 4 — 2026-10-30 — a NaN or infinite deposit is refused; a finite one stops at f32::MAX / 16 (#9)

**Triage.** Open issues: **#9** (`bug`, filed 19:45:29) and **#10** (`bug`, filed 19:45:30). No `consumer` or enhancement issue. #9 is the item: the older by one second, first in iteration 3's backlog, and silent and permanent where #10 is a loud debug-build panic (a release build wraps and reads the cell as off-grid). **Skipped, with reason:** #10's remaining half (`deposit_disc` forms `center + offset` in `i32` and walks an unclamped `(2r+1)²` box) shares the function but is a different failure, and one improvement per pass; it heads the backlog. Parked work: `wiggum/parked-pass-8-*` holds the wall-gradient research note and bench draft that iteration 3 already adopted; stale, ignored, not merged or deleted. Corpus re-scan: `catalog_recent` (200) held only ETH CGL (digital humans, animation, AR games), psychology, medicine and graphics-book embeds, nothing on stigmergy, pheromones, reaction-diffusion, influence maps or crowds; `corpus.md` gained the stems this pass used and the MMAS entry (`Research: corpus`, `691bb34`).

**What changed and why it ranked first.** `StigGrid::deposit` and `RallyGrid::deposit` wrote `amount × falloff` into the disc unchecked, and the tick cannot remove a non-finite value, so one NaN, one `∞`, or `f32::MAX` twice poisoned a channel for good, while `hotspot_cell` and `saturation_stats` reported an empty field. The runner-up, #10, fires on one extreme coordinate; #9 fires on one bad float and corrupts the whole channel.

- A non-finite `amount`, a rally vector with a non-finite component, and a rally deposit while `RallyDef::accumulate` is not finite write nothing (as an undefined channel already does).
- A finite deposit stops at `±f32::MAX / 16` (`grid::VALUE_LIMIT`, `pub(crate)`, quoted in the docs and README) instead of overflowing to `∞`. Chosen from a probe: a uniform field of `f32::MAX / 4` ticks finite for 2 000 ticks, `/ 2` overflows at tick 1; the limit leaves a factor of four.
- `deposited()` is `max(-L).min(L)` and not `clamp`, and the rally finiteness test is `#[inline(never)]`: both from measurement (below). Deposits inside the limit are bit-for-bit what they were.
- Three permanent tests in `src/lib.rs` (the issue's test with an added "a refused deposit leaves the grid bit-identical"; the limit, both signs, rally, 200 ticks at the limit; a non-finite `accumulate`). The first two failed on `ff92756` (34 passed, 2 failed) and pass here (37 pass). `examples/bench.rs` gains `deposit_x1000` and `rally_deposit_x1000`.
- **Assumed decision (owner may override), to be posted on #9 with label `assumed-decision`:** a non-finite deposit is *refused*, where pbrt clamps an infinite ray length to `MaxFloat`, because a stored `f32::MAX` overflows in the tick's neighbour sum; the finite-overflow limit `f32::MAX / 16` is a design default (derivation in the note); the rally refuses a non-finite gain as the price of the cheaper `max`/`min`.

**Sources.** Model: `10.1016_j.robot.2019.103251` (Tang et al. 2019: deposits accumulate by addition and the intermediate-vector divides by the step, so a stationary robot deposits `0/0`), `10.1109_jas.2021.1004129` (ACO review: pheromone is a sum of released quantities). Shipping game: `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` (Lewis: breadcrumb maps decay and add "on top", never reset, where a wiped-and-rebuilt map heals). Technical: `book_real_time_collision_detection_ericson` (NaN poisons arithmetic; test in the positive form), `PBR3_03_Shapes` and `PBR3_11_Volume_Scattering` (pbrt: NaN asserts, `∞ × 0` clamped to the largest finite float). Note: `docs/research/non-finite-deposit.md`. **Pending index:** Stützle & Hoos 2000, *MAX-MIN Ant System* (`10.1016/s0167-739x(00)00043-1`), the model-side precedent for a bound on stored pheromone: found by `paper_search`, not in the corpus. `hs paper download "<doi>"` treated the DOI as a search and downloaded seven other papers of the result (not MMAS) into the corpus, an unintended side effect of how that command takes its argument.

**home-still.** `system_status`: distill healthy (`bge-m3`, 18 070 embedded, 488 794 chunks); scribe `192.168.1.110` healthy (1 converting, 11 of 12 slots free), `bmb.local` healthy (4 converting, 2 of 6 free); 854 queued. One `hs distill search` timed out at 60 s and the next answered; the MCP `distill_search` worked throughout.

**Field checklist** (`/tmp/s4/{before,after}/findings-raw.txt`; before = `ff92756`, after = this branch; scratch harnesses over the public API):

| Check | Before | After |
|---|---|---|
| conservation, evap 0, 1000 ticks, walled map; field hash | rel −9.445e-9; `add48ad2fa2f6e6e` | same; same |
| rock max; decay vs `(0.9)^20` | 0; rel −5.121e-7 | same |
| ramp `2x + y`, interior gradient | `(4, 2)` | same |
| soak, 10 000 ticks × 16 finite deposits (72 814 off-grid or undefined); field hash | 0 non-finite, 0 negative; `3b95a82bf36b7c43` | same; same |
| the soak with 1 deposit in 16 hostile (`NaN`, `±∞`, `f32::MAX`, `MAX/2`) | **5 496 of 6 144** non-finite, first at tick 0 | **0**; 0 negative |
| one NaN deposit on open 64×64: NaN cells after 1 / 10 / 50 / 100 / 200 ticks | 21 / 303 / 2 479 / 4 086 / 4 096 | 0 / 0 / 0 / 0 / 0 |
| after 200 ticks: `hotspot_cell`, `saturation_stats`, `sample_cell`, `gradient_cell` | `(None, 0)`, `(0, 0)`, NaN, `(NaN, NaN)` | `((10, 10), 2.17e-3)`, `(2.17e-3, 0.027)`, 1.4e-18, finite |
| `+∞` at radius 2.0; `f32::MAX` twice then 2 ticks; rally NaN then 2 `evaporate` | 4 NaN + 9 ∞; cell ∞, 13 non-finite; 13 NaN | 0; cell 2.1267647e37, 0; 0 |
| uniform `f32::MAX / k` field, 2 000 ticks: first non-finite tick, k = 16, 8, 4, 2, 1 | none, none, none, 1, 1 | identical (tick untouched) |
| ±`L` checkerboard, 3 000 ticks, diffuse 1 / 0.25 / 1 with evaporation 0.1 | overflows at the deposit | finite throughout |
| bench `determinism`: caller, Bevy 6, rayon 1, 2, 14; 3 rounds × 2 sides | `a446265d6a95cf51` | `a446265d6a95cf51`; 76 shared lines, one hash each, equal |

**Bench.** The bench binary cannot gate 5% on this host: it is a macOS arm64 machine at load 3–10, and the same tick code reads 0.52–2.47× between runs (64² one channel inline: 14.8 → 25.8 µs at one thread, 10.2 → 19.1 µs at the default pool: the P/E-core split iteration 3 logged; the tick is untouched). `busy_pool` medians 510 → 631 µs (one thread) and 640 → 649 µs (default), noise of the same kind; `parked_ticks` is Linux-only (`n/a`). The gate is the **in-process interleaved A/B** (old crate renamed beside the new one, alternating order each batch, 1 000 batches of 1 000 deposits, `/tmp/s4/after/ab-final-r1..3.txt`; every field hash equal after every run). Median new / old, three runs:

| Case | 64² | 256² | 1 024² |
|---|---|---|---|
| `StigGrid::deposit` × 1 000 | 1.026 / 1.033 / 1.034 | 0.986 / 0.981 / 0.952 | 1.015 / 1.021 / 1.018 |
| `RallyGrid::deposit` × 1 000 | 0.996 / 0.995 / 0.989 | 0.997 / 0.968 / 0.964 | 0.999 / 0.998 / 1.015 |

Forms tried on the way (64², 1 500 batches, three runs each): `clamp` 1.045–1.048 scalar and 1.03–1.035 rally (1.06–1.09 in an earlier set); `abs`/`copysign` 1.043–1.052 and 1.035–1.041; `max`/`min` 1.021–1.024 and 1.004–1.014; `max`/`min` with the finiteness test inlined into the rally deposit **1.06–1.077** (over the 5% gate, from the disc walk's code layout and not from the test); the same test out of line 0.993–1.002. No regression over 5% on any shipped case; the worst is 1.034.

**Critique and verdict** (`/tmp/s4/critique.md`): **GREAT** for the deposit paths #9 names. Flaws kept: the limit is an invariant of deposits and not of the field (`set_channel` takes finite values up to `f32::MAX`, which overflow at the first tick); a NaN def (`evaporate`/`diffuse`) still makes a NaN field, and a deposit into such a cell now writes `−L` instead of NaN (probe: 16 NaN cells after one tick, then `−2.1267647e37`); a rally with a non-finite `accumulate` is a silent no-op, the API having no error channel; the closure of the tick at the limit is argued and probed, not proven (a `diffuse` above 1 is outside it); `#[inline(never)]` is layout engineering measured on aarch64 only; the scalar deposit costs a steady +2.6–3.4% at 64²; #10 is untouched. No capture: the change is bit-identical for in-range deposits, so `docs/foraging.gif` and its caption still describe the file.

**Gate:** build, build all-features, release, test, test all-features (37 unit, 4 caller-thread, 2 leaf, 1 doctest), examples, clippy `-D warnings`, rustdoc `-D warnings`, fmt, wasm32 — all green on `fc45a34` (clippy red once on `manual_clamp` for the deliberate `max`/`min`, fixed with a justified `#[allow]` in `fafa60d`'s tree before the commit). The linker's `__eh_frame section too large` warning on the `foraging` example is pre-existing and not an error.

**Commits:** `64e2d08` research note, `691bb34` corpus, `fafa60d` implementation + tests + bench cases (`Fixes #9`), `fc45a34` docs (README gotcha, CHANGELOG, measured numbers in the note), then this log.

**Release: not done.** More than six hours have passed since `v0.2.1` (14:11, now 21:20) and `src/**` and `README.md` changed, so one is due. This host cannot publish: no `~/.cargo/credentials*`, no `CARGO_REGISTRY_TOKEN`, and `cargo-semver-checks` is not installed. The change adds no public item and removes none, so it is a patch (0.2.2) carrying #7, #8 and #9; all three stay under `## [Unreleased]`, and no tag or version bump is made for a crate that is not on crates.io, so the repository does not claim a release that does not exist.

### Ranked backlog

1. **#10 (bug, deposit half)**: `deposit_disc` forms `center + offset` in `i32` and walks an unclamped `(2r+1)²` box: overflow on an extreme cell in a dev build (a saturated `CellFrame::world_to_cell`), a hang on a huge `deposit_radius`. Clamp the box to the grid in `i64` before the loop; the cost then scales with grid ∩ disc.
2. **Bug, not filed:** a def with `diffuse` outside `[0, 1]` or NaN, or a NaN `evaporate`, `decay` or `accumulate`, is used as given (a NaN def makes a NaN field in one tick, and a deposit into it writes `−L`). Decide clamp vs. refuse at `StigGrid::new` / `RallyGrid::new`.
3. **Bug, not filed:** `set_channel` takes finite values up to `f32::MAX`, which overflow at the first tick; clamp a restore to `±VALUE_LIMIT` so the limit holds for the whole field.
4. One `run` call per tick for every diffusing channel (one scope instead of N); then a cost that scales with live cells, not the whole grid, for large mostly-empty maps. And a precomputed falloff table per channel for the deposit walk (it spends a `sqrt` and a divide per cell, about 240 ns per 13-cell deposit): bit-identical because the same operations run once at construction.
5. Bench hygiene: a P-core pin or a long single-core warm-up so macOS runs are comparable (and `parked_ticks` on a Linux host); a regenerated `docs/foraging.gif` from the capture knob when the behaviour next moves.

**Merged and cleaned up (iteration 4).** Merged as `094075a` (`Fixes #9`; `main` had carried the research commit `64e2d08` since before the branch was cut, so the merge is a `--no-ff` of a descendant). The gate was re-run green on the merged tree (all six steps, exit 0) before `main` was pushed, and #9 closed as completed when the push landed; the measurements and the assumed decision are in a comment on it, with label `assumed-decision`. `cargo publish --dry-run` on the merged tree packages 12 files (257.9 KiB, 70.4 KiB compressed), verifies and stops at the upload. Removed: `~/s4_base` (worktree, 1.9 MB, plain `git worktree remove`; no `~/s4_base-target` was ever built, the baseline was a renamed copy of its `src/` inside the A/B harness) and `/tmp/s4` (490 MB: harnesses, bench binaries and rounds, findings, critique). `/` free space went from 9.3 GB to 9.7 GB (it was 14 GB at the start: the other loops on this disk and the 38 GB `~/target/stigmergy-loop` cache are what fill it). **Left, listed, not deleted:** that shared target (the loop's build cache), branch `loop/iter-4` (merged), the parked branch `wiggum/parked-pass-8-20261030-151006` (stale, adopted in iteration 3), and the seven papers `hs paper download` fetched into the home-still corpus by mistake (not in this repository). `git worktree list` shows only the clone, and no `~/s*_base*` exists, so nothing is left from earlier passes on this host.
