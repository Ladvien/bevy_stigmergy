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

/// The floor set, in the two shapes the passes want: an ascending list to iterate, and a mask to test.
///
/// **The list is read off the mask, deliberately.** They are two views of the same set, and a mask that
/// disagreed with the list would change diffusion silently at the map edges — the neighbour blend would
/// include a cell the evaporation pass never touched. Deriving one from the other is what leaves nowhere
/// to write that bug.
///
/// The iterator may yield cells in any order and may repeat one; cells outside the grid are dropped.
/// The list comes out ascending row-major and repeat-free whatever it was given, and both properties
/// are load-bearing: [`crate::StigGrid::hotspot_cell`]'s first-max-wins scan depends on the order, and
/// [`crate::DiffuseChunks`] hands each chunk the grid span between its first cell and the next chunk's.
pub(crate) fn floor_sets(
    width: usize,
    height: usize,
    floor_cells: impl Iterator<Item = IVec2>,
) -> (Vec<FloorCell>, Vec<bool>) {
    let mut mask = vec![false; width * height];
    for c in floor_cells {
        if in_grid(c, width, height) {
            mask[row_major(c, width)] = true;
        }
    }
    let cells = mask
        .iter()
        .enumerate()
        .filter(|&(_, &floor)| floor)
        .map(|(idx, _)| {
            let (x, y) = (idx % width, idx / width);
            let floor = |ok: bool, at: usize| if ok && mask[at] { 1 } else { 0 };
            // `&&` short-circuits, so `at` is only read when the neighbour is inside the grid.
            let floor_nbrs = floor(x + 1 < width, idx + 1) * NB_PX
                | floor(x > 0, idx.wrapping_sub(1)) * NB_NX
                | floor(y + 1 < height, idx + width) * NB_PY
                | floor(y > 0, idx.wrapping_sub(width)) * NB_NY;
            FloorCell { idx, floor_nbrs }
        })
        .collect();
    (cells, mask)
}

/// Walk the floor cells within `radius` (in cells) of `center`, calling `emit(cell_index, falloff)` with
/// the linear falloff `1 - dist/radius` (`1.0` when `radius == 0`).
///
/// The shared deposit kernel — the disc walk, the in-bounds-and-floor mask, and the falloff math — used
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
    floor_mask: &[bool],
    center: IVec2,
    radius: f32,
    mut emit: impl FnMut(usize, f32),
) {
    let radius = radius.max(0.0);
    let r = radius.ceil() as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            let cell = center + IVec2::new(dx, dy);
            if !in_grid(cell, width, height) || !floor_mask[row_major(cell, width)] {
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
