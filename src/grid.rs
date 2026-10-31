//! The grid convention every field in this crate shares, and the shared deposit kernel.

use bevy_math::IVec2;

/// Row-major index of a cell in a `width`-wide grid: `y * width + x`.
///
/// **This is public because it is a layout contract, not an implementation detail.** A caller that
/// keeps a companion grid over the same cells — a biomass map, a visibility mask — must index it the
/// same way for the two to line up. Stating the convention is cheaper than discovering it.
#[inline]
pub fn row_major(c: IVec2, width: usize) -> usize {
    c.y as usize * width + c.x as usize
}

/// Is `c` inside a `width`×`height` grid? Negative coordinates included, which is why this takes
/// `IVec2` rather than a `usize` pair.
#[inline]
pub fn in_grid(c: IVec2, width: usize, height: usize) -> bool {
    c.x >= 0 && c.y >= 0 && (c.x as usize) < width && (c.y as usize) < height
}

/// A precomputed floor cell: its row-major grid index and which of its four neighbours are floor,
/// carried together so the per-tick passes need no bounds test and no floor-mask lookup.
#[derive(Clone, Copy)]
pub(crate) struct FloorCell {
    pub(crate) idx: usize,
    /// [`NB_PX`] | [`NB_NX`] | [`NB_PY`] | [`NB_NY`] for each neighbour that is an in-grid floor cell.
    pub(crate) floor_nbrs: u8,
}

/// The `+x` neighbour (`idx + 1`) is floor.
pub(crate) const NB_PX: u8 = 1;
/// The `-x` neighbour (`idx - 1`) is floor.
pub(crate) const NB_NX: u8 = 2;
/// The `+y` neighbour (`idx + width`) is floor.
pub(crate) const NB_PY: u8 = 4;
/// The `-y` neighbour (`idx - width`) is floor.
pub(crate) const NB_NY: u8 = 8;
/// The cell itself is floor. A cell's flag byte is `IS_FLOOR | NB_*...` for floor and `0` for rock, so
/// one load answers both "is this floor" and "which of its neighbours are".
pub(crate) const IS_FLOOR: u8 = 16;

/// The floor set, in the two shapes the passes want: an ascending list to iterate, and one flag byte per
/// grid cell to test ([`IS_FLOOR`] and the `NB_*` neighbour bits, `0` for rock).
///
/// **The list is read off the mask, deliberately, and the flags off the list.** They are views of the
/// same set, and flags that disagreed with the list would change diffusion silently at the map edges —
/// the neighbour blend would include a cell the evaporation pass never touched. Deriving one from the
/// other is what leaves nowhere to write that bug.
///
/// The iterator may yield cells in any order and may repeat one; cells outside the grid are dropped.
/// The list comes out ascending row-major and repeat-free whatever it was given, and both properties
/// are load-bearing: [`crate::StigGrid::hotspot_cell`]'s first-max-wins scan depends on the order, and
/// [`crate::DiffuseChunks`] hands each chunk the grid span between its first cell and the next chunk's.
pub(crate) fn floor_sets(
    width: usize,
    height: usize,
    floor_cells: impl Iterator<Item = IVec2>,
) -> (Vec<FloorCell>, Vec<u8>) {
    let mut mask = vec![false; width * height];
    for c in floor_cells {
        if in_grid(c, width, height) {
            mask[row_major(c, width)] = true;
        }
    }
    let cells: Vec<FloorCell> = mask
        .iter()
        .enumerate()
        .filter(|&(_, &floor)| floor)
        .map(|(idx, _)| {
            let (x, y) = (idx % width, idx / width);
            // Each test checks the grid edge first, so the mask is only read for an in-grid neighbour.
            let mut floor_nbrs = 0;
            if x + 1 < width && mask[idx + 1] {
                floor_nbrs |= NB_PX;
            }
            if x > 0 && mask[idx - 1] {
                floor_nbrs |= NB_NX;
            }
            if y + 1 < height && mask[idx + width] {
                floor_nbrs |= NB_PY;
            }
            if y > 0 && mask[idx - width] {
                floor_nbrs |= NB_NY;
            }
            FloorCell { idx, floor_nbrs }
        })
        .collect();
    let mut flags = vec![0u8; width * height];
    for fc in &cells {
        flags[fc.idx] = IS_FLOOR | fc.floor_nbrs;
    }
    (cells, flags)
}

/// Walk the floor cells within `radius` (in cells) of `center`, calling `emit(cell_index, falloff)` with
/// the linear falloff `1 - dist/radius` (`1.0` when `radius == 0`).
///
/// The shared deposit kernel — the disc walk, the in-bounds-and-floor test, and the falloff math — used
/// by both the scalar and the vectorial deposit paths, which differ only in what they accumulate.
///
/// **This masks the destination cell, not the path.** It is a Euclidean disc that skips non-floor
/// cells, not a flood fill and not a line-of-sight test — so a radius wide enough to span a wall does
/// reach the floor on the far side of it. That is deliberate (a deposit is a smell or a sound, and both
/// go round corners), but it surprises often enough to be worth stating. Diffusion is the pass that
/// respects walls.
pub(crate) fn deposit_disc(
    width: usize,
    height: usize,
    cell_flags: &[u8],
    center: IVec2,
    radius: f32,
    mut emit: impl FnMut(usize, f32),
) {
    let radius = radius.max(0.0);
    let r = radius.ceil() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            let cell = center + IVec2::new(dx, dy);
            if !in_grid(cell, width, height) || cell_flags[row_major(cell, width)] & IS_FLOOR == 0 {
                continue;
            }
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            if dist > radius {
                continue;
            }
            let falloff = if radius > 0.0 { 1.0 - dist / radius } else { 1.0 };
            emit(row_major(cell, width), falloff);
        }
    }
}

/// The most a deposit leaves in a cell, in either sign: `f32::MAX / 16`, about `2.13e37`.
///
/// The tick sums four neighbours, so a cell holding more than `f32::MAX / 4` can overflow to `∞` there
/// and then to NaN; `/ 16` leaves a factor of four for the rounding in the blend. It is far above any
/// value a game stores, so a deposit below it is bit-for-bit what `cell + delta` is.
pub(crate) const VALUE_LIMIT: f32 = f32::MAX / 16.0;

/// `cell + delta`, held to `±`[`VALUE_LIMIT`]: what a deposit writes, so two finite deposits can never
/// add up to `∞`. The identity for every sum inside the limit. A NaN stays NaN (`clamp` keeps it),
/// which is what a NaN in a caller's def has always done, not a value this function invents.
#[inline]
pub(crate) fn deposited(cell: f32, delta: f32) -> f32 {
    (cell + delta).clamp(-VALUE_LIMIT, VALUE_LIMIT)
}
