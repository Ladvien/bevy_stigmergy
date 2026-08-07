//! The scalar half: N decaying influence channels over one cell grid.

use bevy_math::{IVec2, Vec2};

use crate::grid::{deposit_disc, floor_sets, in_grid, row_major, FloorCell};

/// Per-channel behaviour. The caller supplies one of these per channel, typically from its own config.
#[derive(Clone, Copy)]
pub struct ChannelDef {
    /// Fraction of value lost per second (ACO evaporation ρ).
    pub evaporate: f32,
    /// Blend weight `[0,1]` toward the 4-neighbour average each update (Ch.29 diffusion).
    pub diffuse: f32,
    /// Radius, **in cells**, that a single deposit smears over (placement kernel).
    pub deposit_radius: f32,
}

impl Default for ChannelDef {
    fn default() -> Self {
        Self { evaporate: 0.4, diffuse: 0.1, deposit_radius: 1.5 }
    }
}

/// `N` scalar influence channels over one fixed cell grid, row-major `y*width + x`.
///
/// Channels are addressed by a plain `usize` index. The *meaning* of each index is the caller's — a
/// channel table is game content, and a library that named the channels would be naming somebody's
/// game.
///
/// `N` is a const generic rather than a runtime length so the storage stays `[Vec<f32>; N]`: one
/// indirection per channel access, and an array whose iteration a caller can fold reproducibly.
pub struct StigGrid<const N: usize> {
    width: usize,
    height: usize,
    channels: [Vec<f32>; N],
    defs: [ChannelDef; N],
    /// Reused double-buffer for the diffusion pass (avoids per-frame allocation).
    scratch: Vec<f32>,
    /// Reused contiguous, floor-indexed output buffer for the *parallel* diffusion map: one slot per
    /// `floor_cells` entry, same order. The parallel pass writes disjoint slots here (no cross-cell
    /// reduction), then a serial scatter copies it into the grid-indexed `scratch` double-buffer.
    diffuse_out: Vec<f32>,
    /// The floor cells — the only cells that ever carry value — precomputed once so the per-tick passes
    /// skip the rock cells.
    floor_cells: Vec<FloorCell>,
    /// The same set as a mask, for the neighbour test inside the diffusion stencil.
    floor_mask: Vec<bool>,
}

impl<const N: usize> StigGrid<N> {
    /// Allocate empty channels over a `width`×`height` grid whose floor cells are `floor_cells`.
    ///
    /// `floor_cells` must be **ascending row-major** and free of repeats — see
    /// [`crate::grid::floor_sets`]. Cells outside the grid are ignored rather than panicking, so a
    /// caller whose floor set and dimensions disagree gets an empty region, not a crash.
    pub fn new(
        width: usize,
        height: usize,
        floor_cells: impl Iterator<Item = IVec2>,
        defs: [ChannelDef; N],
    ) -> Self {
        let cells = width * height;
        let (floor_cells, floor_mask) = floor_sets(width, height, floor_cells);
        let diffuse_out = vec![0.0; floor_cells.len()];
        Self {
            width,
            height,
            channels: std::array::from_fn(|_| vec![0.0; cells]),
            defs,
            scratch: vec![0.0; cells],
            diffuse_out,
            floor_cells,
            floor_mask,
        }
    }

    /// Point read at a cell (query). Off-grid reads as 0.
    #[inline]
    pub fn sample_cell(&self, channel: usize, c: IVec2) -> f32 {
        if in_grid(c, self.width, self.height) {
            self.channels[channel][row_major(c, self.width)]
        } else {
            0.0
        }
    }

    /// Direction of *increasing* value, magnitude ≈ the local slope. Central differences on the
    /// 4-neighbour cells; follow it with `+`, flee it with `-`.
    #[inline]
    pub fn gradient_cell(&self, channel: usize, c: IVec2) -> Vec2 {
        let at = |dx: i32, dy: i32| -> f32 {
            let n = c + IVec2::new(dx, dy);
            if in_grid(n, self.width, self.height) {
                self.channels[channel][row_major(n, self.width)]
            } else {
                0.0
            }
        };
        Vec2::new(at(1, 0) - at(-1, 0), at(0, 1) - at(0, -1))
    }

    /// Add `amount` at `center`, smeared over the channel's `deposit_radius` with linear falloff. Only
    /// floor cells receive value, so deposits do not bleed into rock.
    pub fn deposit(&mut self, channel: usize, center: IVec2, amount: f32) {
        let (w, h) = (self.width, self.height);
        let radius = self.defs[channel].deposit_radius;
        let mask = &self.floor_mask;
        let grid = &mut self.channels[channel];
        deposit_disc(w, h, mask, center, radius, |i, falloff| {
            grid[i] += amount * falloff;
        });
    }

