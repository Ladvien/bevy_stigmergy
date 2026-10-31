//! The vectorial half: one direction per cell, rather than a concentration.

use bevy_math::{IVec2, Vec2};

use crate::grid::{FloorCell, deposit_disc, deposited, floor_sets, in_grid, row_major};

/// Tuning for the vectorial pheromone (mirrors [`crate::ChannelDef`], but for the vector store).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "reflect", derive(bevy_reflect::Reflect))]
pub struct RallyDef {
    /// Decay coefficient `c_d`, a fraction lost per unit of the `dt` passed to
    /// [`RallyGrid::evaporate`]: per second when stepped in seconds, per game-day when stepped in
    /// game-days. Drives both that step and the `(1 - c_d)` term of the accumulation recurrence —
    /// evaporation is the automatic "call it off".
    pub decay: f32,
    /// Accumulation gain `c_a`, applied to each deposited intermediate-vector.
    pub accumulate: f32,
    /// Radius, **in cells**, that a single deposit smears over (placement kernel, linear falloff).
    pub deposit_radius: f32,
}

impl Default for RallyDef {
    fn default() -> Self {
        Self { decay: 0.3, accumulate: 0.5, deposit_radius: 2.0 }
    }
}

/// A vectorial pheromone map: each floor cell stores a 2-D **direction**, not a scalar concentration.
///
/// Tang, Xu, Yu, Zhang & Zhang, "Dynamic target searching and tracking with swarm robots based on
/// stigmergy", Robotics & Autonomous Systems 2019 (DOI 10.1016/j.robot.2019.103251).
///
/// An agent that senses a target deposits an intermediate-vector `s` pointing at it; the map
/// accumulates deposits with decay (`pher = (1 - c_d)·pher + c_a·s`, the paper's `pher_N^m` recurrence)
/// and evaporates each frame. Readers sample **locally** and steer along the stored vector, so a swarm
/// tracks a target's live motion — and an agent far from any arrow reads ≈0 rather than being pulled by
/// a distant beacon, which is the locality a global-peak scalar cannot express.
pub struct RallyGrid {
    width: usize,
    height: usize,
    grid: Vec<Vec2>,
    decay: f32,
    accumulate: f32,
    deposit_radius: f32,
    /// Only floor cells receive value, so evaporation skips the rock cells.
    floor_cells: Vec<FloorCell>,
    cell_flags: Vec<u8>,
}

impl RallyGrid {
    /// Allocate an empty vector grid. `floor_cells` has the same contract as [`crate::StigGrid::new`].
    pub fn new(width: usize, height: usize, floor_cells: impl Iterator<Item = IVec2>, def: RallyDef) -> Self {
        let cells = width * height;
        let (floor_cells, cell_flags) = floor_sets(width, height, floor_cells);
        Self {
            width,
            height,
            grid: vec![Vec2::ZERO; cells],
            decay: def.decay,
            accumulate: def.accumulate,
            deposit_radius: def.deposit_radius,
            floor_cells,
            cell_flags,
        }
    }

    /// Local vectorial read at a cell (query). Off-grid reads as `Vec2::ZERO`.
    ///
    /// Magnitude ≈ the local beacon strength (gate on it); direction ≈ the bearing to the target (steer
    /// along it).
    #[inline]
    pub fn sample_cell(&self, c: IVec2) -> Vec2 {
        if in_grid(c, self.width, self.height) { self.grid[row_major(c, self.width)] } else { Vec2::ZERO }
    }

    /// Accumulate a deposited intermediate-vector `s` (Tang's `c_a·s` term), smeared over
    /// `deposit_radius` with linear falloff. Only floor cells receive value.
    ///
    /// A deposit is a finite quantity or it is nothing. A vector with a NaN or infinite component is a
    /// no-op: nothing is written, and [`evaporate`](Self::evaporate) only multiplies, so it could never
    /// remove one that was. (Tang's own intermediate-vector divides by the step length, so a robot that
    /// has not moved produces `0/0`.) So is any deposit while [`RallyDef::accumulate`] is not finite,
    /// since `c_a·s` is the amount written. A finite `s` that would take a component of a cell past
    /// `±f32::MAX / 16` (about `2.1e37`) stops there instead of overflowing to `∞`.
    pub fn deposit(&mut self, center: IVec2, s: Vec2) {
        if !Self::can_deposit(s, self.accumulate) {
            return;
        }
        let (w, h) = (self.width, self.height);
        let accumulate = self.accumulate;
        let radius = self.deposit_radius;
        let flags = &self.cell_flags;
        let grid = &mut self.grid;
        deposit_disc(w, h, flags, center, radius, |i, falloff| {
            let add = s * (accumulate * falloff);
            grid[i] = Vec2::new(deposited(grid[i].x, add.x), deposited(grid[i].y, add.y));
        });
    }

    /// Is this a deposit the field can hold: the vector and the gain `c_a` both finite?
    ///
    /// **Out of line on purpose.** Inlined into [`deposit`](Self::deposit), the same two tests changed
    /// the code of the disc walk and measured 6–8% slower on a 64² field (1 000 deposits, about 245 µs
    /// each batch, old and new interleaved in one process); called, the walk is the code it was.
    #[inline(never)]
    fn can_deposit(s: Vec2, accumulate: f32) -> bool {
        s.is_finite() && accumulate.is_finite()
    }

    /// One evaporation step: decay every cell toward zero (the `(1 - c_d)` term / the automatic
    /// call-off). `dt` is in the caller's own time unit; [`RallyDef::decay`] is a fraction lost per
    /// unit of it. Iterates floor cells only, which is bit-identical — scaling a zero vector is a no-op.
    pub fn evaporate(&mut self, dt: f32) {
        let retain = (1.0 - self.decay * dt).clamp(0.0, 1.0);
        for fc in &self.floor_cells {
            self.grid[fc.idx] *= retain;
        }
    }

    /// The full grid, in row-major order — the deterministic accessor a caller folds into a hash.
    pub fn cells(&self) -> &[Vec2] {
        &self.grid
    }

    /// Grid dimensions, `(width, height)`.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}
