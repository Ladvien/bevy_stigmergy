//! **Stigmergy: coordination through the environment.**
//!
//! Agents write into a shared field and read it back, so they coordinate *through the world* rather
//! than by messaging each other (Holland & Melhuish, "Stigmergy, self-organization, and sorting in
//! collective robotics", 1999; Tang, Liu & Pan, ACO review, IEEE/CAA JAS 2021 — deposit + evaporation +
//! positive feedback). Each channel is a scalar grid over cells, and the three standard influence-map
//! operations are **placement** (deposit), **diffusion** (blur to neighbours), and **query**
//! (sample/gradient) — Lewis, "Escaping the Grid", Game AI Pro 2 Ch.29.
//!
//! The field is computed once and shared by every agent (Mark, "Modular Tactical Influence Maps",
//! Ch.30), which is where emergent *group* behaviour comes from: nobody negotiates, and a crowd still
//! converges on a trail, disperses from a threat, or recruits toward an alarm.
//!
//! Two stores are provided:
//!
//! * [`StigGrid`] — `N` **scalar** channels. Concentrations that decay, spread, and can be climbed.
//! * [`RallyGrid`] — one **vector** per cell, storing a *bearing* rather than an amount, for tracking a
//!   moving target (Tang et al. 2019).
//!
//! # What this crate deliberately does not do
//!
//! It owns no resources, registers no systems, and has no schedule. The caller decides when the field
//! ticks and in what order relative to everything else — which is the only useful arrangement, because
//! "deposit before or after the agents think" is a gameplay decision, not a library's.
//!
//! It owns no threads either. [`StigGrid::evaporate_diffuse`] runs on the thread that calls it, so a
//! Bevy system never parks waiting on another pool; a large grid hands its stencil to the caller's own
//! pool with [`StigGrid::evaporate_diffuse_with`], and gets the same bits.
//!
//! It also does not name the channels. A channel table is game content; a library that shipped one
//! would be naming somebody else's game. Channels are `usize` indices, and `N` is yours to pick.
//!
//! # Cell space, not world space
//!
//! Every entry point takes an `IVec2` **cell**. The crate does not decide how big a cell is or where the
//! grid sits in the world.
//!
//! [`CellFrame`] is an optional plain value type for the mapping nearly every caller writes anyway — an
//! axis-aligned grid of uniform cells: `world_to_cell`, `cell_centre`, the ascending `cells()` a floor
//! set wants, and `covering` for a world rectangle. It is a convenience, not a requirement: a caller with
//! a hex sheet, a rotated board or a camera-relative projection ignores it and keeps its own mapping, and
//! nothing in the crate behaves differently either way.
//!
//! # Determinism
//!
//! The passes are written to be bit-reproducible, because a simulation that hashes its state cannot
//! afford otherwise:
//!
//! * The neighbour sum keeps a fixed E/W/S/N order. Float addition is non-associative, so the order is
//!   load-bearing rather than stylistic.
//! * The diffusion is a pure stencil over disjoint output slots — each cell reads the *previous* grid
//!   and writes only its own slot — so its result is identical however the pass is split and on
//!   however many threads it runs, which is what makes [`StigGrid::evaporate_diffuse_with`] safe to
//!   hand to the caller's pool.
//! * Skipping rock cells is exact, not an approximation: deposits are floor-masked, `0 · retain` is
//!   still 0, and the diffusion double-buffer's rock cells stay 0 across the swap.
//! * [`StigGrid::channels`] and [`RallyGrid::cells`] expose the **full** grids so a caller can fold the
//!   rock-cells-stay-zero invariant into its own fingerprint, not just the floor cells.
//!
//! Deposits are applied with a non-associative `+=`, so a caller that produces them in an unordered way
//! (an ECS query, a hash map) must sort its batch before submitting. This crate cannot do that for you:
//! it never sees the batch, only the individual deposits.

mod frame;
pub mod grid;
mod scalar;
mod vector;

