//! Issue #6: the field tick runs on the caller's thread, and handing it to a pool cannot change a bit.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use bevy_math::IVec2;
use bevy_stigmergy::{ChannelDef, DiffuseChunk, StigGrid};

/// A floor set with walls in it: every cell except a vertical wall with one gap, and scattered rock, so
/// chunk boundaries fall next to walls as well as open floor.
fn floor(w: usize, h: usize) -> Vec<IVec2> {
    let mut cells = Vec::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let wall = x == w as i32 / 2 && y != 3;
            let rock = (x * 7 + y * 13) % 11 == 0;
            if !wall && !rock {
                cells.push(IVec2::new(x, y));
            }
        }
    }
    cells
}

const DEFS: [ChannelDef; 3] = [
    ChannelDef { evaporate: 0.05, diffuse: 0.2, deposit_radius: 1.5 },
    ChannelDef { evaporate: 0.3, diffuse: 0.45, deposit_radius: 2.5 },
    ChannelDef { evaporate: 0.0, diffuse: 0.1, deposit_radius: 0.0 },
];

fn seeded(w: usize, h: usize) -> StigGrid<3> {
    let mut g = StigGrid::new(w, h, floor(w, h).into_iter(), DEFS);
    g.deposit(0, IVec2::new(2, 2), 10.0);
    g.deposit(1, IVec2::new(w as i32 - 3, h as i32 - 2), 4.0);
    g.deposit(2, IVec2::new(1, h as i32 - 1), 7.0);
    g
}

fn deposit_tick(g: &mut StigGrid<3>, t: i32) {
    g.deposit((t % 3) as usize, IVec2::new((t * 5) % 17, (t * 3) % 11), 1.0 + (t % 4) as f32);
}

/// Before the fix, `evaporate_diffuse` ran its stencil on rayon's global pool and blocked its caller
/// until that pool's threads had run it. With every rayon thread busy, the caller waited for all of
/// them — the 150 ms hitch Sanctuary measured inside a Bevy system. The tick must finish on its own.
#[test]
fn the_tick_finishes_while_rayons_global_pool_is_saturated() {
    let threads = rayon::current_num_threads();
    let started = Arc::new(AtomicUsize::new(0));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    for _ in 0..threads {
        let (started, gate) = (started.clone(), gate.clone());
        rayon::spawn(move || {
            started.fetch_add(1, Ordering::SeqCst);
            let (open, cv) = &*gate;
            let mut open = open.lock().unwrap_or_else(|e| e.into_inner());
            while !*open {
                open = cv.wait(open).unwrap_or_else(|e| e.into_inner());
            }
        });
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while started.load(Ordering::SeqCst) < threads && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(started.load(Ordering::SeqCst), threads, "could not occupy rayon's pool");

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut g = seeded(48, 32);
        for t in 0..20 {
            deposit_tick(&mut g, t);
            g.evaporate_diffuse(0.1);
        }
        let _ = tx.send(g.sample_cell(0, IVec2::new(2, 2)));
    });
    let result = rx.recv_timeout(Duration::from_secs(5));

    let (open, cv) = &*gate;
    *open.lock().unwrap_or_else(|e| e.into_inner()) = true;
    cv.notify_all();

    let value = result.expect("evaporate_diffuse waited on rayon's busy pool instead of running inline");
    assert!(value > 0.0, "the tick ran and the field still holds the deposit");
}

/// Run `ticks` ticks through `step`, returning every channel's bits.
fn run_ticks(ticks: i32, mut step: impl FnMut(&mut StigGrid<3>)) -> Vec<u32> {
    let mut g = seeded(48, 32);
    for t in 0..ticks {
        deposit_tick(&mut g, t);
        step(&mut g);
    }
    g.channels().iter().flat_map(|c| c.iter().map(|v| v.to_bits())).collect()
}

/// The same field for every chunk count and every way of running the chunks: inline, reversed, on a
/// rayon pool, on scoped threads, and with a runner that leaves half the chunks unrun.
#[test]
fn every_partition_and_runner_gives_the_same_bits() {
    let reference = run_ticks(60, |g| g.evaporate_diffuse(0.1));
    let floor_cells = floor(48, 32).len();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(3).build().expect("a private rayon pool");

    for chunks in [0, 1, 2, 3, 7, 64, floor_cells, floor_cells + 5, usize::MAX] {
        let inline = run_ticks(60, |g| g.evaporate_diffuse_with(0.1, chunks, |c| c.for_each(DiffuseChunk::run)));
        assert_eq!(inline, reference, "inline, chunks={chunks}");

        let reversed = run_ticks(60, |g| {
            g.evaporate_diffuse_with(0.1, chunks, |c| {
                let mut all: Vec<DiffuseChunk<'_>> = c.collect();
                while let Some(chunk) = all.pop() {
                    chunk.run();
                }
            })
        });
        assert_eq!(reversed, reference, "reversed, chunks={chunks}");

        let rayon_run = run_ticks(60, |g| {
            g.evaporate_diffuse_with(0.1, chunks, |c| pool.scope(|s| c.for_each(|chunk| s.spawn(move |_| chunk.run()))))
        });
        assert_eq!(rayon_run, reference, "rayon scope, chunks={chunks}");

        let threads = run_ticks(60, |g| {
            g.evaporate_diffuse_with(0.1, chunks, |c| {
                std::thread::scope(|s| c.take(8).for_each(|chunk| drop(s.spawn(move || chunk.run()))))
            })
        });
        assert_eq!(threads, reference, "scoped threads (first 8 only), chunks={chunks}");

        let lazy = run_ticks(60, |g| {
            g.evaporate_diffuse_with(0.1, chunks, |c| c.step_by(2).for_each(std::mem::forget))
        });
        assert_eq!(lazy, reference, "a runner that forgets chunks, chunks={chunks}");
    }
}

