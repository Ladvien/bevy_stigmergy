//! The scalar half: N decaying influence channels over one cell grid.

use bevy_math::{IVec2, Vec2};

use crate::grid::{FloorCell, deposit_disc, floor_sets, in_grid, row_major};

/// Per-channel behaviour. The caller supplies one of these per channel, typically from its own config.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "reflect", derive(bevy_reflect::Reflect))]
pub struct ChannelDef {
    /// Fraction of value lost per unit of the `dt` passed to [`StigGrid::evaporate_diffuse`]: per
    /// second when stepped in seconds, per game-day when stepped in game-days.
    ///
    /// The crate has no clock, so it cannot know which. A siege field stepped at 60 Hz and a society
    /// field stepped once a game-day are both correct uses of the same number, and the same value means
    /// something different in each: `0.4` loses 40% of the field per second in the first and 40% per
    /// game-day in the second.
    pub evaporate: f32,
    /// Blend weight `[0,1]` toward the 4-neighbour average, applied **per call** of
    /// [`StigGrid::evaporate_diffuse`] and *not* scaled by its `dt`. Calling it twice at `dt/2` spreads
    /// twice as much as calling it once at `dt`.
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
    /// Reused contiguous, floor-indexed output buffer for the diffusion stencil: one slot per
    /// `floor_cells` entry, same order. Chunks of the stencil write disjoint runs of it (no cross-cell
    /// reduction), then a serial scatter copies it into the grid-indexed `scratch` double-buffer.
    diffuse_out: Vec<f32>,
    /// One "has run" flag per chunk of the current diffusion pass, reused across ticks, so a chunk the
    /// caller's runner never ran is found and run inline rather than leaving last tick's values behind.
    chunk_done: Vec<bool>,
    /// The floor cells — the only cells that ever carry value — precomputed once so the per-tick passes
    /// skip the rock cells.
    floor_cells: Vec<FloorCell>,
    /// The same set as a mask, for the neighbour test inside the diffusion stencil.
    floor_mask: Vec<bool>,
}

