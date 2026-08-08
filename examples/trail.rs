//! **A scent trail that spreads, fades, and does not seep through walls.**
//!
//! Two channels over one grid: a slow SCENT that agents lay as they walk, and a fast ALARM. The map
//! has a wall down the middle with a single gap, and the wall cells are simply absent from the floor
//! set — so diffusion cannot cross it. Nobody negotiates; the field is computed once and every agent
//! reads the same one.
//!
//! Run: `cargo run -p bevy_stigmergy --example trail`

use bevy_math::IVec2;
use bevy_stigmergy::{ChannelDef, StigGrid};

const W: usize = 48;
const H: usize = 17;
const SCENT: usize = 0;
const ALARM: usize = 1;
/// The wall runs along this column, with a gap punched at `GAP_Y`.
const WALL_X: i32 = 24;
const GAP_Y: i32 = 8;
const DT: f32 = 0.1;

/// Every cell except the wall column, which keeps its gap.
fn floor_cells() -> impl Iterator<Item = IVec2> {
    (0..H as i32).flat_map(|y| {
        (0..W as i32).filter_map(move |x| {
            let is_wall = x == WALL_X && y != GAP_Y;
            (!is_wall).then_some(IVec2::new(x, y))
        })
    })
}

/// A 10-step ramp, darkest first. `peak` normalises so the plot stays readable as the field decays.
fn glyph(v: f32, peak: f32) -> char {
    const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
    if peak <= 0.0 {
        return ' ';
    }
    let t = (v / peak).clamp(0.0, 1.0);
    let i = ((t * (RAMP.len() - 1) as f32).round() as usize).min(RAMP.len() - 1);
    RAMP[i]
}

fn plot(field: &StigGrid<2>, channel: usize, title: &str) {
    let peak = (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| IVec2::new(x, y)))
        .map(|c| field.sample_cell(channel, c))
        .fold(0.0f32, f32::max);

    println!("\n{title}   (peak {peak:.4})");
    for y in 0..H as i32 {
        let row: String = (0..W as i32)
            .map(|x| {
                let c = IVec2::new(x, y);
                if x == WALL_X && y != GAP_Y {
                    '│'
                } else {
                    glyph(field.sample_cell(channel, c), peak)
                }
            })
            .collect();
        println!("  {row}");
    }
}

fn main() {
    // SCENT lingers and spreads; ALARM is loud and brief. The meaning of each index is the caller's —
    // the crate never names a channel.
    let mut field = StigGrid::<2>::new(
        W,
        H,
        floor_cells(),
        [
            ChannelDef { evaporate: 0.15, diffuse: 0.20, deposit_radius: 1.5 },
            ChannelDef { evaporate: 1.20, diffuse: 0.05, deposit_radius: 3.0 },
        ],
    );

    // An agent walks left-to-right along row 8, through the gap, laying scent as it goes.
    for x in 2..46 {
        field.deposit(SCENT, IVec2::new(x, GAP_Y), 1.0);
        field.evaporate_diffuse(DT);
    }
    plot(&field, SCENT, "SCENT after the walk — note it bleeds through the gap, not the wall");

    // One alarm, off the trail and on the left side of the wall.
    field.deposit(ALARM, IVec2::new(12, 3), 6.0);
    plot(&field, ALARM, "ALARM, one deposit");

    for _ in 0..12 {
        field.evaporate_diffuse(DT);
    }
    plot(&field, ALARM, "ALARM, 1.2s later — evaporate 1.2/s has nearly cleared it");

    // A reader climbs the gradient rather than searching: this is the whole interface for an agent.
    println!("\nGradient of SCENT — what a follower would steer along:");
    for &(x, y) in &[(6, 6), (12, 8), (20, 10), (WALL_X, GAP_Y), (34, 8)] {
        let c = IVec2::new(x, y);
        let g = field.gradient_cell(SCENT, c);
        println!(
            "  cell ({x:>2},{y:>2})  value {:.4}  gradient ({:>6.3}, {:>6.3})",
            field.sample_cell(SCENT, c),
            g.x,
            g.y
        );
    }

    println!(
        "\nThe wall never appears in this crate's API. It is expressed purely by which cells were\n\
         handed to `StigGrid::new` as floor — so a caller's map format stays the caller's."
    );
}
