//! The caller's cost of a field tick, measured the way a caller pays it.
//!
//! `cargo run --release --example bench` prints one line per case:
//! `stig: <case> grid=<w>x<h> ch=<n> threads=<t> median_us=<x> p99_us=<y> hash=<h>`.
//!
//! - **Throughput:** `evaporate_diffuse` and `RallyGrid::evaporate` per tick over 64², 256² and 1024²
//!   grids with ~70% floor, at 1, 4 and 8 channels: 20 warm-up ticks, then the median and p99 of 200.
//! - **Busy pool:** the same tick on a 175×175 two-channel grid, called from inside a Bevy `TaskPool`
//!   whose every other worker is busy — the caller's wall clock, so a call that parks shows as a stall.
//! - **Determinism:** a fixed 500-tick scenario hashed bit-for-bit at 1, 2 and the default thread count.
//!
//! `STIG_BENCH_QUICK=1` runs fewer ticks and only the small grids, for a smoke check.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bevy::math::IVec2;
use bevy::tasks::{TaskPool, TaskPoolBuilder};
use bevy_stigmergy::{ChannelDef, RallyDef, RallyGrid, StigGrid};

const WARMUP: usize = 20;

fn measured_ticks() -> usize {
    if quick() { 40 } else { 200 }
}

fn quick() -> bool {
    std::env::var("STIG_BENCH_QUICK").is_ok_and(|v| v == "1")
}

/// A fixed, cheap integer hash: the floor layout and the deposit positions come from it, so every run
/// and every machine sees the same scenario.
fn mix(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

/// ~70% floor: blocky rock (8×8 blocks, ~20%) plus scattered single-cell rock (~12%).
fn floor(w: usize, h: usize) -> Vec<IVec2> {
    let mut cells = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let block = mix(((x / 8) as u64) << 32 | (y / 8) as u64) % 100 < 20;
            let speck = mix(((x as u64) << 32 | y as u64) ^ 0x5eed) % 100 < 12;
            if !block && !speck {
                cells.push(IVec2::new(x as i32, y as i32));
            }
        }
    }
    cells
}

fn defs<const N: usize>() -> [ChannelDef; N] {
    std::array::from_fn(|ch| ChannelDef {
        evaporate: 0.05 + 0.02 * ch as f32,
        diffuse: 0.1 + 0.05 * (ch % 3) as f32,
        deposit_radius: 1.5 + (ch % 2) as f32,
    })
}

/// Deposits for tick `t`: a fixed count per channel at hashed cells, some deliberately off-grid.
fn deposit<const N: usize>(grid: &mut StigGrid<N>, w: usize, h: usize, t: usize) {
    for ch in 0..N {
        for k in 0..32u64 {
            let r = mix((t as u64) << 24 ^ (ch as u64) << 12 ^ k);
            let x = (r % (w as u64 + 4)) as i32 - 2;
            let y = ((r >> 32) % (h as u64 + 4)) as i32 - 2;
            grid.deposit(ch, IVec2::new(x, y), 1.0 + (k % 5) as f32);
        }
    }
}

fn fnv(bits: impl Iterator<Item = u32>) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for b in bits {
        for byte in b.to_le_bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn grid_hash<const N: usize>(grid: &StigGrid<N>) -> u64 {
    fnv(grid.channels().iter().flat_map(|c| c.iter().map(|v| v.to_bits())))
}

fn stats(mut us: Vec<f64>) -> (f64, f64) {
    us.sort_by(f64::total_cmp);
    let median = us[us.len() / 2];
    let p99 = us[((us.len() as f64 * 0.99).ceil() as usize).saturating_sub(1).min(us.len() - 1)];
    (median, p99)
}

fn line(case: &str, w: usize, h: usize, ch: usize, threads: &str, us: Vec<f64>, hash: u64) {
    let (median, p99) = stats(us);
    println!(
        "stig: {case} grid={w}x{h} ch={ch} threads={threads} median_us={median:.1} p99_us={p99:.1} hash={hash:016x}"
    );
}

/// The thread count the global pool runs at, as `RAYON_NUM_THREADS` sets it.
fn pool_threads() -> String {
    std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "default".into())
}

/// The default tick: inline on the calling thread.
fn tick<const N: usize>(grid: &mut StigGrid<N>, dt: f32) {
    grid.evaporate_diffuse(dt);
}

/// The caller-run tick: the stencil split into one chunk per thread of the current rayon pool and run
/// there — what a caller with a large grid and its own pool writes.
fn tick_rayon<const N: usize>(grid: &mut StigGrid<N>, dt: f32) {
    let chunks = rayon::current_num_threads();
    grid.evaporate_diffuse_with(dt, chunks, |c| rayon::scope(|s| c.for_each(|chunk| s.spawn(move |_| chunk.run()))));
}

