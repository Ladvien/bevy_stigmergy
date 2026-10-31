# A deposit that is not a number (issue #9)

What `StigGrid::deposit` and `RallyGrid::deposit` do with an `amount` that is NaN, `±∞`, or finite but
large enough to overflow the cell it lands in; and why a field that accepts them cannot be repaired by
the tick. `conservative-diffusion.md` covers what the tick computes; this note covers what is allowed
into the field before it does.

## The defect, measured

Both deposits write `amount × falloff` into every cell of the disc with no check. Measured on `ff92756`
(v0.2.1) by a scratch harness over the crate's public API (`/tmp/s4/before/findings-raw.txt`); the
walled map is 64×48 with a wall at `x = 32` (one gap) and about 9% scattered rock, 2 749 floor cells:

| Scenario | Result |
|---|---|
| one NaN deposit, radius 1.5, on an open 64×64 field with diffusion 0.25 | NaN at **21** of 4 096 cells after 1 tick, **303** after 10, **2 479** after 50, **4 086** after 100, **4 096** after 200 |
| the same field after 200 ticks, read back | `sample_cell` = NaN, `gradient_cell` = `(NaN, NaN)`; **`hotspot_cell` = `(None, 0.0)` and `saturation_stats` = `(0, 0)`**: the health readers report an empty field |
| `deposit(+∞)`, integer radius 2.0, 16×16 | **4 NaN and 9 ∞** cells before any tick (the rim has `falloff == 0`, and `∞ × 0` is NaN) |
| `deposit(f32::MAX)` twice at one cell, radius 0 | the cell reads **∞**; two ticks later **13** non-finite cells |
| `RallyGrid::deposit(NaN, 1)`, radius 2 | **13** cells hold NaN, and two `evaporate` calls change none of them (`NaN × retain` is NaN even at `retain == 0`) |
| soak, 10 000 ticks × 16 deposits (finite amounts; 72 814 land off-grid or on an undefined channel) | 0 non-finite, 0 negative cells; field hash `3b95a82bf36b7c43` |
| the same soak with 1 deposit in 16 hostile (`NaN`, `±∞`, `f32::MAX`, `f32::MAX / 2`) | **5 496 of 6 144** cells non-finite, first at tick **0** |

