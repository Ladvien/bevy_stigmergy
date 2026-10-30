//! An optional, plain value type for the world↔cell mapping the crate deliberately does not own.

use bevy_math::{IVec2, Vec2};

use crate::grid::in_grid;

/// An axis-aligned world↔cell mapping: where the grid starts, how wide a cell is, and how many cells
/// there are.
///
/// **The grid still thinks in cells.** Every entry point on [`crate::StigGrid`] and
/// [`crate::RallyGrid`] takes an `IVec2`, and this crate still does not know what a cell means in a
/// world. `CellFrame` exists because *most* callers write the identical mapping — floor the
/// difference, divide by a cell size, add the origin back for a centre — and two copies of one formula
/// are two chances to drift. A caller whose mapping is not an axis-aligned uniform grid (a hex sheet, a
/// camera-relative projection, a rotated board) ignores this type and keeps its own; nothing here is
/// required.
///
/// The convention, which is the one this maps with:
///
/// - **Row-major, `y * width + x`**, ascending — the order [`StigGrid::new`](crate::StigGrid::new) keeps
///   its floor set in.
/// - **A cell owns the square from its corner, not its centre**: `world_to_cell` **floors**, so
///   `-0.01` lands in cell `-1`, not cell `0`.
/// - **A centre is half a cell in**: `cell_centre` adds `0.5 * cell_size`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "reflect", derive(bevy_reflect::Reflect))]
pub struct CellFrame {
    /// World position of cell `(0, 0)`'s **lower-left corner**.
    pub origin: Vec2,
    /// Width and height of one cell, in world units. Must be finite and positive for the frame to map
    /// anything.
    pub cell_size: f32,
    /// Cells along X, ascending.
    pub width: usize,
    /// Cells along Y, ascending.
    pub height: usize,
}

impl CellFrame {
    /// A frame with an explicit origin and size. The caller is responsible for `cell_size` being
    /// positive; a frame built with a bad one maps every point off-grid rather than panicking.
    pub fn new(origin: Vec2, cell_size: f32, width: usize, height: usize) -> Self {
        Self { origin, cell_size, width, height }
    }

    /// The frame that covers the world rectangle `min`..`max` in cells of `cell_size`, with `min` as
    /// the origin.
    ///
    /// Returns a `0×0` frame when `cell_size` is not finite or `<= 0`, or when `max` is less than `min`
    /// on either axis: there is no rectangle to cover, and a negative size would wrap into a huge one
    /// when cast to `usize`.
    pub fn covering(min: Vec2, max: Vec2, cell_size: f32) -> Self {
        let bad = !min.is_finite()
            || !max.is_finite()
            || !cell_size.is_finite()
            || cell_size <= 0.0
            || max.x < min.x
            || max.y < min.y;
        if bad {
            return Self { origin: min, cell_size: 0.0, width: 0, height: 0 };
        }
        // `ceil` so a last partial cell is inside the frame rather than clipped off its edge.
        let width = ((max.x - min.x) / cell_size).ceil();
        let height = ((max.y - min.y) / cell_size).ceil();
        Self {
            origin: min,
            cell_size,
            // A rectangle so large that its cell count does not fit a `usize` is not a frame any caller
            // has; clamping keeps the conversion total.
            width: if width >= 0.0 { width.min(usize::MAX as f32) as usize } else { 0 },
            height: if height >= 0.0 { height.min(usize::MAX as f32) as usize } else { 0 },
        }
    }

    /// The cell a world point falls in: `floor((p - origin) / cell_size)` on each axis.
    ///
    /// A non-finite point, or a frame whose `cell_size` is not usable, gives
    /// `IVec2::splat(-1)` — a cell outside any non-empty frame, so every reader already treats it as
    /// off-grid. It does not panic, and it does not alias cell `(0, 0)`.
    #[inline]
    pub fn world_to_cell(&self, p: Vec2) -> IVec2 {
        if !p.is_finite() || !self.cell_size.is_finite() || self.cell_size <= 0.0 {
            return IVec2::splat(-1);
        }
        let local = (p - self.origin) / self.cell_size;
        IVec2::new(local.x.floor() as i32, local.y.floor() as i32)
    }

    /// The world position of a cell's centre: `origin + (c + 0.5) * cell_size`.
    #[inline]
    pub fn cell_centre(&self, c: IVec2) -> Vec2 {
        self.origin + (c.as_vec2() + Vec2::splat(0.5)) * self.cell_size
    }

    /// Is `c` inside the frame? Negative coordinates included, which is the whole reason it takes an
    /// `IVec2`.
    #[inline]
    pub fn contains(&self, c: IVec2) -> bool {
        in_grid(c, self.width, self.height)
    }

