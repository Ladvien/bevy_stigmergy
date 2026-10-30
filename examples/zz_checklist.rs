//! Scratch field checklist for iteration 3 (issue #8). Not committed: copied into `examples/` of a tree,
//! built and run there, and removed again.

use bevy_math::{IVec2, Vec2};
use bevy_stigmergy::{ChannelDef, StigGrid};
use std::collections::{HashSet, VecDeque};

const W: usize = 64;
const H: usize = 48;

fn all_floor() -> impl Iterator<Item = IVec2> {
    (0..H as i32).flat_map(move |y| (0..W as i32).map(move |x| IVec2::new(x, y)))
}

fn walled_floor() -> Vec<IVec2> {
    all_floor().filter(|c| if c.x == 32 { c.y == 24 } else { (c.x * 7 + c.y * 13) % 11 != 0 }).collect()
}

fn field_of(f: impl Fn(i32, i32) -> f32) -> Vec<f32> {
    let mut v = Vec::with_capacity(W * H);
    for y in 0..H as i32 {
        for x in 0..W as i32 {
            v.push(f(x, y));
        }
    }
    v
}

fn quiet(floor: &[IVec2], vals: &[f32]) -> StigGrid<1> {
    let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.0, deposit_radius: 0.0 }];
    let mut g = StigGrid::<1>::new(W, H, floor.iter().copied(), defs);
    assert!(g.set_channel(0, vals));
    g
}

const DIRS: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn bfs(floor: &HashSet<IVec2>, src: IVec2) -> std::collections::HashMap<IVec2, u32> {
    let mut d = std::collections::HashMap::new();
    let mut q = VecDeque::new();
    d.insert(src, 0u32);
    q.push_back(src);
    while let Some(c) = q.pop_front() {
        let dc = d[&c];
        for dir in DIRS {
            let n = c + dir;
            if floor.contains(&n) && !d.contains_key(&n) {
                d.insert(n, dc + 1);
                q.push_back(n);
            }
        }
    }
    d
}

