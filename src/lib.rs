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
//! It also does not name the channels. A channel table is game content; a library that shipped one
//! would be naming somebody else's game. Channels are `usize` indices, and `N` is yours to pick.
//!
//! # Cell space, not world space
//!
//! Every entry point takes an `IVec2` **cell**. The crate has no idea how big a cell is or where the
//! grid sits in the world, and it does not want to — a caller that already owns a world↔cell mapping
//! would otherwise have to keep a second copy of it here, and the two would drift.
//!
//! # Determinism
//!
//! The passes are written to be bit-reproducible, because a simulation that hashes its state cannot
//! afford otherwise:
//!
//! * The neighbour sum keeps a fixed E/W/S/N order. Float addition is non-associative, so the order is
//!   load-bearing rather than stylistic.
//! * The diffusion is a pure stencil over disjoint output slots — each cell reads the *previous* grid
//!   and writes only its own slot — so its result is identical for any thread count, which is what
//!   makes the `rayon` pass safe.
//! * Skipping rock cells is exact, not an approximation: deposits are floor-masked, `0 · retain` is
//!   still 0, and the diffusion double-buffer's rock cells stay 0 across the swap.
//! * [`StigGrid::channels`] and [`RallyGrid::cells`] expose the **full** grids so a caller can fold the
//!   rock-cells-stay-zero invariant into its own fingerprint, not just the floor cells.
//!
//! Deposits are applied with a non-associative `+=`, so a caller that produces them in an unordered way
//! (an ECS query, a hash map) must sort its batch before submitting. This crate cannot do that for you:
//! it never sees the batch, only the individual deposits.

pub mod grid;
mod scalar;
mod vector;

pub use grid::{in_grid, row_major};
pub use scalar::{ChannelDef, StigGrid};
pub use vector::{RallyDef, RallyGrid};

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::{IVec2, Vec2};

    /// A `w`×`h` grid whose floor set is every cell — ascending row-major, as the contract requires.
    fn all_floor(w: usize, h: usize) -> impl Iterator<Item = IVec2> {
        (0..h as i32).flat_map(move |y| (0..w as i32).map(move |x| IVec2::new(x, y)))
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
        assert_eq!(g.sample_cell(0, IVec2::new(0, 0)), 1.0, "with no floor neighbour it blends toward itself");
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

    #[test]
    fn a_cell_with_no_floor_neighbours_keeps_its_own_value() {
        // The `n > 0.0` fallback: an isolated cell blends toward ITSELF, so full diffusion is a no-op.
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
}
