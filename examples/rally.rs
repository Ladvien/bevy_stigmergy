//! **The vectorial pheromone: a swarm tracking a target that keeps moving.**
//!
//! A scalar field has one global peak, so followers converge on where the target *was*. This stores a
//! direction per cell instead: scouts that can see the target deposit an intermediate-vector pointing
//! at it, the map accumulates with decay (`pher = (1-c_d)·pher + c_a·s`), and a reader far from any
//! arrow reads ≈0 rather than being dragged toward a stale beacon.
//!
//! Tang, Xu, Yu, Zhang & Zhang, "Dynamic target searching and tracking with swarm robots based on
//! stigmergy", Robotics & Autonomous Systems 2019.
//!
//! Run: `cargo run -p bevy_stigmergy --example rally`

use bevy_math::{IVec2, Vec2};
use bevy_stigmergy::{RallyDef, RallyGrid};

const W: usize = 40;
const H: usize = 15;
const DT: f32 = 0.1;

fn all_floor() -> impl Iterator<Item = IVec2> {
    (0..H as i32).flat_map(|y| (0..W as i32).map(move |x| IVec2::new(x, y)))
}

/// Eight-way arrow for a direction, or `·` when the cell holds no usable signal.
fn arrow(v: Vec2) -> char {
    if v.length() < 0.02 {
        return '·';
    }
    const DIRS: [char; 8] = ['→', '↘', '↓', '↙', '←', '↖', '↑', '↗'];
    let a = v.y.atan2(v.x);
    let oct = (a / std::f32::consts::FRAC_PI_4).round().rem_euclid(8.0) as usize;
    DIRS[oct.min(7)]
}

fn plot(field: &RallyGrid, target: IVec2, scouts: &[IVec2], title: &str) {
    println!("\n{title}");
    for y in 0..H as i32 {
        let row: String = (0..W as i32)
            .map(|x| {
                let c = IVec2::new(x, y);
                if c == target {
                    'T'
                } else if scouts.contains(&c) {
                    'S'
                } else {
                    arrow(field.sample_cell(c))
                }
            })
            .collect();
        println!("  {row}");
    }
}

fn main() {
    let mut field = RallyGrid::new(
        W,
        H,
        all_floor(),
        RallyDef { decay: 0.35, accumulate: 0.6, deposit_radius: 4.0 },
    );

    // Three scouts that can see the target and keep marking it. They never move.
    let scouts = [IVec2::new(6, 3), IVec2::new(8, 11), IVec2::new(20, 7)];

    // The target walks right across the map. Each tick every scout deposits a fresh vector at its own
    // cell pointing at where the target is *now*.
    let mut target = IVec2::new(10, 7);
    for step in 0..90 {
        for &s in &scouts {
            let to_target = (target - s).as_vec2();
            if to_target.length() > 0.0 {
                field.deposit(s, to_target.normalize());
            }
        }
        field.evaporate(DT);

        if step % 3 == 0 && target.x < (W as i32 - 4) {
            target.x += 1;
        }
        if step == 30 {
            plot(&field, target, &scouts, "t = 3.0s — arrows lead to the target's live position");
        }
    }
    plot(&field, target, &scouts, "t = 9.0s — the target moved, and the field followed it");

    println!("\nWhat a follower reads at a few cells:");
    for &(x, y) in &[(6, 3), (12, 7), (20, 7), (34, 2), (2, 13)] {
        let v = field.sample_cell(IVec2::new(x, y));
        println!("  cell ({x:>2},{y:>2})  vector ({:>6.3}, {:>6.3})  |v| {:.3}", v.x, v.y, v.length());
    }

    // Stop marking: decay is the automatic "call it off". Nobody has to broadcast a cancel.
    for _ in 0..60 {
        field.evaporate(DT);
    }
    plot(&field, target, &scouts, "scouts stopped marking, 6s of decay — the recruitment expires itself");
}