pub use frame::CellFrame;
pub use grid::{in_grid, row_major};
pub use scalar::{ChannelDef, DiffuseChunk, DiffuseChunks, StigGrid};
pub use vector::{RallyDef, RallyGrid};

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::{IVec2, Vec2};

    /// A `w`×`h` grid whose floor set is every cell, ascending row-major.
    fn all_floor(w: usize, h: usize) -> impl Iterator<Item = IVec2> {
        (0..h as i32).flat_map(move |y| (0..w as i32).map(move |x| IVec2::new(x, y)))
    }

    // ---- Issue #3: an out-of-range channel is a documented empty answer, never a panic. ----

    /// The two indices a bad host config actually produces: exactly `N`, and the largest `usize`.
    const BAD_CHANNELS: [usize; 2] = [2, usize::MAX];

    #[test]
    fn an_undefined_channel_samples_as_zero() {
        let mut g = StigGrid::<2>::new(4, 4, all_floor(4, 4), [ChannelDef::default(); 2]);
        g.deposit(0, IVec2::new(1, 1), 5.0);
        for channel in BAD_CHANNELS {
            assert_eq!(g.sample_cell(channel, IVec2::new(1, 1)), 0.0, "channel {channel}");
        }
        // And the real channel is untouched, so the read did not fall back to channel 0.
        assert!(g.sample_cell(0, IVec2::new(1, 1)) > 0.0);
    }

    #[test]
    fn an_undefined_channel_has_no_gradient() {
        let mut g = StigGrid::<2>::new(4, 4, all_floor(4, 4), [ChannelDef::default(); 2]);
        g.deposit(0, IVec2::new(1, 1), 5.0);
        // One cell out from the deposit's centre, where the slope is real: at the centre itself the
        // gradient is zero by symmetry, which would make the positive control prove nothing.
        let on_slope = IVec2::new(2, 1);
        for channel in BAD_CHANNELS {
            assert_eq!(g.gradient_cell(channel, on_slope), Vec2::ZERO, "channel {channel}");
        }
        assert_ne!(g.gradient_cell(0, on_slope), Vec2::ZERO, "the real channel has a slope here");
    }

    #[test]
    fn an_undefined_channel_deposits_nothing() {
        let mut g = StigGrid::<2>::new(4, 4, all_floor(4, 4), [ChannelDef::default(); 2]);
        g.deposit(0, IVec2::new(1, 1), 2.0);
        g.deposit(1, IVec2::new(3, 3), 3.0);
        let before = g.channels().clone();
        for channel in BAD_CHANNELS {
            g.deposit(channel, IVec2::new(2, 2), 9.0);
        }
        assert_eq!(g.channels(), &before, "a deposit on an undefined channel must write nothing");
    }

    #[test]
    fn an_undefined_channel_has_no_hotspot() {
        let mut g = StigGrid::<2>::new(4, 4, all_floor(4, 4), [ChannelDef::default(); 2]);
        g.deposit(0, IVec2::new(1, 1), 5.0);
        for channel in BAD_CHANNELS {
            assert_eq!(g.hotspot_cell(channel), (None, 0.0), "channel {channel}");
        }
        assert_eq!(g.hotspot_cell(0).0, Some(IVec2::new(1, 1)));
    }

    // ---- Restoring a field from a save. ----

    #[test]
    fn a_channel_round_trips_through_the_restore() {
        let mut g = StigGrid::<2>::new(5, 4, all_floor(5, 4), [ChannelDef::default(); 2]);
        // A real field with history: several deposits, so values differ per cell.
        g.deposit(0, IVec2::new(1, 1), 3.0);
        g.deposit(0, IVec2::new(3, 2), 1.5);
        g.deposit(1, IVec2::new(0, 0), 2.0);
        let saved = g.channels().clone();

        let mut restored = StigGrid::<2>::new(5, 4, all_floor(5, 4), [ChannelDef::default(); 2]);
        for (c, values) in saved.iter().enumerate() {
            assert!(restored.set_channel(c, values), "channel {c} should restore");
        }
        assert_eq!(restored.channels(), &saved);
    }

    #[test]
    fn a_restore_of_an_undefined_channel_or_wrong_length_is_refused() {
        let mut g = StigGrid::<2>::new(3, 3, all_floor(3, 3), [ChannelDef::default(); 2]);
        g.deposit(0, IVec2::new(1, 1), 1.0);
        let before = g.channels().clone();
        let correct = vec![1.0_f32; 9];
        for channel in BAD_CHANNELS {
            assert!(!g.set_channel(channel, &correct), "channel {channel} must be refused");
        }
        for len in [0usize, 8, 10] {
            let values = vec![1.0; len];
            assert!(!g.set_channel(0, &values), "{len} values is not a 3x3 grid");
        }
        assert_eq!(g.channels(), &before, "a refused restore must change nothing");
    }

    #[test]
    fn a_restore_forces_rock_cells_to_zero_and_rejects_non_finite_values() {
        // Column 1 is rock. `evaporate_diffuse` skips rock cells on the invariant that they are 0, so a
        // save that carries a stale value there must not bring it back; and a NaN would poison every sum
        // downstream.
        let floor = all_floor(3, 3).filter(|c| c.x != 1);
        let mut g = StigGrid::<2>::new(3, 3, floor, [ChannelDef::default(); 2]);
        let mut values = vec![1.0_f32; 9];
        values[1] = 99.0; // a rock cell, (1, 0)
        values[4] = f32::NAN; // a rock cell, (1, 1)
        values[0] = f32::INFINITY; // a floor cell, (0, 0)
        assert!(g.set_channel(0, &values));

        let channel = &g.channels()[0];
        assert_eq!(channel[1], 0.0, "rock cell (1, 0) must be 0");
        assert_eq!(channel[4], 0.0, "rock cell (1, 1) must be 0");
        assert_eq!(channel[0], 0.0, "a non-finite value must be written as 0");
        assert_eq!(channel[2], 1.0, "a finite floor value is kept");
    }

    #[test]
    fn an_empty_channel_is_skipped_and_stays_empty() {
        let mut g = StigGrid::<2>::new(4, 4, all_floor(4, 4), [ChannelDef::default(); 2]);
        g.evaporate_diffuse(1.0 / 60.0);
        assert!(g.channels()[0].iter().all(|&v| v == 0.0));
        assert!(g.channels()[1].iter().all(|&v| v == 0.0));
    }

    #[test]
    fn a_deposit_falls_off_linearly_and_peaks_at_the_centre() {
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 2.0 }];
        let mut g = StigGrid::<1>::new(5, 5, all_floor(5, 5), defs);
        g.deposit(0, IVec2::new(2, 2), 1.0);
        let centre = g.sample_cell(0, IVec2::new(2, 2));
        let one_out = g.sample_cell(0, IVec2::new(3, 2));
        assert_eq!(centre, 1.0, "falloff at distance 0 is 1 - 0/r = 1");
        assert_eq!(one_out, 0.5, "falloff at distance 1 with radius 2 is 1 - 1/2");
        assert_eq!(g.sample_cell(0, IVec2::new(0, 2)), 0.0, "distance 2 is exactly at the radius edge");
    }

    #[test]
    fn diffusion_does_not_cross_a_wall() {
        // A 3x1 corridor with the middle cell missing: two floor cells that share no edge.
        let floor = [IVec2::new(0, 0), IVec2::new(2, 0)];
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 1.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(3, 1, floor.into_iter(), defs);
        g.deposit(0, IVec2::new(0, 0), 1.0);
        g.evaporate_diffuse(1.0 / 60.0);
        assert_eq!(g.sample_cell(0, IVec2::new(2, 0)), 0.0, "influence must not cross the wall");
        assert_eq!(g.sample_cell(0, IVec2::new(1, 0)), 0.0, "and the wall cell stays zero across the swap");
        assert_eq!(
            g.sample_cell(0, IVec2::new(0, 0)),
            1.0,
            "with no floor neighbour it blends toward itself"
        );
    }

    #[test]
    fn a_deposit_masks_its_destination_cell_but_is_not_line_of_sight() {
        // Worth pinning because it surprises: the kernel is a Euclidean disc that skips non-floor
        // DESTINATIONS. It does not trace a path, so a radius wide enough to span a wall still reaches
        // the floor on the far side. Callers wanting occlusion must supply a smaller radius (or their
        // own visibility test) — the diffusion pass is what respects walls.
        let floor = [IVec2::new(0, 0), IVec2::new(2, 0)];
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 4.0 }];
        let mut g = StigGrid::<1>::new(3, 1, floor.into_iter(), defs);
        g.deposit(0, IVec2::new(0, 0), 1.0);
        assert_eq!(g.sample_cell(0, IVec2::new(1, 0)), 0.0, "the wall cell never receives a deposit");
        assert_eq!(g.sample_cell(0, IVec2::new(2, 0)), 0.5, "but a floor cell past it does: 1 - 2/4");
    }

    // ---- Issue #7: diffusion moves value between floor cells and never makes or destroys it. ----

    /// The floor total, summed in `f64` so the sum itself adds no rounding worth measuring.
    fn floor_total<const N: usize>(g: &StigGrid<N>, channel: usize) -> f64 {
        g.channels()[channel].iter().map(|&v| f64::from(v)).sum()
    }

    /// 64×48 with a wall down x = 32 open only at y = 24, and scattered single rock cells: every cell
    /// beside the wall, a rock or the grid edge has fewer than four floor neighbours.
    fn walled_floor() -> impl Iterator<Item = IVec2> {
        all_floor(64, 48).filter(|c| if c.x == 32 { c.y == 24 } else { (c.x * 7 + c.y * 13) % 11 != 0 })
    }

    /// A 20×20 room with a one-cell-wide corridor leaving it east and ending in a dead end at x = 40.
    fn dead_end_floor() -> impl Iterator<Item = IVec2> {
        all_floor(64, 48).filter(|c| {
            ((2..22).contains(&c.x) && (2..22).contains(&c.y)) || (c.y == 12 && (22..=40).contains(&c.x))
        })
    }

    #[test]
    fn diffusion_moves_value_without_making_or_losing_any() {
        // 0.2.1 blended each cell toward the mean of its floor neighbours, which is not symmetric beside a
        // wall, a rock or the grid edge: in 1000 ticks these three maps lost 9.8%, 11.7% and 35% of their
        // total. Each has a deposit beside a wall, the grid edge or the end of the corridor.
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.25, deposit_radius: 2.0 }];
        let maps: [(&str, Vec<IVec2>, [IVec2; 2]); 3] = [
            ("open", all_floor(64, 48).collect(), [IVec2::new(0, 0), IVec2::new(50, 30)]),
            ("walled", walled_floor().collect(), [IVec2::new(31, 24), IVec2::new(50, 47)]),
            ("dead end", dead_end_floor().collect(), [IVec2::new(40, 12), IVec2::new(2, 2)]),
        ];
        for (name, floor, deposits) in maps {
            let mut g = StigGrid::<1>::new(64, 48, floor.into_iter(), defs);
            for at in deposits {
                g.deposit(0, at, 100.0);
            }
            let start = floor_total(&g, 0);
            for _ in 0..1000 {
                g.evaporate_diffuse(1.0);
            }
            let rel = (floor_total(&g, 0) - start) / start;
            assert!(rel.abs() < 1e-5, "{name}: the floor total moved by {rel:e} with evaporation 0");
        }
    }

    #[test]
    fn with_diffusion_on_the_total_decays_at_exactly_the_evaporation_rate() {
        // Evaporation is the only thing that removes value, so the total after k ticks is (1 - e·dt)^k of
        // the start whatever diffusion does to its shape. 0.2.1 was 6% high after 20 ticks here.
        let defs = [ChannelDef { evaporate: 0.1, diffuse: 0.25, deposit_radius: 2.0 }];
        let mut g = StigGrid::<1>::new(64, 48, walled_floor(), defs);
        g.deposit(0, IVec2::new(31, 24), 10.0);
        let start = floor_total(&g, 0);
        for _ in 0..20 {
            g.evaporate_diffuse(1.0);
        }
        let want = start * 0.9f64.powi(20);
        let rel = (floor_total(&g, 0) - want) / want;
        assert!(rel.abs() < 1e-5, "total off the evaporation rate by {rel:e}");
    }

    #[test]
    fn a_uniform_field_stays_uniform_beside_walls() {
        // The other half of a symmetric exchange: no spurious slope at a wall. A rule that conserved the
        // total by letting each cell give `d / n` of itself to each neighbour would pile value into the
        // cells beside walls, and every agent would read a gradient toward them.
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.25, deposit_radius: 0.0 }];
        let floor: Vec<IVec2> = walled_floor().collect();
        let mut g = StigGrid::<1>::new(64, 48, floor.iter().copied(), defs);
        assert!(g.set_channel(0, &[1.0; 64 * 48]));
        for _ in 0..1000 {
            g.evaporate_diffuse(1.0);
        }
        for c in floor {
            let v = g.sample_cell(0, c);
            assert!((v - 1.0).abs() < 1e-6, "{c} drifted to {v}");
        }
    }

    #[test]
    fn a_cell_with_no_floor_neighbours_keeps_its_own_value() {
        // An isolated cell has no floor face to exchange across, so even full diffusion is a no-op.
        let floor = [IVec2::new(1, 1)];
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 1.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(3, 3, floor.into_iter(), defs);
        g.deposit(0, IVec2::new(1, 1), 1.0);
        g.evaporate_diffuse(1.0 / 60.0);
        assert_eq!(g.sample_cell(0, IVec2::new(1, 1)), 1.0);
    }

    #[test]
    fn evaporation_retains_the_documented_fraction_and_clamps() {
        let defs = [ChannelDef { evaporate: 0.5, diffuse: 0.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(1, 1, all_floor(1, 1), defs);
        g.deposit(0, IVec2::ZERO, 1.0);
        g.evaporate_diffuse(1.0);
        assert_eq!(g.sample_cell(0, IVec2::ZERO), 0.5);
        // A dt large enough to overshoot must clamp at zero, never go negative.
        g.evaporate_diffuse(100.0);
        assert_eq!(g.sample_cell(0, IVec2::ZERO), 0.0);
    }

    #[test]
    fn the_hotspot_is_the_lowest_index_among_tied_maxima() {
        // First-max-wins under a strict `>`, scanning ascending row-major. This is the property that
        // makes the result independent of anything but the grid contents.
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(4, 2, all_floor(4, 2), defs);
        g.deposit(0, IVec2::new(3, 0), 1.0);
        g.deposit(0, IVec2::new(1, 1), 1.0);
        let (cell, v) = g.hotspot_cell(0);
        assert_eq!(v, 1.0);
        assert_eq!(cell, Some(IVec2::new(3, 0)), "index 3 precedes index 5, so it wins the tie");
    }

    #[test]
    fn a_floor_set_in_any_order_with_repeats_is_the_same_field() {
        // A caller's floor set comes from its own map, in whatever order its walk produced. Reversed and
        // with every cell listed twice, it must give the field the ascending list gives: the same bits
        // after a run of ticks (a repeated cell is not evaporated twice), and the same hotspot.
        let defs = [ChannelDef { evaporate: 0.3, diffuse: 0.25, deposit_radius: 1.5 }];
        let floor: Vec<IVec2> = all_floor(9, 7).filter(|c| c.x != 4 || c.y == 5).collect();
        let scrambled = floor.iter().rev().chain(floor.iter()).copied();
        let mut ascending = StigGrid::<1>::new(9, 7, floor.iter().copied(), defs);
        let mut any_order = StigGrid::<1>::new(9, 7, scrambled, defs);
        for t in 0..40 {
            for g in [&mut ascending, &mut any_order] {
                g.deposit(0, IVec2::new(t % 9, (t * 3) % 7), 1.0);
                g.evaporate_diffuse_with(0.1, 3, |chunks| chunks.for_each(DiffuseChunk::run));
            }
        }
        let bits = |g: &StigGrid<1>| g.channels()[0].iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&any_order), bits(&ascending));
        assert_eq!(any_order.hotspot_cell(0), ascending.hotspot_cell(0));
    }

    #[test]
    fn an_empty_channel_has_no_hotspot() {
        let g = StigGrid::<1>::new(3, 3, all_floor(3, 3), [ChannelDef::default(); 1]);
        assert_eq!(g.hotspot_cell(0), (None, 0.0));
    }

    #[test]
    fn the_gradient_points_uphill() {
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 2.0 }];
        let mut g = StigGrid::<1>::new(5, 5, all_floor(5, 5), defs);
        g.deposit(0, IVec2::new(4, 2), 1.0);
        let grad = g.gradient_cell(0, IVec2::new(2, 2));
        assert!(grad.x > 0.0, "value rises toward +x, so the gradient must too: {grad}");
        assert_eq!(grad.y, 0.0, "the deposit is symmetric in y");
    }

    // ---- Issue #8: the gradient beside a wall agrees with the zero-flux wall the tick computes. ----

    /// A `w`×`h` field holding `f(x, y)` at every cell, in the row-major order `set_channel` takes.
    fn field_of(w: usize, h: usize, f: impl Fn(i32, i32) -> f32) -> Vec<f32> {
        let mut values = Vec::with_capacity(w * h);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                values.push(f(x, y));
            }
        }
        values
    }

    /// A one-channel grid over `floor`, 64×48, holding `values`, with the tick off.
    fn held_field(floor: &[IVec2], values: &[f32]) -> StigGrid<1> {
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(64, 48, floor.iter().copied(), defs);
        assert!(g.set_channel(0, values));
        g
    }

    #[test]
    fn a_uniform_field_has_no_slope_anywhere_walls_included() {
        // 0.2.x read a rock or off-grid neighbour as 0.0, so a flat field reported a slope pointing away
        // from every wall, rock and grid edge, and an agent fleeing it was pulled into them. The tick
        // leaves a uniform field exactly uniform, so this is the field an idle map holds.
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.25, deposit_radius: 0.0 }];
        let floor: Vec<IVec2> = walled_floor().collect();
        let mut g = StigGrid::<1>::new(64, 48, floor.iter().copied(), defs);
        assert!(g.set_channel(0, &[1.0; 64 * 48]));
        g.evaporate_diffuse(1.0);
        for c in floor {
            assert_eq!(g.gradient_cell(0, c), Vec2::ZERO, "a flat field has a slope at {c}");
        }
    }

    #[test]
    fn each_floor_face_of_a_cell_adds_its_slope_and_a_wall_face_adds_none() {
        // On the ramp 2x + y the central difference is (4, 2) in open floor: one cell of slope per floor
        // face along an axis. A wall face adds nothing, so beside a wall the component across it is the
        // one-sided difference from the floor side, a 1-wide corridor has no cross slope, and nothing
        // ever points downhill on a ramp that only rises.
        let ramp = field_of(64, 48, |x, y| (2 * x + y) as f32);
        let maps: [(&str, Vec<IVec2>); 2] =
            [("walled", walled_floor().collect()), ("dead end", dead_end_floor().collect())];
        for (name, floor) in maps {
            let g = held_field(&floor, &ramp);
            let cells: std::collections::HashSet<IVec2> = floor.iter().copied().collect();
            let is_floor = |c: IVec2| cells.contains(&c);
            let (mut open, mut beside_wall) = (0, 0);
            for &c in &floor {
                let faces = |a: IVec2, b: IVec2| f32::from(is_floor(c + a)) + f32::from(is_floor(c + b));
                let want = Vec2::new(2.0 * faces(IVec2::X, IVec2::NEG_X), faces(IVec2::Y, IVec2::NEG_Y));
                assert_eq!(g.gradient_cell(0, c), want, "{name}: wrong slope at {c}");
                if want == Vec2::new(4.0, 2.0) {
                    open += 1;
                } else {
                    beside_wall += 1;
                }
            }
            assert!(open > 0 && beside_wall > 0, "{name}: the map must have both open floor and wall faces");
        }
    }

    #[test]
    fn a_field_flat_across_a_wall_has_no_slope_across_it() {
        // A field that varies along one axis only is flat across every wall face on the other, so the
        // component across it is exactly zero at every floor cell, wall or no wall. Fleeing it can then
        // never point into a wall.
        let floor: Vec<IVec2> = walled_floor().collect();
        let along_y = held_field(&floor, &field_of(64, 48, |_, y| (0.37 * y as f32).sin()));
        let along_x = held_field(&floor, &field_of(64, 48, |x, _| (0.29 * x as f32).cos()));
        let (mut y_slopes, mut x_slopes) = (0, 0);
        for &c in &floor {
            let (gy, gx) = (along_y.gradient_cell(0, c), along_x.gradient_cell(0, c));
            assert_eq!(gy.x, 0.0, "a field flat along x has an x slope at {c}");
            assert_eq!(gx.y, 0.0, "a field flat along y has a y slope at {c}");
            y_slopes += usize::from(gy.y != 0.0);
            x_slopes += usize::from(gx.x != 0.0);
        }
        assert!(y_slopes > 1000 && x_slopes > 1000, "the fields must really slope: {y_slopes}, {x_slopes}");
    }

    #[test]
    fn a_cell_that_is_not_floor_has_no_slope() {
        // Rock holds 0.0 as a storage invariant, not as a value, and off the grid there is no field: both
        // read as the empty answer, however far off the grid and next to however steep a slope. A rock
        // cell read against its floor neighbours would push a fleeing agent standing in it deeper in.
        let floor: Vec<IVec2> = walled_floor().collect();
        let g = held_field(&floor, &field_of(64, 48, |x, y| (2 * x + y) as f32));
        let rock = IVec2::new(32, 10);
        assert!(!floor.contains(&rock) && floor.contains(&IVec2::new(31, 10)));
        assert_eq!(g.gradient_cell(0, IVec2::new(31, 10)), Vec2::new(2.0, 2.0), "the floor beside it slopes");
        let not_floor = [rock, IVec2::new(0, 0), IVec2::new(-1, 5), IVec2::new(64, 5), IVec2::new(5, 48)];
        let extremes = [IVec2::MAX, IVec2::MIN, IVec2::new(i32::MAX, 0), IVec2::new(0, i32::MIN)];
        for c in not_floor.into_iter().chain(extremes) {
            assert_eq!(g.gradient_cell(0, c), Vec2::ZERO, "slope at {c}");
        }
    }

    #[test]
    fn saturation_reports_a_sharp_peak_as_not_flat() {
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(10, 10, all_floor(10, 10), defs);
        g.deposit(0, IVec2::new(5, 5), 1.0);
        let (peak, flatness) = g.saturation_stats();
        assert_eq!(peak, 1.0);
        assert_eq!(flatness, 0.01, "1 hot cell of 100");
    }

    #[test]
    fn a_rally_deposit_accumulates_and_evaporates() {
        let def = RallyDef { decay: 0.5, accumulate: 1.0, deposit_radius: 0.0 };
        let mut g = RallyGrid::new(3, 3, all_floor(3, 3), def);
        g.deposit(IVec2::new(1, 1), Vec2::new(1.0, 0.0));
        assert_eq!(g.sample_cell(IVec2::new(1, 1)), Vec2::new(1.0, 0.0));
        g.deposit(IVec2::new(1, 1), Vec2::new(1.0, 0.0));
        assert_eq!(g.sample_cell(IVec2::new(1, 1)), Vec2::new(2.0, 0.0), "deposits accumulate");
        g.evaporate(1.0);
        assert_eq!(g.sample_cell(IVec2::new(1, 1)), Vec2::new(1.0, 0.0), "half retained at decay 0.5");
    }

    #[test]
    fn off_grid_reads_are_zero_rather_than_a_panic() {
        let g = StigGrid::<1>::new(2, 2, all_floor(2, 2), [ChannelDef::default(); 1]);
        assert_eq!(g.sample_cell(0, IVec2::new(-1, 0)), 0.0);
        assert_eq!(g.sample_cell(0, IVec2::new(0, 9)), 0.0);
        let r = RallyGrid::new(2, 2, all_floor(2, 2), RallyDef::default());
        assert_eq!(r.sample_cell(IVec2::new(-1, -1)), Vec2::ZERO);
    }

    // ---- Issue #9: a deposit that is not a finite number must not reach the field. ----

    #[test]
    fn a_non_finite_deposit_leaves_the_field_finite() {
        let defs = [ChannelDef { evaporate: 0.1, diffuse: 0.25, deposit_radius: 2.0 }];
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut g = StigGrid::<1>::new(8, 8, all_floor(8, 8), defs);
            g.deposit(0, IVec2::new(2, 2), 1.0);
            let before = g.channels().clone();
            g.deposit(0, IVec2::new(5, 5), bad);
            assert_eq!(g.channels(), &before, "{bad}: a refused deposit writes nothing");
            for _ in 0..20 {
                g.evaporate_diffuse(0.1);
            }
            assert!(g.channels()[0].iter().all(|v| v.is_finite()), "{bad} poisoned the channel");
            assert!(g.gradient_cell(0, IVec2::new(3, 3)).is_finite());
            assert!(g.sample_cell(0, IVec2::new(2, 2)) > 0.0, "the real deposit survives");
        }
        let mut r = RallyGrid::new(8, 8, all_floor(8, 8), RallyDef::default());
        r.deposit(IVec2::new(4, 4), Vec2::X);
        let before = r.cells().to_vec();
        for bad in [
            Vec2::new(f32::NAN, 1.0),
            Vec2::new(1.0, f32::NAN),
            Vec2::new(f32::INFINITY, 0.0),
            Vec2::new(0.0, f32::NEG_INFINITY),
        ] {
            r.deposit(IVec2::new(4, 4), bad);
            assert_eq!(r.cells(), &before[..], "{bad}: a refused deposit writes nothing");
        }
        r.evaporate(0.1);
        assert!(r.cells().iter().all(|v| v.is_finite()));
        assert!(r.sample_cell(IVec2::new(4, 4)).x > 0.0, "the real deposit survives");
    }

    #[test]
    fn a_deposit_stops_at_the_limit_instead_of_overflowing_to_infinity() {
        let limit = f32::MAX / 16.0;
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 1.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(6, 6, all_floor(6, 6), defs);
        // Every cell takes two of the largest finite deposits, of each sign in turn.
        for sign in [1.0, -1.0] {
            for c in all_floor(6, 6) {
                g.deposit(0, c, sign * f32::MAX);
                g.deposit(0, c, sign * f32::MAX);
            }
            assert!(g.channels()[0].iter().all(|&v| v == sign * limit), "pinned at {sign} * limit");
            // The tick must carry a field this full: four neighbours summed would pass `f32::MAX` if the
            // limit were any higher.
            for _ in 0..200 {
                g.evaporate_diffuse(1.0);
            }
            assert!(g.channels()[0].iter().all(|v| v.is_finite()), "the tick overflowed at {sign} * limit");
            assert!(g.gradient_cell(0, IVec2::new(3, 3)).is_finite());
            g.set_channel(0, &[0.0; 36]);
        }

        let def = RallyDef { decay: 0.0, accumulate: 1.0, deposit_radius: 0.0 };
        let mut r = RallyGrid::new(4, 4, all_floor(4, 4), def);
        for _ in 0..3 {
            r.deposit(IVec2::new(1, 1), Vec2::new(f32::MAX, -f32::MAX));
        }
        assert_eq!(r.sample_cell(IVec2::new(1, 1)), Vec2::new(limit, -limit));
    }
}
