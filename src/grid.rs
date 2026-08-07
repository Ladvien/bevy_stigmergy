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

/// A precomputed floor cell: its row-major grid index and its `(x, y)` coordinates, carried together so
/// the per-tick passes need neither a floor test nor an `i % w` / `i / w` recompute.
/// `idx == row_major(pos, width)`.
#[derive(Clone, Copy)]
pub(crate) struct FloorCell {
    pub(crate) idx: usize,
    pub(crate) pos: IVec2,
}

/// The floor set, in the two shapes the passes want: an ascending list to iterate, and a mask to test.
///
/// **Both are built in one pass from one iterator, deliberately.** They are two views of the same set,
/// and a mask that disagreed with the list would change diffusion silently at the map edges — the
/// neighbour blend would include a cell the evaporation pass never touched. Deriving them separately
/// is the bug; this function exists so there is nowhere to write it.
///
/// The iterator must yield cells in **ascending row-major order** and must not repeat one. Ascending
/// order is load-bearing for [`crate::StigGrid::hotspot_cell`], whose first-max-wins scan depends on it.
pub(crate) fn floor_sets(
    width: usize,
    height: usize,
    floor_cells: impl Iterator<Item = IVec2>,
) -> (Vec<FloorCell>, Vec<bool>) {
    let mut cells = Vec::with_capacity(width * height);
    let mut mask = vec![false; width * height];
    for c in floor_cells {
        if !in_grid(c, width, height) {
            continue;
        }
        let idx = row_major(c, width);
        mask[idx] = true;
        cells.push(FloorCell { idx, pos: c });
    }
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