impl<const N: usize> StigGrid<N> {
    /// Allocate empty channels over a `width`×`height` grid whose floor cells are `floor_cells`.
    ///
    /// `floor_cells` must be **ascending row-major** and free of repeats;
    /// [`crate::CellFrame::cells`] and [`crate::CellFrame::floor_cells`] both are. Cells outside the
    /// grid are ignored rather than panicking, so a caller whose floor set and dimensions disagree gets
    /// an empty region, not a crash.
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
            chunk_done: Vec::new(),
            floor_cells,
            floor_mask,
        }
    }

    /// Point read at a cell (query). Off-grid reads as 0, and so does an undefined channel.
    ///
    /// A channel index comes from the caller's config, so an out-of-range one is a bug in the host, not
    /// a reason to take the game down: it reads as the empty field.
    #[inline]
    pub fn sample_cell(&self, channel: usize, c: IVec2) -> f32 {
        let Some(grid) = self.channels.get(channel) else {
            return 0.0;
        };
        if in_grid(c, self.width, self.height) { grid[row_major(c, self.width)] } else { 0.0 }
    }

    /// Direction of *increasing* value, magnitude ≈ the local slope. Central differences on the
    /// 4-neighbour cells; follow it with `+`, flee it with `-`.
    ///
    /// An undefined channel has no slope: it reads as [`Vec2::ZERO`], which every steering rule is
    /// already written to ignore.
    #[inline]
    pub fn gradient_cell(&self, channel: usize, c: IVec2) -> Vec2 {
        let Some(grid) = self.channels.get(channel) else {
            return Vec2::ZERO;
        };
        let at = |dx: i32, dy: i32| -> f32 {
            let n = c + IVec2::new(dx, dy);
            if in_grid(n, self.width, self.height) { grid[row_major(n, self.width)] } else { 0.0 }
        };
        Vec2::new(at(1, 0) - at(-1, 0), at(0, 1) - at(0, -1))
    }

    /// Add `amount` at `center`, smeared over the channel's `deposit_radius` with linear falloff. Only
    /// floor cells receive value, so deposits do not bleed into rock.
    ///
    /// An undefined channel is a no-op: nothing is written, so every channel keeps the value it had.
    pub fn deposit(&mut self, channel: usize, center: IVec2, amount: f32) {
        let (Some(def), Some(grid)) = (self.defs.get(channel), self.channels.get_mut(channel)) else {
            return;
        };
        let (w, h) = (self.width, self.height);
        let radius = def.deposit_radius;
        let mask = &self.floor_mask;
        deposit_disc(w, h, mask, center, radius, |i, falloff| {
            grid[i] += amount * falloff;
        });
    }

    /// One evaporation + diffusion step for every channel, **inline on the calling thread**.
    ///
    /// `dt` is in the caller's own time unit and is used only by evaporation —
    /// [`ChannelDef::evaporate`] is a fraction lost per unit of it. Diffusion is per call, not per unit
    /// of `dt`.
    ///
    /// Diffusion blends only between floor cells, so influence does not seep through walls.
    ///
    /// Nothing here touches a thread pool: the call does its work on the thread that made it and
    /// returns, so a Bevy system that calls it never parks waiting for another pool's threads. A large
    /// grid that wants the stencil spread over workers uses
    /// [`evaporate_diffuse_with`](Self::evaporate_diffuse_with) and the caller's own pool; this is
    /// exactly that call with one chunk run in place, so the two give the same bits.
    ///
    /// **Both passes iterate `floor_cells` rather than the whole grid, and that is bit-identical rather
    /// than an approximation:** evaporating a rock cell is `0 · retain`, the double-buffer's rock cells
    /// stay 0 across the swap, and deposits are floor-masked, so a rock cell is invariantly 0. The
    /// neighbour sum keeps its fixed E/W/S/N order — float addition is non-associative, so the order is
    /// load-bearing.
    pub fn evaporate_diffuse(&mut self, dt: f32) {
        self.evaporate_diffuse_with(dt, 1, |chunks| chunks.for_each(DiffuseChunk::run));
    }

    /// [`evaporate_diffuse`](Self::evaporate_diffuse) with the diffusion stencil handed to the caller
    /// as `chunks` disjoint [`DiffuseChunk`]s, so it runs on **the caller's** threads.
    ///
    /// For every channel that diffuses this tick, `run` is called once with a [`DiffuseChunks`]
    /// iterator; it runs each chunk ([`DiffuseChunk::run`]) wherever it likes and returns when they are
    /// done. In a Bevy app that is the `ComputeTaskPool` the app already has:
    ///
    /// ```ignore
    /// let pool = ComputeTaskPool::get();
    /// grid.evaporate_diffuse_with(dt, pool.thread_num(), |chunks| {
    ///     pool.scope(|s| chunks.for_each(|chunk| s.spawn(async move { chunk.run() })));
    /// });
    /// ```
    ///
    /// `rayon::scope`, `std::thread::scope` and a plain loop work the same way. Evaporation and the
    /// copy back into the grid stay on the calling thread; they are cell-local and cheap.
    ///
    /// **The result is bit-identical for every `chunks` and every runner.** Each chunk reads only the
    /// previous grid and writes only its own cells' slots, with no cross-cell accumulator, so neither
    /// the partition nor the order the chunks run in can change a bit.
    ///
    /// `chunks` is clamped to `1..=` the floor-cell count, so `0` means one chunk. A chunk the runner
    /// drops or forgets instead of running is run inline after `run` returns, so a runner bug costs
    /// time, never a stale field.
    pub fn evaporate_diffuse_with(&mut self, dt: f32, chunks: usize, mut run: impl FnMut(DiffuseChunks<'_>)) {
        let (width, height) = (self.width, self.height);
        // Split the disjoint field borrows so a pass can read `channels[ch]` and the floor sets while
        // its chunks write the reused `diffuse_out` and `chunk_done`.
        let Self { channels, defs, scratch, diffuse_out, chunk_done, floor_cells, floor_mask, .. } = self;
        let chunks = chunks.clamp(1, floor_cells.len().max(1));
        chunk_done.resize(chunks, false);
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
            let pass = DiffusePass { src: &channels[ch], mask: floor_mask, width, height, diffuse: def.diffuse };
            chunk_done.fill(false);
            run(DiffuseChunks::new(pass, floor_cells, diffuse_out, chunk_done));
            // The same partition again: whatever the runner left unrun is run here.
            for chunk in DiffuseChunks::new(pass, floor_cells, diffuse_out, chunk_done) {
                if !*chunk.done {
                    chunk.run();
                }
            }
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
    ///
    /// An undefined channel has no peak: `(None, 0.0)`, the same empty answer it would give for an
    /// empty real one.
    pub fn hotspot_cell(&self, channel: usize) -> (Option<IVec2>, f32) {
        let Some(grid) = self.channels.get(channel) else {
            return (None, 0.0);
        };
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

    /// Restore a channel from a saved snapshot, for a host that persists its field.
    ///
    /// Returns `false` and changes **nothing** when the channel index is out of range, or when `values`
    /// is not exactly `width * height` long — the two ways a stale or foreign save can arrive.
    ///
    /// Floor cells only are copied; rock cells are forced to `0.0`, because
    /// [`evaporate_diffuse`](Self::evaporate_diffuse) relies on that invariant to skip them. A
    /// non-finite saved value becomes `0.0`: a NaN in a field poisons every downstream sum, and the
    /// hotspot and gradient readers would then report a peak nobody can reach.
    pub fn set_channel(&mut self, channel: usize, values: &[f32]) -> bool {
        let (Some(grid), true) = (self.channels.get_mut(channel), values.len() == self.width * self.height)
        else {
            return false;
        };
        grid.fill(0.0);
        for fc in &self.floor_cells {
            let v = values[fc.idx];
            grid[fc.idx] = if v.is_finite() { v } else { 0.0 };
        }
        true
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

/// What every chunk of one channel's diffusion pass shares: the previous grid, the floor mask, the
/// dimensions and the blend weight. Shared references only, so it is `Copy` and `Send`.
#[derive(Clone, Copy)]
struct DiffusePass<'a> {
    src: &'a [f32],
    mask: &'a [bool],
    width: usize,
    height: usize,
    diffuse: f32,
}

/// One slice of a diffusion pass: a contiguous run of floor cells and the output slots only they write.
///
/// Handed out by [`DiffuseChunks`] inside [`StigGrid::evaporate_diffuse_with`]. It is `Send`, so it can
/// be moved into a task on any pool; [`run`](Self::run) it there. Chunks never share an output slot, so
/// they may run concurrently and in any order.
pub struct DiffuseChunk<'a> {
    pass: DiffusePass<'a>,
    cells: &'a [FloorCell],
    out: &'a mut [f32],
    done: &'a mut bool,
}

impl DiffuseChunk<'_> {
    /// Blend each of this chunk's floor cells toward the average of its floor neighbours, reading the
    /// previous grid and writing this chunk's own slots.
    pub fn run(self) {
        let DiffusePass { src, mask, width: w, height: h, diffuse } = self.pass;
        for (out, fc) in self.out.iter_mut().zip(self.cells) {
            let (x, y) = (fc.pos.x, fc.pos.y);
            let mut sum = 0.0;
            let mut n = 0.0;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nb = IVec2::new(x + dx, y + dy);
                if nb.x >= 0
                    && nb.y >= 0
                    && (nb.x as usize) < w
                    && (nb.y as usize) < h
                    && mask[(nb.y as usize) * w + nb.x as usize]
                {
                    sum += src[(nb.y as usize) * w + nb.x as usize];
                    n += 1.0;
                }
            }
            let avg = if n > 0.0 { sum / n } else { src[fc.idx] };
            *out = src[fc.idx] * (1.0 - diffuse) + avg * diffuse;
        }
        *self.done = true;
    }

    /// How many floor cells this chunk updates — a cost estimate for a runner that balances work.
    pub fn cells(&self) -> usize {
        self.cells.len()
    }
}