    /// One evaporation + diffusion step for every channel. `dt` in seconds.
    ///
    /// Diffusion blends only between floor cells, so influence does not seep through walls.
    ///
    /// **Both passes iterate `floor_cells` rather than the whole grid, and that is bit-identical rather
    /// than an approximation:** evaporating a rock cell is `0 · retain`, the double-buffer's rock cells
    /// stay 0 across the swap, and deposits are floor-masked, so a rock cell is invariantly 0. The
    /// neighbour sum keeps its fixed E/W/S/N order — float addition is non-associative, so the order is
    /// load-bearing.
    pub fn evaporate_diffuse(&mut self, dt: f32) {
        use rayon::prelude::*;
        let w = self.width;
        let h = self.height;
        // Split the disjoint field borrows so the parallel map can read `channels[ch]`, write the reused
        // `diffuse_out`, and read `floor_cells`/`floor_mask` at once.
        let Self { channels, defs, scratch, diffuse_out, floor_cells, floor_mask, .. } = self;
        for ch in 0..N {
            let def = defs[ch];
            let retain = (1.0 - def.evaporate * dt).clamp(0.0, 1.0);
            // Evaporate in place (cell-local, cheap) and note whether any mass survives. An empty channel
            // diffuses 0 → 0, so its diffusion is skipped entirely — bit-identical, and on a typical tick
            // several channels are empty.
            let mut any_mass = false;
            {
                let grid = &mut channels[ch];
                for fc in floor_cells.iter() {
                    let v = grid[fc.idx] * retain;
                    grid[fc.idx] = v;
                    any_mass |= v != 0.0;
                }
            }
            if def.diffuse <= 0.0 || !any_mass {
                continue;
            }
            // Blend each floor cell toward the average of its floor neighbours (double-buffered, parallel).
            let diffuse = def.diffuse;
            let grid = &channels[ch];
            diffuse_out
                .par_iter_mut()
                .zip(floor_cells.par_iter())
                .for_each(|(out, fc)| {
                    let (x, y) = (fc.pos.x, fc.pos.y);
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let nb = IVec2::new(x + dx, y + dy);
                        if nb.x >= 0
                            && nb.y >= 0
                            && (nb.x as usize) < w
                            && (nb.y as usize) < h
                            && floor_mask[(nb.y as usize) * w + nb.x as usize]
                        {
                            sum += grid[(nb.y as usize) * w + nb.x as usize];
                            n += 1.0;
                        }
                    }
                    let avg = if n > 0.0 { sum / n } else { grid[fc.idx] };
                    *out = grid[fc.idx] * (1.0 - diffuse) + avg * diffuse;
                });
            // Serial scatter: contiguous floor-indexed results → grid-indexed `scratch`, then swap in.
            for (fc, &v) in floor_cells.iter().zip(diffuse_out.iter()) {
                scratch[fc.idx] = v;
            }
            std::mem::swap(&mut channels[ch], scratch);
        }
    }

    /// The peak `(cell, value)` in a channel.
    ///
    /// `None` when no floor cell carries a positive value. `floor_cells` is ascending-index order and
    /// rock cells are 0 (so they can never beat `best` under the strict `>`), which makes this the
    /// identical **first-max-wins** result as scanning the whole grid.
    pub fn hotspot_cell(&self, channel: usize) -> (Option<IVec2>, f32) {
        let grid = &self.channels[channel];
        let mut best = 0.0f32;
        let mut best_cell = None;
        for fc in &self.floor_cells {
            let v = grid[fc.idx];
            if v > best {
                best = v;
                best_cell = Some(fc.pos);
            }
        }
        (best_cell, best)
    }

    /// Field-degeneracy stats: `(peak, flatness)` where `peak` is the largest value over every channel
    /// and floor cell, and `flatness` is the fraction of floor cells whose strongest channel is at least
    /// **half** that peak.
    ///
    /// A healthy field has a sharp peak over sparse activity (low flatness). A saturated field
    /// (evaporation ≈ 0) has a runaway peak; a whole-map smear (huge radius or diffusion) is high *and*
    /// uniform (flatness → 1), so agents cannot navigate its gradient. Read-only and order-independent.
    pub fn saturation_stats(&self) -> (f32, f32) {
        let per_cell_max = |i: usize| (0..N).map(|ch| self.channels[ch][i]).fold(0.0f32, f32::max);
        let floor = self.floor_cells.len();
        let peak = self.floor_cells.iter().map(|fc| per_cell_max(fc.idx)).fold(0.0f32, f32::max);
        if floor == 0 || peak <= 0.0 {
            return (peak, 0.0);
        }
        let thresh = 0.5 * peak;
        let hot = self.floor_cells.iter().filter(|fc| per_cell_max(fc.idx) >= thresh).count();
        (peak, hot as f32 / floor as f32)
    }

    /// Every channel's full grid, in channel order.
    ///
    /// The deterministic accessor a caller folds into a hash: the **full** grid, so the
    /// rock-cells-stay-zero invariant is pinned too, and not just the floor cells.
    pub fn channels(&self) -> &[Vec<f32>; N] {
        &self.channels
    }

    /// Grid dimensions, `(width, height)`.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
}