fn throughput<const N: usize>(case: &str, threads: &str, w: usize, h: usize, tick: fn(&mut StigGrid<N>, f32)) {
    let mut grid = StigGrid::<N>::new(w, h, floor(w, h).into_iter(), defs::<N>());
    let mut us = Vec::with_capacity(measured_ticks());
    for t in 0..WARMUP + measured_ticks() {
        deposit(&mut grid, w, h, t);
        let start = Instant::now();
        tick(&mut grid, 0.1);
        let el = start.elapsed().as_secs_f64() * 1e6;
        if t >= WARMUP {
            us.push(el);
        }
    }
    line(case, w, h, N, threads, us, grid_hash(&grid));
}

/// The caller-run tick on a Bevy task pool, written exactly as the README shows it for
/// `ComputeTaskPool`.
fn tick_bevy<const N: usize>(grid: &mut StigGrid<N>, dt: f32) {
    static POOL: LazyLock<TaskPool> = LazyLock::new(|| TaskPoolBuilder::new().build());
    let pool = &*POOL;
    grid.evaporate_diffuse_with(dt, pool.thread_num(), |chunks| {
        pool.scope(|s| chunks.for_each(|chunk| s.spawn(async move { chunk.run() })));
    });
}

fn throughput_all<const N: usize>(w: usize, h: usize) {
    throughput::<N>("evaporate_diffuse", "caller", w, h, tick::<N>);
    throughput::<N>("evaporate_diffuse_with_rayon", &pool_threads(), w, h, tick_rayon::<N>);
    throughput::<N>("evaporate_diffuse_with_bevy", "default", w, h, tick_bevy::<N>);
}

fn rally(w: usize, h: usize) {
    let mut grid = RallyGrid::new(w, h, floor(w, h).into_iter(), RallyDef::default());
    let mut us = Vec::with_capacity(measured_ticks());
    for t in 0..WARMUP + measured_ticks() {
        for k in 0..32u64 {
            let r = mix((t as u64) << 24 ^ k);
            let c = IVec2::new((r % w as u64) as i32, ((r >> 32) % h as u64) as i32);
            grid.deposit(c, bevy::math::Vec2::new(1.0, -0.5));
        }
        let start = Instant::now();
        grid.evaporate(0.1);
        let el = start.elapsed().as_secs_f64() * 1e6;
        if t >= WARMUP {
            us.push(el);
        }
    }
    let hash = fnv(grid.cells().iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits()]));
    line("rally_evaporate", w, h, 1, "1", us, hash);
}

/// Sanctuary's shape (issue #6): a 175×175, two-channel field ticked from inside a Bevy task pool whose
/// other workers are all busy. What is timed is the caller's wall clock around the tick.
fn busy_pool() {
    let (w, h) = (175, 175);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let pool = TaskPoolBuilder::new().num_threads(workers).build();
    let stop = AtomicBool::new(false);
    let mut us = Vec::new();
    let mut hash = 0;
    pool.scope(|s| {
        for _ in 0..workers.saturating_sub(1) {
            s.spawn(async {
                let mut x = 1u64;
                while !stop.load(Ordering::Relaxed) {
                    x = std::hint::black_box(mix(x));
                }
            });
        }
        s.spawn(async {
            let mut grid = StigGrid::<2>::new(w, h, floor(w, h).into_iter(), defs::<2>());
            for t in 0..WARMUP + measured_ticks() {
                deposit(&mut grid, w, h, t);
                let start = Instant::now();
                tick(&mut grid, 0.1);
                let el = start.elapsed().as_secs_f64() * 1e6;
                if t >= WARMUP {
                    us.push(el);
                }
            }
            hash = grid_hash(&grid);
            stop.store(true, Ordering::Relaxed);
        });
    });
    line("busy_pool", w, h, 2, &format!("bevy{workers}"), us, hash);
}

/// A fixed 500-tick scenario, its stencil run in `threads` chunks on a rayon pool of `threads`, hashed
/// bit-for-bit. Every line must print the same hash as the inline `determinism` line.
fn determinism(threads: usize) {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build();
    let Ok(pool) = pool else {
        println!("stig: determinism threads={threads} pool build failed");
        return;
    };
    let hash = pool.install(|| determinism_run(tick_rayon::<4>));
    line("determinism", 128, 128, 4, &threads.to_string(), vec![0.0], hash);
}

fn determinism_run(tick: fn(&mut StigGrid<4>, f32)) -> u64 {
    let (w, h) = (128, 128);
    let mut grid = StigGrid::<4>::new(w, h, floor(w, h).into_iter(), defs::<4>());
    for t in 0..500 {
        deposit(&mut grid, w, h, t);
        tick(&mut grid, 0.1);
    }
    grid_hash(&grid)
}

fn main() {
    let sizes: &[usize] = if quick() { &[64, 256] } else { &[64, 256, 1024] };
    for &s in sizes {
        throughput_all::<1>(s, s);
        throughput_all::<4>(s, s);
        throughput_all::<8>(s, s);
        rally(s, s);
    }
    busy_pool();
    line("determinism", 128, 128, 4, "caller", vec![0.0], determinism_run(tick::<4>));
    let default = std::thread::available_parallelism().map_or(4, |n| n.get());
    for threads in [1, 2, default] {
        determinism(threads);
    }
}