/// The chunks of one channel's diffusion pass, in ascending cell order: an [`ExactSizeIterator`] of
/// [`DiffuseChunk`]s whose sizes differ by at most one cell.
///
/// Splitting borrows the reused buffers inside [`StigGrid`], so handing out the chunks allocates
/// nothing.
pub struct DiffuseChunks<'a> {
    pass: DiffusePass<'a>,
    cells: &'a [FloorCell],
    out: &'a mut [f32],
    done: &'a mut [bool],
}

impl<'a> DiffuseChunks<'a> {
    fn new(pass: DiffusePass<'a>, cells: &'a [FloorCell], out: &'a mut [f32], done: &'a mut [bool]) -> Self {
        Self { pass, cells, out, done }
    }
}

impl<'a> Iterator for DiffuseChunks<'a> {
    type Item = DiffuseChunk<'a>;

    fn next(&mut self) -> Option<DiffuseChunk<'a>> {
        let remaining = self.done.len();
        if remaining == 0 {
            return None;
        }
        // Ceiling division over the chunks left: earlier chunks take the remainder, so sizes differ by
        // at most one and every chunk is non-empty while there are at least as many cells as chunks.
        let take = self.cells.len().div_ceil(remaining);
        let (cells, rest_cells) = self.cells.split_at_checked(take)?;
        let (out, rest_out) = std::mem::take(&mut self.out).split_at_mut_checked(take)?;
        let (done, rest_done) = std::mem::take(&mut self.done).split_first_mut()?;
        self.cells = rest_cells;
        self.out = rest_out;
        self.done = rest_done;
        Some(DiffuseChunk { pass: self.pass, cells, out, done })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.done.len(), Some(self.done.len()))
    }
}

impl ExactSizeIterator for DiffuseChunks<'_> {}