Nothing in the tick can clean this up. The skip test `grid[i] * retain != 0.0` is true for NaN, so a
poisoned channel always diffuses; every floor neighbour adds `src[j] * retain` into the sum, so one NaN
neighbour makes the cell NaN. The readers that would notice it are the ones that cannot:
`hotspot_cell` compares with `v > best` and `saturation_stats` folds with `f32::max`, and both skip a NaN.
`set_channel` already says why it refuses a non-finite saved value ("a NaN in a field poisons every
downstream sum"); `deposit` is the other door into the same field and does not.

## Sources

| Kind | Stem | What it gives |
|---|---|---|
| Model | `10.1016_j.robot.2019.103251` — Tang, Xu, Yu, Zhang & Zhang, *Dynamic target searching and tracking with swarm robots based on stigmergy*, Robotics & Autonomous Systems 2019 | The deposit `RallyGrid` implements. Its intermediate-vector divides by the step length, so a robot that has not moved deposits `0/0`: a NaN deposit is a thing the model itself produces. Deposits are accumulated into the map by plain addition. |
| Model | `10.1109_jas.2021.1004129` — Tang, Liu & Pan, *A Review on Representative Swarm Intelligence Algorithms for Solving Optimization Problems*, IEEE/CAA J. Autom. Sinica 2021 | The ant-colony update `τ = (1 − ρ)τ + Σ Δτ^k`: pheromone is a sum of released quantities, each a real amount an ant carried. |
| Shipping game | `GameAIPro2_Chapter29_Escaping_the_Grid_Infinite-Resolution_Influence_Mapping` — Lewis, Game AI Pro 2 ch. 29 | A breadcrumb map is never reset: old values decay and new ones go "on top". A map that is wiped each update heals itself; one that accumulates cannot. |
| Technical | `book_real_time_collision_detection_ericson` — Ericson, *Real-Time Collision Detection*, the chapter's IEEE-754 section | NaN poisons arithmetic; `INF - INF` and `0 · INF` are NaN; a range test written the obvious way lets a NaN through; the safe form states what is accepted. |
| Technical | `PBR3_03_Shapes`, `PBR3_11_Volume_Scattering` — Pharr, Jakob & Humphreys, *Physically Based Rendering*, 3rd ed. | A shipping renderer's practice: NaNs are asserted against, and where `∞ × 0` threatens, the operand is clamped to the largest finite float. |

## Craft principle

Tang et al., on how the vectorial pheromone is built, and the formula whose denominator is the robot's
step:

> "Therefore, when robots pass the same position from multiple directions and generate corresponding
> intermediate-vectors, these intermediate-vectors can be accumulated to get a new vector, as the vector
> pher in Fig. 1."

> `grad(B) = lim_{G→B} (f_B − f_G)/|GB| · (p_B − p_G)/|GB|`

The map is a sum of deposits, and a deposit is derived from a measured signal and a measured step. A
stationary robot has `|GB| = 0`, so the formula is `0/0` for it, and a sensor can return NaN: the model's
own recurrence can hand the store a NaN. The ant-colony form describes the scalar case, in the review's
words:

> "`Δτ^k_ij` is the pheromone released by the k-th ant passing the edge (i, j)." … "`τ_ij = τ_ij + Σ_{k=1}^m Δτ^k_ij`." … "`τ_ij = (1 − ρ) τ_ij`"

A released quantity of pheromone is a finite non-negative amount of a physical substance. "No amount" is
a legitimate deposit (nothing is released); "a NaN amount" is not a quantity at all, and "infinite
pheromone" is not one a cell can hold.

## How shipping games do it

Lewis (Game AI Pro 2 ch. 29) on the map shape this crate is, and the one it is not:

> "When the map is updated, rather than resetting the values and recalculating them based on the relevant
> placement and diffusion rules, the old values are decayed by a given proportion. The new influence
> values are then added 'on top of' the old values."

> "Some applications need not perform any propagation at all, such as tactical maps that store the
> instantaneous locations of various agents. Such maps can be queried directly for as long as their
> information is deemed up to date, and when the map becomes stale, it can simply be wiped and recomputed
> from scratch."

A wiped-and-rebuilt map forgives a bad frame: the next rebuild overwrites it. A breadcrumb map is the
other case, and the whole point of this crate (Sanctuary runs one every tick): nothing overwrites a cell,
so what is written once is read for as long as the decay lets it live, and for NaN that is forever. The
chapter says nothing about validating a placement, because on a map that is rebuilt it never mattered; a
map that accumulates has to do it at the placement, which is the only moment the bad value is still
separate from the good ones.

The renderer practice is the same lesson at a larger scale (Pharr et al., ch. 3 and 11):

> "NaNs propagate through computations: any arithmetic operation where one of the operands is a NaN
> itself always returns NaN. Thus, if a NaN emerges from a long chain of computations, we know that
> something went awry somewhere along the way. In debug builds, pbrt has many `Assert()` statements that
> check for NaN values, as we almost never expect them to come up in the regular course of events."

> "The problem is that multiplying Infinity by zero results in the floating-point 'not a number' (NaN)
> value, which propagates throughout all computations that use it. … The implementation here resolves
> this issue by clamping the ray segment length to the largest representable non-infinite floating-point
> value."

Two practices: assert that NaN does not occur, and where an infinity is *meant* (a ray with no far end),
clamp it to the largest finite value. Neither fits a deposit directly. A release build cannot assert a
host's data away, and a deposit of `+∞` is never meant: it has no cell to be written to. The clamp does
fit the one finite case, a sum that overflows.

## Model adopted

**A deposit is a finite quantity or it is nothing.** `StigGrid::deposit` with an `amount` that is NaN or
`±∞`, and `RallyGrid::deposit` with a vector that has a NaN or infinite component, write nothing, the way
an undefined channel already does and the way `set_channel` already treats a non-finite saved value. The
test is the positive form, `amount.is_finite()`, because Ericson's point is that the negative form
(`!(x > lo)`, `x != x`) is the one a NaN slips through:

> "NaNs can therefore slip through range checks of the type illustrated in the following code. … The safe
> way of writing this test is as follows. `if (val >= min && val <= max) { … }`"

**Refused, not clamped.** Pharr's clamp to `MaxFloat` is right where `∞` stands for "no end". Here it
would write `f32::MAX` into a cell, and the next tick sums four neighbours of it: `4 × f32::MAX` is `∞`.
Refusing loses nothing a field could have held.

**A finite deposit stops at a limit instead of overflowing.** `grid[i] += amount × falloff` can reach
`±∞` with both operands finite (`f32::MAX` twice, measured above), and the tick then spreads it and turns
it to NaN one operation later. A deposit now writes `(grid[i] + amount × falloff)` clamped to
`±VALUE_LIMIT`. The tick is a convex blend (`diffuse ≤ 1`, so every weight is non-negative and they sum
to 1), so it maps `[−L, L]` into itself in exact arithmetic; its float form sums four neighbours, so `L`
must satisfy `4 L ≤ f32::MAX` with room for rounding.

**`VALUE_LIMIT = f32::MAX / 16` (≈ 2.13 × 10³⁷).** `f32::MAX / 4` is the exact bound; the extra factor of
4 is margin for the rounding in `own · (1 − n·edge) + sum · edge`, which can lift a maximum by an ulp. It is not a
proof that no sequence of ticks ever passes `L` (one ulp per tick for 1.2 × 10⁷ ticks would eat the
margin), and the test does not claim one: it pins 200 ticks of a field filled to the limit. A clamp to a
range that holds is the identity, so every in-range deposit is bit-identical to before. The limit is a
documented number in the two `deposit` docs, not a public constant: a caller's own sentinel is a
finite number it chose, and "stops at about 2 × 10³⁷" is all it needs to know.

**Out of scope, recorded.** `set_channel` takes finite values up to `f32::MAX` as given, and a def with a
NaN `evaporate`, `diffuse` or `accumulate` poisons by a different door; both are in the backlog, not here.
Issue #10 (an `i32` overflow in the disc walk) shares `deposit_disc` and is a different failure.

## Parameters

| Name | Value | Basis |
|---|---|---|
| refused amounts | NaN, `+∞`, `−∞` (scalar); a vector with any such component (rally) | `is_finite()`; design default, matches `set_channel` |
| `VALUE_LIMIT` | `f32::MAX / 16` | `4 L ≤ f32::MAX` for the four-neighbour sum, ×4 margin; design default |
| clamp | after the add, per component for the rally vector | identity on every in-range value |
| `set_channel`, defs | unchanged | not what #9 reports |

## Validation targets

1. The issue's test, `a_non_finite_deposit_leaves_the_field_finite`, fails on `ff92756` and passes.
2. A refused deposit leaves `channels()` / `cells()` bit-identical to before the call (asserted, not
   inferred from "still finite").
3. `a_deposit_stops_at_the_limit_instead_of_overflowing_to_infinity`: two `f32::MAX` deposits at every
   cell, of each sign, read exactly `±f32::MAX / 16`; 200 ticks at `diffuse = 1` stay finite; the rally
   vector stops at `(L, −L)`.
4. Soak, 10 000 ticks × 16 deposits, 1 in 16 hostile: **0 non-finite cells** (before: 5 496 of 6 144).
5. The finite-amount soak and the conservation run hash **bit-for-bit as before** (`3b95a82bf36b7c43`,
   `add48ad2fa2f6e6e`): no in-range value changes.
6. The bench's `determinism` lines equal at 1, 2 and the default thread count and equal to the
   pre-change hash; every throughput hash equal to before (the tick is untouched).
7. `deposit` and `RallyGrid::deposit` within 5% of before, interleaved A/B in one process.

## What great looks like

A bad deposit leaves no trace: the field, the hotspot, the gradient and the examples read exactly as they
would have without the call, and the host learns of its bug from its own data, not from agents that
walk into a NaN. The two kinds of reader agree: where `sample_cell` can return NaN, `hotspot_cell` and
`saturation_stats` must not report a healthy peak; after this change none can, because nothing
non-finite is ever written. The cost is one finite test and one clamp per written cell, below what a
`deposit` call already spends on the disc walk.

## Deviations

- `VALUE_LIMIT = f32::MAX / 16` is a **design default** (derivation above); no source gives a bound for
  a float field.
- A non-finite deposit is **refused**, where Pharr et al. clamp `∞` to `MaxFloat`: a design default,
  argued above. An owner who prefers saturation overrides it on #9; it is one branch.
- Finite negative deposits are accepted as before; only non-finite values and overflow are changed.

## Pending index

- `10.1016/s0167-739x(00)00043-1` — Stützle & Hoos 2000, *MAX-MIN Ant System* (Future Generation Computer
  Systems). It bounds every pheromone trail to `[τ_min, τ_max]`: the model-side precedent for a limit on
  the stored value. Found by `paper_search`; `paper_download` fetched other papers of the search and not
  this one, so it is not in the corpus and the limit above rests on the float argument, not on it.