    /// Every cell, **ascending row-major** — the order [`StigGrid::new`](crate::StigGrid::new) keeps its
    /// floor set in, so `grid(…, frame.cells(), …)` is already in the crate's own order.
    pub fn cells(&self) -> impl Iterator<Item = IVec2> + '_ {
        (0..self.height).flat_map(move |y| (0..self.width).map(move |x| IVec2::new(x as i32, y as i32)))
    }

    /// The cells whose **centre** passes `is_floor`, in the same ascending order as [`cells`](Self::cells).
    ///
    /// Testing the centre rather than the corner is what makes a wall the caller drew between two cells
    /// land in one of them, instead of the boundary flipping on a floating-point tie.
    pub fn floor_cells<'a>(
        &'a self,
        mut is_floor: impl FnMut(Vec2) -> bool + 'a,
    ) -> impl Iterator<Item = IVec2> + 'a {
        self.cells().filter(move |&c| is_floor(self.cell_centre(c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_point_round_trips_through_its_cell_centre() {
        let frame = CellFrame::new(Vec2::new(-8.0, -4.0), 2.0, 8, 4);
        for y in 0..frame.height as i32 {
            for x in 0..frame.width as i32 {
                let c = IVec2::new(x, y);
                let centre = frame.cell_centre(c);
                assert_eq!(frame.world_to_cell(centre), c, "cell {c:?} did not round-trip from {centre}");
                assert!(frame.contains(c));
            }
        }
    }

    #[test]
    fn a_cell_owns_the_square_from_its_corner() {
        let frame = CellFrame::new(Vec2::ZERO, 1.0, 4, 4);
        // The corner itself is in the cell; just below it is not (floor, not round).
        assert_eq!(frame.world_to_cell(Vec2::new(0.0, 0.0)), IVec2::new(0, 0));
        assert_eq!(frame.world_to_cell(Vec2::new(0.99, 0.99)), IVec2::new(0, 0));
        assert_eq!(frame.world_to_cell(Vec2::new(1.0, 0.0)), IVec2::new(1, 0));
        assert_eq!(frame.world_to_cell(Vec2::new(-0.01, 0.0)), IVec2::new(-1, 0));
        assert!(!frame.contains(IVec2::new(-1, 0)));
    }

    #[test]
    fn covering_sizes_to_the_rectangle() {
        // A 10 x 6 rectangle in 2-unit cells is 5 x 3 cells, exactly.
        let frame = CellFrame::covering(Vec2::new(0.0, 0.0), Vec2::new(10.0, 6.0), 2.0);
        assert_eq!((frame.width, frame.height), (5, 3));
        assert_eq!(frame.origin, Vec2::ZERO);

        // A partial last cell is inside the frame: 5 units of 2-unit cells is 3 cells, not 2.
        let frame = CellFrame::covering(Vec2::new(-1.0, -1.0), Vec2::new(4.0, 3.0), 2.0);
        assert_eq!((frame.width, frame.height), (3, 2));
        assert_eq!(frame.origin, Vec2::new(-1.0, -1.0));
        // The far corner is inside, which is the point of the `ceil`.
        assert!(frame.contains(frame.world_to_cell(Vec2::new(3.9, 2.9))));
    }

    #[test]
    fn covering_refuses_a_rectangle_it_cannot_cover() {
        // Zero, negative and non-finite cell sizes; inverted and non-finite corners. Every one is a
        // 0x0 frame rather than a wrapped-around usize.
        let bad = [
            (Vec2::ZERO, Vec2::new(1.0, 1.0), 0.0),
            (Vec2::ZERO, Vec2::new(1.0, 1.0), -1.0),
            (Vec2::ZERO, Vec2::new(1.0, 1.0), f32::NAN),
            (Vec2::new(1.0, 0.0), Vec2::ZERO, 1.0),
            (Vec2::new(f32::NAN, 0.0), Vec2::new(1.0, 1.0), 1.0),
        ];
        for (min, max, size) in bad {
            let frame = CellFrame::covering(min, max, size);
            assert_eq!((frame.width, frame.height), (0, 0), "covering({min}, {max}, {size}) should be empty");
        }
    }

    #[test]
    fn unusable_input_maps_off_grid_rather_than_panicking() {
        let frame = CellFrame::new(Vec2::ZERO, 1.0, 4, 4);
        for p in [Vec2::new(f32::NAN, 0.0), Vec2::new(0.0, f32::INFINITY)] {
            let c = frame.world_to_cell(p);
            assert_eq!(c, IVec2::splat(-1), "{p} should map off-grid");
            assert!(!frame.contains(c));
        }
        // A frame with a bad cell size cannot map at all, not just for NaN points.
        for size in [0.0_f32, -2.0, f32::NAN] {
            let broken = CellFrame::new(Vec2::ZERO, size, 4, 4);
            assert_eq!(broken.world_to_cell(Vec2::ZERO), IVec2::splat(-1), "cell_size {size}");
        }
    }

    #[test]
    fn cells_are_ascending_row_major_and_complete() {
        let frame = CellFrame::new(Vec2::ZERO, 1.0, 5, 3);
        let cells: Vec<IVec2> = frame.cells().collect();
        assert_eq!(cells.len(), 15);
        for pair in cells.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                b.y > a.y || (b.y == a.y && b.x == a.x + 1),
                "{a:?} then {b:?} is not ascending row-major"
            );
        }
        assert_eq!(cells[0], IVec2::new(0, 0));
        assert_eq!(cells[14], IVec2::new(4, 2));
    }

    #[test]
    fn floor_cells_keeps_the_order_and_tests_centres() {
        let frame = CellFrame::new(Vec2::ZERO, 1.0, 4, 4);
        // Keep the left column: cells whose centre x is below 1.0.
        let kept: Vec<IVec2> = frame.floor_cells(|centre| centre.x < 1.0).collect();
        assert_eq!(kept, vec![IVec2::new(0, 0), IVec2::new(0, 1), IVec2::new(0, 2), IVec2::new(0, 3)]);
    }
}