/// The chunks cover every floor cell exactly once, in sizes that differ by at most one.
#[test]
fn chunks_partition_the_floor_evenly() {
    let floor_cells = floor(48, 32).len();
    for chunks in [1, 2, 5, 13, floor_cells, floor_cells * 2] {
        let mut g = seeded(48, 32);
        let mut sizes = Vec::new();
        g.evaporate_diffuse_with(0.1, chunks, |c| {
            sizes.clear();
            assert_eq!(c.len(), chunks.min(floor_cells), "chunks={chunks}");
            for chunk in c {
                sizes.push(chunk.cells());
                chunk.run();
            }
        });
        assert_eq!(sizes.iter().sum::<usize>(), floor_cells, "chunks={chunks}");
        let (lo, hi) = (sizes.iter().min().copied(), sizes.iter().max().copied());
        assert!(lo.zip(hi).is_some_and(|(lo, hi)| hi - lo <= 1 && lo > 0), "chunks={chunks}: {sizes:?}");
    }
}

/// 0.2.0's tick, written as plainly as it reads: evaporate every floor cell in place, then blend each
/// floor cell toward the mean of its floor neighbours (E, W, S, N, in that order) into a fresh grid. A
/// channel with no diffusion or no mass left after evaporation only evaporates.
fn reference_tick(channels: &mut [Vec<f32>], mask: &[bool], w: usize, h: usize, defs: &[ChannelDef], dt: f32) {
    for (grid, def) in channels.iter_mut().zip(defs) {
        let retain = (1.0 - def.evaporate * dt).clamp(0.0, 1.0);
        let mut any_mass = false;
        for (v, _) in grid.iter_mut().zip(mask).filter(|(_, floor)| **floor) {
            *v *= retain;
            any_mass |= *v != 0.0;
        }
        if def.diffuse <= 0.0 || !any_mass {
            continue;
        }
        let mut next = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if !mask[i] {
                    continue;
                }
                let (mut sum, mut n) = (0.0f32, 0.0f32);
                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx >= 0 && ny >= 0 && (nx as usize) < w && (ny as usize) < h && mask[ny as usize * w + nx as usize]
                    {
                        sum += grid[ny as usize * w + nx as usize];
                        n += 1.0;
                    }
                }
                let avg = if n > 0.0 { sum / n } else { grid[i] };
                next[i] = grid[i] * (1.0 - def.diffuse) + avg * def.diffuse;
            }
        }
        *grid = next;
    }
}

/// The tick fuses evaporation into the stencil and writes straight into the double-buffer. Neither may
/// change a bit: every tick, from the same state, it must equal the two-pass reference exactly — inline
/// and split into chunks — including the channel that does not diffuse and a `dt` that empties a channel.
#[test]
fn the_tick_matches_the_two_pass_reference_bit_for_bit() {
    let (w, h) = (48, 32);
    let floor_cells = floor(w, h);
    let mut mask = vec![false; w * h];
    for c in &floor_cells {
        mask[c.y as usize * w + c.x as usize] = true;
    }
    let defs = [
        DEFS[0],
        DEFS[1],
        DEFS[2],
        ChannelDef { evaporate: 0.2, diffuse: 0.0, deposit_radius: 1.0 },
    ];
    for chunks in [1, 5] {
        let mut g = StigGrid::new(w, h, floor_cells.iter().copied(), defs);
        for t in 0..120i32 {
            g.deposit((t % 4) as usize, IVec2::new((t * 5) % 47, (t * 3) % 31), 1.0 + (t % 4) as f32);
            // Every 17th tick, a `dt` long enough to empty the fast-evaporating channels outright.
            let dt = if t % 17 == 16 { 5.0 } else { 0.1 };
            let mut expected = g.channels().to_vec();
            reference_tick(&mut expected, &mask, w, h, &defs, dt);
            g.evaporate_diffuse_with(dt, chunks, |c| c.for_each(DiffuseChunk::run));
            let bits = |chs: &[Vec<f32>]| chs.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(g.channels()), bits(&expected), "tick {t}, chunks={chunks}");
        }
    }
}