fn main() {
    let floor = walled_floor();
    let set: HashSet<IVec2> = floor.iter().copied().collect();
    println!("floor cells: {}", floor.len());

    // 1. uniform field after one tick.
    {
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.25, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(W, H, floor.iter().copied(), defs);
        assert!(g.set_channel(0, &[1.0; W * H]));
        g.evaporate_diffuse(1.0);
        let mut nonzero = 0;
        let mut max = 0.0f32;
        for &c in &floor {
            let v = g.gradient_cell(0, c);
            if v != Vec2::ZERO {
                nonzero += 1;
            }
            max = max.max(v.length());
        }
        println!("uniform: nonzero slope at {nonzero} of {} floor cells, max |grad| {max:.3}", floor.len());
    }

    // 2. ramp 2x + y.
    {
        let g = quiet(&floor, &field_of(|x, y| (2 * x + y) as f32));
        let (mut interior, mut interior_ok, mut boundary, mut downhill) = (0, 0, 0, 0);
        for &c in &floor {
            let v = g.gradient_cell(0, c);
            let four = DIRS.iter().all(|d| set.contains(&(c + *d)));
            if four {
                interior += 1;
                if v == Vec2::new(4.0, 2.0) {
                    interior_ok += 1;
                }
            } else {
                boundary += 1;
                if v.x < 0.0 || v.y < 0.0 {
                    downhill += 1;
                }
            }
        }
        println!("ramp 2x+y: interior {interior_ok}/{interior} read (4,2); boundary cells {boundary}, with a downhill component {downhill}");
        let v = g.gradient_cell(0, IVec2::new(10, 10));
        println!("ramp at (10,10): ({}, {}), angle off uphill {:.4} rad", v.x, v.y, v.angle_to(Vec2::new(4.0, 2.0)).abs());
    }

    // 3. one-axis fields, and fleeing into a face the field is flat across.
    {
        let along_y = quiet(&floor, &field_of(|_, y| (0.37 * y as f32).sin()));
        let along_x = quiet(&floor, &field_of(|x, _| (0.29 * x as f32).cos()));
        let (mut cross_y, mut cross_x) = (0, 0);
        let (mut flee_in, mut faces) = (0, 0);
        for &c in &floor {
            let gy = along_y.gradient_cell(0, c);
            let gx = along_x.gradient_cell(0, c);
            if gy.x != 0.0 {
                cross_y += 1;
            }
            if gx.y != 0.0 {
                cross_x += 1;
            }
            // faces flat across: along_y is flat across x faces; along_x across y faces.
            for d in [IVec2::X, IVec2::NEG_X] {
                if !set.contains(&(c + d)) {
                    faces += 1;
                    if -gy.x * d.x as f32 > 0.0 {
                        flee_in += 1;
                    }
                }
            }
            for d in [IVec2::Y, IVec2::NEG_Y] {
                if !set.contains(&(c + d)) {
                    faces += 1;
                    if -gx.y * d.y as f32 > 0.0 {
                        flee_in += 1;
                    }
                }
            }
        }
        println!("flat-axis fields: cross-axis slope at {cross_y} cells (sin(.37y), x comp) and {cross_x} cells (cos(.29x), y comp)");
        println!("fleeing points into a wall face the field is flat across: {flee_in} of {faces} faces");
    }

    // 4. agents fleeing / climbing a settled source.
    {
        let src = IVec2::new(10, 24);
        let defs = [ChannelDef { evaporate: 0.05, diffuse: 0.25, deposit_radius: 1.5 }];
        let mut g = StigGrid::<1>::new(W, H, floor.iter().copied(), defs);
        for _ in 0..600 {
            g.deposit(0, src, 5.0);
            g.evaporate_diffuse(1.0);
        }
        let dist = bfs(&set, src);
        let starts: Vec<IVec2> = (0..200u64)
            .map(|k| floor[(mix(k ^ 0x51) % floor.len() as u64) as usize])
            .filter(|c| dist.contains_key(c))
            .collect();
        for (name, sign) in [("flee", -1.0f32), ("climb", 1.0f32)] {
            let mut pos = starts.clone();
            for _ in 0..400 {
                for p in pos.iter_mut() {
                    let g = g.gradient_cell(0, *p) * sign;
                    let mut best = (0.0f32, *p);
                    for d in DIRS {
                        let n = *p + d;
                        if set.contains(&n) {
                            let s = g.dot(d.as_vec2());
                            if s > best.0 {
                                best = (s, n);
                            }
                        }
                    }
                    *p = best.1;
                }
            }
            let mean0 = starts.iter().map(|c| dist[c] as f64).sum::<f64>() / starts.len() as f64;
            let mean1 = pos.iter().map(|c| dist[c] as f64).sum::<f64>() / pos.len() as f64;
            let near = pos.iter().filter(|c| dist[c] <= 3).count();
            let wall = pos.iter().filter(|c| DIRS.iter().any(|d| !set.contains(&(**c + *d)))).count();
            println!(
                "agents {name}: {} agents, mean geodesic distance to source {mean0:.1} -> {mean1:.1}, within 3 cells {near}, ending beside a wall/rock {wall}",
                pos.len()
            );
        }
    }

    // 5. untouched by the change, kept for the record: conservation, decay, soak.
    {
        let defs = [ChannelDef { evaporate: 0.0, diffuse: 0.25, deposit_radius: 2.0 }];
        let mut g = StigGrid::<1>::new(W, H, floor.iter().copied(), defs);
        g.deposit(0, IVec2::new(31, 24), 100.0);
        g.deposit(0, IVec2::new(50, 47), 100.0);
        let total = |g: &StigGrid<1>| g.channels()[0].iter().map(|&v| v as f64).sum::<f64>();
        let t0 = total(&g);
        for _ in 0..1000 {
            g.evaporate_diffuse(1.0);
        }
        println!("conservation, evaporation 0, 1000 ticks, walled: rel err {:.3e}", (total(&g) - t0) / t0);

        let defs = [ChannelDef { evaporate: 0.1, diffuse: 0.0, deposit_radius: 0.0 }];
        let mut g = StigGrid::<1>::new(W, H, floor.iter().copied(), defs);
        g.deposit(0, IVec2::new(5, 5), 1.0);
        for _ in 0..20 {
            g.evaporate_diffuse(1.0);
        }
        let want = 0.9f64.powi(20);
        let got = g.sample_cell(0, IVec2::new(5, 5)) as f64;
        println!("decay: {got:.9} vs (0.9)^20 = {want:.9}, rel err {:.1e}", (got - want) / want);

        let defs = [ChannelDef { evaporate: 0.2, diffuse: 0.25, deposit_radius: 2.0 }];
        let mut g = StigGrid::<2>::new(W, H, floor.iter().copied(), [defs[0], defs[0]]);
        let mut refused = 0u64;
        for t in 0..10_000u64 {
            for k in 0..16u64 {
                let r = mix(t * 16 + k);
                let c = IVec2::new((r % (W as u64 + 12)) as i32 - 6, ((r >> 20) % (H as u64 + 12)) as i32 - 6);
                let ch = ((r >> 40) % 3) as usize;
                if ch >= 2 || c.x < 0 || c.y < 0 || c.x >= W as i32 || c.y >= H as i32 {
                    refused += 1;
                }
                g.deposit(ch, c, (r >> 50) as f32 / 100.0);
            }
            g.evaporate_diffuse(0.05);
        }
        let mut nan = 0;
        let mut neg = 0;
        for ch in 0..2 {
            for &v in g.channels()[ch].iter() {
                if !v.is_finite() {
                    nan += 1;
                }
                if v < 0.0 {
                    neg += 1;
                }
            }
        }
        println!("soak 10 000 ticks x 16 deposits ({refused} off-grid/undefined): {nan} non-finite, {neg} negative");
    }
}
