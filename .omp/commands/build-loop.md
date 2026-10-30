---
description: One iteration of the bevy_stigmergy improvement loop
argument-hint: "[optional focus, e.g. 'issue #6 only']"
---

One run of this command is one iteration of a long-running loop on bevy_stigmergy, a Bevy 0.20 leaf crate of stigmergic influence fields: `N` scalar channels over a fixed cell grid that agents deposit into, which evaporate, diffuse between floor cells and can be sampled or climbed, plus a vectorial rally pheromone. Games use it for trails nobody planned, crowds that route around each other and social fields (Sanctuary runs one every tick; bevy_emerge's crowds-and-paths family issue hosts it beside `bevy_orca`). Do ONE improvement per iteration, the single most valuable one from a game developer's point of view, finish it completely, then stop.

This crate must be correct AND deterministic AND cheap on the caller's frame. A field that drifts, a result that changes with the thread count, and a tick that stalls the thread that called it are all failures, however good the behaviour looks. The judgement here is a simulation engineer's: conservation where the model says so, a stencil that is the same on one thread or twenty-four, a cost that scales with the cells that matter, and an API that leaves the schedule, the world mapping and the walls to the caller (`CLAUDE.md`, "Where the boundary falls").

You do all the work yourself, including looking at any images and judging them. Do not delegate
visual review.

Extra instructions for this run (a focus such as "issue #6 only"): $ARGUMENTS
They override the choice of item in section 4, not the gates.

State lives in `docs/loop-log.md`; rules and traps live in `CLAUDE.md`. Follow them; do not
re-derive them.

## Your work is saved even if you are killed — read this before you start

**The driver saves whatever you leave uncommitted, without touching your branch.** Every ten minutes
it records the whole tree to a snapshot ref (`refs/wiggum/pass-N-...`), and again when the pass ends.
Your `git status`, your index and your branch are unchanged — so this is not a substitute for your own
commit step, which is still the only thing that puts work on `main`.
Recover a killed pass's tree with `git -C <repo> log --all --oneline --grep='WIP(pass'` then
`git show <ref>`.

1. **Commit as you go, not once at the end.** Your commit step is last, behind implement, gate and
   benchmark, so a pass killed at the ceiling never reaches it. Commit the research note alone before
   writing code, as your sections already say; then commit the implementation, then the docs and the
   log. Each commit that lands is work a killed pass cannot lose.
2. **A snapshot ref is not a commit on your branch.** `git log` will not show it. Do not look for
   one; if an earlier pass was killed, its tree is in `refs/wiggum/` and nobody's gate ran on it.
3. **Parked work is a branch, and you look for it before you pick an item.** When a pass is stopped or
   killed with its tree uncommitted, the driver's next start commits that tree to a branch
   (`wiggum/parked-pass-N-<time>`, or on the pass's own `loop/iter-N`) in a commit titled
   `WIP(pass N, parked)` and puts the clone back on `main`: you start clean and nobody tells you. So run
   `git branch --list 'wiggum/parked-*'` and `git log --all --oneline -n 5 --grep='parked'` once, after
   you have triaged the filed issues and before you commit to an item. If the newest parked tree is for
   the item you would pick, or for an issue or bug that outranks your pick, **adopt it**: it is a first
   draft that was never built, tested or gated and was cut off mid-work, so read it, bring its files into
   your own `loop/iter-N` branch (`git checkout <parked-branch> -- <paths>` or `git cherry-pick`),
   finish it, and run the real gate on the result before it ships under your name. Never merge the
   parked branch itself and never delete it. If it is stale (its files conflict with `main`, or the item
   was fixed meanwhile), say so in one line in the log and ignore it. A parked branch that exists only on
   this clone holds a lot of data: the driver keeps anything over 10 MB of new data off origin.


## 1. Orient

1. `CLAUDE.md`: rules, the boundary, the leaf ratchet, determinism.
2. `docs/loop-log.md` (create it if missing): what earlier iterations did, found and deferred, and the
   newest iteration's ranked backlog. The next iteration number is the last one plus one.
3. `git log` since the last logged iteration.
4. The `docs/research/<system>.md` note of any system you may touch, and `docs/research/corpus.md` for
   the reading list per domain (create both the first time a pass researches).
5. `git status --short`. The driver (`wiggum.sh`) starts a pass only on `main` with no
   tracked changes. If you find tracked changes you did not make, stop and log them; do not commit or
   revert them.
6. **Re-scan the corpus for new sources.** Run `catalog_recent` (limit 200) and add any stigmergy,
   pheromone, ant-colony, reaction-diffusion, influence-map, crowd-simulation or parallel-stencil
   document that `docs/research/corpus.md` does not list, as its own commit (`Research: corpus`).

Preflight home-still now, so research is not blocked later:

- `system_status` (MCP) or `hs status`. HomeStill's servers run on other machines; never restart them
  from here. If distill is down, skip `distill_search` and `distill_index`, search with the CLI
  (`hs distill search "<query>" --output json`) or `paper_search`, and list each unindexed note under
  `Pending index`. Record the live state in the log; do not copy an earlier iteration's outage note.
- Scribe: check `scribe_health`. `scribe_convert` may queue or fail; then rely on the corpus.
- **An outage is not a reason to idle.** `paper_search`, the CLI and the papers already quoted in this
  repository need no GPU; research from them and build.

## 2. Capture the current state

Build first (`cargo build --release --examples --all-features`). Keep everything under `/tmp/sN/` (`N` the
iteration), the baseline under `/tmp/sN/before/`. Keep a worktree of the starting commit for later
comparisons, **detached, at a commit, and off `/tmp`**: `git worktree add --detach ~/sN_base <sha>`, built
with `CARGO_TARGET_DIR=~/sN_base-target`.

Section 9 says how to name, build and remove that baseline. A baseline left behind costs the next pass
a build cache's worth of disk, and one with `main` checked out stops anything else checking `main` out.

**The numbers are the state.** This crate's cost lands on its caller's frame, so measure it the way a
caller pays it:

- **Throughput:** `evaporate_diffuse` (and the rally update) per tick at grids of 64x64, 256x256 and
  1024x1024 with about 70% floor cells, and 1, 4 and 8 channels: median and p99 over at least 200 ticks,
  after 20 warm-up ticks. Run it with `RAYON_NUM_THREADS=1` and with the default pool.
- **The caller's wall clock:** the same tick called from inside another thread pool's worker (a Bevy
  `ComputeTaskPool` task, or a plain `std::thread` pool standing in for one) while that pool has other
  work queued, so a call that parks its thread shows up as the stall it is (issue #6 measured hitches up
  to 150 ms in Sanctuary).
- **Determinism:** a fixed scenario's fields hashed bit-for-bit after 500 ticks, at 1, 2 and the default
  number of threads. The hashes must be equal.

The release example that does this is `examples/bench.rs`, printing one line per case as
`stig: <case> grid=<w>x<h> ch=<n> threads=<t> median_us=<x> p99_us=<y> hash=<h>`. Add it in the pass that
first needs it, and extend it, never fork it. Record every line in `/tmp/sN/before/bench.txt`.

The loop's host shares its cores with other loops, so a single run is noise: run the bench three times
before and three times after, back to back on the same machine, and compare medians of medians.

The visible behaviour is the `foraging`, `trail` and `rally` examples. When a change can be seen there,
capture a still or a short GIF under the loop's display (the unit sets `DISPLAY=:91`), adding a
`STIG_FRAMES` / `STIG_SHOT` capture knob to the example in the pass that first needs it.

## 3. Inspect closely (you judge the results yourself)

Read every number and every image yourself. Write the **field checklist** to `/tmp/sN/findings.md`,
each line with a measurement, not an impression:

- **conservation:** with evaporation 0 and no deposits, the field's total over floor cells is unchanged
  by diffusion to within float rounding (quote the relative error);
- **walls:** no value ever enters a wall cell; a channel diffused around an obstacle routes around it;
- **decay:** after `k` ticks a deposit has the fraction of its value the evaporation rate says
  (`(1 - rate)^k`), quoted against the measured value;
- **no NaN, no negatives, no infinities** after 10 000 ticks of random deposits, including deposits at
  out-of-range cells, which must be refused, not panicked on;
- **determinism:** the bench's hashes equal at every thread count;
- **gradient:** `climb`/gradient queries point uphill on a known ramp;
- **cost:** the bench's medians and p99s, and the caller's wall clock from inside a busy pool;
- **the examples:** a still of the foraging trail when behaviour changed, read at full frame and zoomed.

## Owner input: recommend, record, proceed

**This loop runs fully automated (owner, 2026-10-29): never stop to wait for the owner.** When a pass needs the owner's input (a design choice the issue or item does not settle, such as a colour, a coverage, an API shape, a schema change or which of two behaviours; or a protected file that must change), it answers the question itself:

1. **Recommend.** Name the two or three real options and pick one, with one line of why. Prefer the conservative choice: reversible, conventional, consistent with the existing code and the issue's stated intent, and the smallest that meets the acceptance.
2. **Assume the owner chose it, and build it this pass.** A decision is never a reason to skip an item or end a pass.
3. **Record it where the owner reviews.** On the issue, a comment beginning `Assumed decision (owner may override):` with the options, the pick and why, and the label `assumed-decision` (`gh label create assumed-decision --color FBCA04 2>/dev/null; gh issue edit <N> --add-label assumed-decision`). With no issue, in the commit body and the log line, beginning `Assumed decision:`.
4. **The owner's reply wins.** An owner comment that contradicts an assumed decision overrides it: the next pass that reads it reworks the item to the owner's choice before anything else.
5. **Earlier blocks are lifted.** An issue or backlog item logged as blocked on, or waiting for, an owner decision is workable now: decide it this way.
6. **Protected checks change only additively.** An assumed decision may add to a protected test or check (a new row or entry, a new field's empty default in a literal, a count or version raised by exactly what the change adds), with the commit message `assumed owner exception: <what>`. It never removes, loosens, skips or re-pins an existing assertion, golden, eval answer or threshold; that is the one block left, so say so on the issue and go on.
7. **Never, whatever you assume.** These need the owner's word in a conversation, and no prompt can give it: yanking a release or changing crate ownership; force-pushing, rewriting history or deleting branches; deleting files or work this pass did not create; editing this command file; spending money or sharing credentials.

## Filed issues come before new features

**A filed issue is a consumer telling you what broke, which outranks anything this pass invented.**
Before you pick an item, list the repository's open issues and triage them:

```sh
gh issue list --repo Ladvien/bevy_stigmergy --state open --limit 100 \
  --json number,title,labels,createdAt --jq '.[] | "#\(.number) \(.title)"'
```

(If `gh` is absent, the same list is
`https://api.github.com/repos/Ladvien/bevy_stigmergy/issues?state=open&per_page=100`.)

Then, in this order:

1. **A bug issue is the pass's item, ahead of any enhancement.** Anything reporting wrong behaviour —
   a crash, a panic, a visual artifact, a value that is implausible, a documented promise that is not
   kept — is a bug. Take the oldest such issue that is still reproducible, and say why it beats the
   others.
2. **An issue labelled `consumer` is the pass's item next, ahead of the series and your own candidates.**
   A project built on this crate needs it and is waiting. The `consumer:<name>` label says which project;
   `consumer:bevy_emerge` is the owner's editor. Its own work resumes when the issue is closed as completed
   and it moves its git pin to a `main` commit containing the fix. Take the oldest one you can work this pass.
   - **Build the general form.** The requester is the example, never a name in this crate's API, a path or a
     default: this crate serves other projects too. Where an issue asks for something shaped to one project,
     build the general form and say so on the issue.
   - **Prove it here**, with a test in this repository that fails before and passes after against the issue's
     acceptance. Never build or run the requesting project to prove it.
   - **Pushing to `main` is the delivery.** Requesters pin a git revision, so a consumer fix never needs a
     publish or a version bump.
   - **One that needs a decision the issue does not settle is decided, not skipped:** recommend, record and
     build it (see "Owner input: recommend, record, proceed"). Only a change that would weaken a protected
     check cannot be worked: say exactly what is needed on the issue (`gh issue comment`), write one log
     line, and go on. A blocked consumer issue never stops the pass.
   - **Read `git log` first.** While this loop is stopped, the requester's loop may fix the issue here at the
     source, in its own commits with tests; check whether that has happened so the work is not done twice.
3. **Every other open enhancement issue is the pass's item next, ahead of your own candidates** (the
   owner's rule, 2026-10-29): a filed request was asked for; a candidate this pass invents was not. The
   order of work is: **bugs first** (a bug issue, then a bug already recorded in the backlog or the log),
   then `consumer` issues, then every other open enhancement issue, oldest first. Only when each of them
   is done, blocked or a release issue does "Pick the single most valuable item" choose a feature of your
   own. A blocked issue gets a `gh issue comment` saying exactly what it needs and is skipped; it never
   stops the pass. Name the issues you skipped, and why, in the log line, so the next pass sees the list
   was worked through.
4. **Close the loop.** An issue your pass fixes is referenced in the commit body (`Fixes #N`) **and**
   in the report, so the next pass can see it was addressed. Do not close an issue you did not fix,
   and do not close one whose fix this pass did not verify. For a `consumer` issue the close is the signal
   the requester waits for: close it only through a verified fix pushed to `main`, never as not planned
   without the owner's word.
5. **A release issue is done by the next release.** An issue asking to publish or bump a version is closed by the pass whose release (the "Release" block in the ship section) publishes it, with the crates.io URL.

**When the API fallback answers `403` or `429`** (rate limit — 60 requests/hour per IP unauthenticated,
shared by everything on this network), do not treat it as "no issues": write one line saying the list
could not be read and continue. Never spend a second call to retry it in the same pass, and never infer
from silence that nothing is filed.

**An empty issue list costs nothing:** if it is empty or `gh` is unreachable, write one line saying so
and continue — never spend a pass on the lookup.

## 4. Pick the single most valuable item

**You are here only when no bug and no open issue can be worked this pass** (see "Filed issues come
before new features"); otherwise that issue is the item. Rank candidates from your findings, the loop log's backlog and the extra instructions, always asking
what matters most to a game developer who will ship this:

1. A wrong result: a field that gains or loses mass it should not, leaks into walls, goes NaN or
   negative, panics on caller data, or changes with the thread count.
2. A cost the caller cannot afford: a stall on the calling thread, a tick that scales with the whole grid
   when few cells are live, an allocation per tick.
3. API friction: setup that takes more lines than it should, a query a game needs and cannot make, a
   value the caller must recompute because the crate hides it.
4. Missing features a game expects from influence maps: per-channel diffusion rates and anisotropy,
   multi-resolution or sparse grids, cheap snapshots and restore, an event when a channel crosses a
   threshold.
5. The examples looking unconvincing next to what the field can show.

Ownership is a gate: the schedule, the world mapping and the walls are the caller's (`CLAUDE.md`), local
avoidance between agents is `bevy_orca`'s, and anything that needs `bevy_ecs` or `bevy_app` is a wrapper
the caller writes. Do not build those here; add a hook instead.

State in one sentence why your pick beats the runner-up. If it is too big, carve off the most valuable
self-contained slice. Record the ranked candidate list in the log, not only the winner.

**A pass never ends idle** (owner, 2026-10-29: "If they're not addressing bugs or enhancing features, they need to evolve their own code base to support Bevy Emerge better or add new features. Research these things in home still, implement them. They shouldn't stay idle."). An empty issue list or a stale backlog is this step's input, never a reason to ship nothing, and a candidate is not "speculation" once it is researched. When the ranked list above gives no workable item, take the first of these that applies:
1. **What the crate's consumers hand-roll.** Read how Sanctuary uses this crate (`gh repo clone Ladvien/sanctuary /tmp/sN/sanctuary -- --depth 1`, then `rg -l bevy_stigmergy`; removed afterwards) and bevy_emerge's crowds-and-paths issue (`gh issue view 107 --repo Ladvien/bevy_emerge`). Find a formula, a wrapper or a query either writes by hand around this crate, and build its general form here, never named for either. Prove it with a test here.
2. **A feature a shipping game in this domain expects and this crate lacks**, found by research: home-still first (`distill_search`; for a paper the corpus lacks, `paper_search`, `paper_download`, `scribe_convert`, then `distill_index`), then shipped-game practice, then section 5 before any code.
3. **Depth on the weakest existing feature**: measure it against its research note's validation targets and improve the worst gap.

Record the ranked candidates and the pick in the log. A pass that ships nothing names the concrete blocker that stopped all three; "nothing new to do" is not a blocker. **"Not researched yet" is not a blocker either: it is this pass's work.** Take the top item, do section 5 for it now (home-still preflight included), commit the note, and build its first slice. "Building it would be speculation" means the research is missing, so do the research. A log-only iteration is a failed pass, not an outcome, and a pass never copies an earlier iteration's reasons for shipping nothing (measured 2026-10-29: iterations 194 and 195 each shipped a log line saying every item was "not researched", and neither ran the research).

## 5. Research thoroughly (before any code)

Every feature gets researched, new or improved, with no exceptions. A field is a model of a physical
process put to work as game AI, so every iteration cites **three** kinds of source:

1. a **model** source: stigmergy, pheromone and ant-colony models, or reaction-diffusion and cellular
   automata (the Dourvas, Sirakoulis & Adamatzky 2019 *IEEE Access* paper `CLAUDE.md` cites is the
   determinism basis already in use);
2. a **shipping-game** source: influence maps and scent or trail systems in shipped games (Game AI Pro
   chapters, GDC talks, studio write-ups);
3. a **technical** source: parallel stencils, cache-friendly grid layout, work-stealing schedulers and
   nested thread pools, or SIMD.

When the home-still MCP bridge answers `401 Refresh token expired`, search with the CLI,
`hs distill search "<query>" --output json`, which refreshes its own token.

If the corpus lacks one of the three, `paper_search` for it, and record the gap under the note's
`## Pending index`. Write the note with the headings this loop requires (`## Sources`,
`## Craft principle`, `## How shipping games do it`, `## Model adopted`, `## Parameters`,
`## Validation targets`, `## Deviations`, `## Pending index`) plus `## What great looks like`: the
visual qualities and reference images you are aiming for. `## Craft principle` (the model source) and
`## How shipping games do it` must each **quote** the source they rest on, not paraphrase it from
memory.

Any changed default goes under `Deviations` with a citation, or is marked a design default. Commit the
note alone before writing code: `git add docs/research/<system>.md && git commit -m "Research: <system>"`.

## 6. Implement within the crate's rules

- Keep every rule in `CLAUDE.md`. In particular: `tests/leaf.rs` is the ratchet (allowed: `bevy_math`,
  `rayon`, optional `serde` and `bevy_reflect`); widening it is a design decision recorded as one.
- **Diffusion stays thread-count-independent**: a pure stencil over disjoint output slots, no shared
  accumulator across cells. A change that could alter a single bit of output proves it did not with
  the bench's hashes at 1, 2 and the default thread count.
- No `unwrap()`, no `expect` on caller data, no panicking index. Cells and deposits come from callers.
- The crate registers nothing and owns no schedule. A choice about *where* work runs (which pool, how
  many threads, inline or parallel) is offered to the caller as a value it passes, with a default that
  costs the caller's frame least.
- Plain functions with their own tests for any new maths, testable without an app.
- Re-export new public items from the crate root; `#![warn(missing_docs)]` holds.
- One path per feature, no fallbacks or shims; delete the code the change replaces.
- Tests: for a bug, a test that fails before and passes after. Permanent tests check what a caller sees
  (values, conservation, determinism, refusals); throwaway checks go in `/tmp`.

## 7. Verify and sign off: it must be RIGHT and FAST

You are grading your own work, so grade it as a hostile reviewer would — a simulation engineer who
will check the conservation and the thread-count hashes before reading anything you are proud of.

- Rebuild, re-run the bench three times and the field checklist under `/tmp/sN/after/`, and compare
  every line with the "before".
- Write the critique BEFORE the verdict (`/tmp/sN/critique.md`): every flaw you can find, against the
  research note's `## What great looks like` and `## Validation targets`. Look for what is wrong, not for
  confirmation that your change worked.
- Then give each feature exactly one verdict: GREAT, NOT YET or FAIL. GREAT means a game could ship on it
  today. "Fine", "acceptable" and "good" are NOT YET.
- On NOT YET or FAIL, write the specific changes that would move it to GREAT, apply them, rebuild,
  re-measure and re-judge. Do not lower the bar to pass.
- After 5 failed rounds on one feature, stop: log the blocker, the remaining flaws and your best
  hypothesis, and revert to the last GREAT state.
- **Performance gate**, equal in weight: median of the three bench medians before and after, same
  machine; no regression over 5% on any case without a logged, justified trade. A change aimed at a stall
  shows the caller's wall clock from inside a busy pool before and after.
- Run the full gate:
  1. `cargo build && cargo build --all-features && cargo build --release && cargo test && cargo test --all-features`
  2. `cargo build --examples --all-features`
  3. `cargo clippy --all-targets --all-features -- -D warnings`
  4. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
  5. `cargo fmt --check`
  6. `cargo build --lib --target wasm32-unknown-unknown`

## 8. Ship

Start only after section 7 passes. Then release (the "Release" block at the end of this section).

1. Document: rustdoc for every changed public item, the README for user-visible changes, the research
   note's `Deviations` and `Pending index`, and an entry under `## [Unreleased]` in `CHANGELOG.md`
   (`Added`, `Changed`, `Fixed`, `Removed`).
2. Regenerate `docs/foraging.gif` when the behaviour it shows has moved, from the example's capture knob.
3. Log: append `## Iteration N — <date> — <title>` to `docs/loop-log.md`:
   - what changed and why it ranked first (and the runner-up)
   - the sources used: the craft, shipping-game and technical stems, by stem
   - your critique and verdict per capture, with evidence filenames
   - the field checklist's numbers, before and after
   - the bench's medians and p99s, before and after
   - the commit hash, and any sibling-repo commits
   - the top 5 remaining items, ranked, for the next iteration
4. Branch and commit: `git checkout -b loop/iter-N` from `main`. Commit by path with messages that
   state the user-visible change: the research note (already committed in section 5), the
   implementation, the docs and changelog, then the log (`Loop log: iteration N, <title>`).
   `git status --short` must be empty afterwards.
5. Push and merge: `git push -u origin loop/iter-N`, then `git checkout main && git pull --ff-only &&
   git merge --no-ff loop/iter-N -m "Merge loop/iter-N: <title>"`. On a conflict, `git merge --abort`,
   log it, and stop.
6. Re-run the gate on the merged tree; a failure stops the pass before the push. Then
   `git push origin main`.

**Release, every pass that changed the package** (owner, 2026-10-29: loops publish, and never wait on CI to do it). After the push, from that pushed commit, on a clean tree with the gate green: if a file `cargo package --list` ships changed since the last published version (`git diff --quiet v<last> HEAD -- <those files>` fails; with no tag `v<last>`, diff from the commit that set that version in `Cargo.toml`, found with `git log -S'version = "<last>"' -- Cargo.toml`), release:
1. Decide the version with `cargo semver-checks check-release` (`cargo install cargo-semver-checks --locked` if missing). Pre-1.0: a breaking change bumps MINOR and resets PATCH, anything else bumps PATCH; never 1.0.0. Move `## [Unreleased]` in `CHANGELOG.md` to `## [<version>] - <date>`, bump `Cargo.toml` and `Cargo.lock`, and commit `release: v<version>`.
2. `cargo publish --dry-run`, then `cargo publish`, locally, with the credentials `cargo login` stored; never print them. Never wait for, watch or re-run a GitHub workflow to release.
3. `git tag -a v<version> -m v<version>`, then push the commit and the tag (merge, never force, on a refused push).
4. `cargo info <crate>@<version>` resolves; put the crates.io URL in the log entry.

**At most one release every 6 hours** (owner, 2026-10-30: bevy_lights published v0.2.1 through v0.2.20 in one day, and then crates.io answered 429 for three passes in a row, each of which did nothing but retry). Release only when the last tag is more than 6 hours old (`git log -1 --format=%ct <last tag>`); until then the changes ride in the next release. A publish refused with 429 is not retried in this pass or the next: log it once and build, and the next release carries it. A pass that only retries a release is an idle pass. Any other failed publish is logged with its error and retried at the next release. Never yank, never reuse a version; fix a bad release forward.

### README, demos and docs stay true

A README is read before anything else, so a false line there costs more than a false line anywhere else.
An audit on 2026-10-27 found one in each of the four loop repos: a `tour.gif` captioned "at noon" over a HUD
reading 10.00 h with a running clock; a `ties.gif` captioned for rivalry colours that appear in none of its 240
frames; a hero caption promising an effect its file never shows; and a demo video captioned as the full
recording of the GIF above it when it shows different objects. Before you push a change a user can see:

1. **Put it in the section it belongs to.** Do not append to the introduction or to the end, and do not
   restructure the README in a feature pass. It reads in this order — what it is in one line, the demo, what
   you get, install, quick start, then the detail sections — and an item added in the wrong place undoes it.
2. **A caption describes the file as it exists.** Open the GIF or still (frame count, size, any HUD burned into
   it) and write only what you can read off it; a figure quoted from program output comes from running that
   program now, not from the log. Recipe: the example's capture knob (section 2).
3. **Media you cannot regenerate here is logged, not left lying.** With no GPU or display on this machine, name
   the stale file and its path in the report and change its caption to what it still shows.
4. **The README's code compiles.** Edit a README snippet only together with the code it shows, and check it against the crate-root doctest it mirrors.

## 9. Clean up when you are done

Worktrees and build directories outside the clone are not free: a `target/` for this crate is tens of GB, a
loop host's `/tmp` may be RAM, and on 2026-10-27 earlier passes had left 37 GB of baseline builds and 5 GB of scratch behind.
**After you ship and before you write the pass's last log line, whether or not you shipped,** remove what *you*
made this pass, and list what earlier passes left.

1. **A baseline is a pair that can be found.** Make it with `git worktree add --detach <path> <sha>` (never a
   branch name: a worktree with a branch checked out stops anything else checking that branch out) and build it
   into `<path>-target`. Then `git worktree list`, plus the `*-target` directory beside each worktree, is the
   whole inventory.
2. **Remove yours.** `git worktree remove <path>` (plain, never `--force`) and `rm -rf <path>-target`, then your
   scratch under `/tmp` (captures, frames, venvs, gate logs, `rustdoctest*`). Remove only what you created this
   pass, and name each path in the log. If git refuses because the worktree has a change in it, name the file
   in the log and leave it.
3. **Leftovers from earlier passes are listed, never deleted.** A killed pass cannot clean up after itself, but
   an agent does not delete what it did not create, so list them once and let a person approve the removal:

   ```sh
   git worktree list --porcelain | awk '/^worktree /{sub(/^worktree /,""); print}' | tail -n +2   # all but the clone
   du -sh <wt> <wt>-target                       # the size of each one that is not yours
   git -C <wt> status --short                    # empty: nothing is lost by removing it
   find <wt> <wt>-target -mmin -120 -print -quit # empty: untouched for two hours, so no live build is using it
   ```

   Log each with its path, size, and whether it is clean, already on `origin/main` and idle (the likeliest to be
   stale). Say so, and still leave it.
4. **Never remove** the clone itself, its own `target/` (the build cache the next pass starts from), anything you
   did not create this pass, or anything a process is using.
5. Write one log line: what you removed and the space it freed (`df -h .` before and after), and the leftovers
   you listed.

## Rules

- One improvement per iteration, fully finished: researched, implemented, GREAT-signed-off,
  benchmarked, gated, committed, documented, logged.
- No feature ships without a written critique, a GREAT verdict and a clean performance check.
- If blocked, log the blocker and pick the next item. A failed gate or a visible defect is not a
  blocker: fix it. A failed push or merge is: log the exact error and stop. Never force-push.
