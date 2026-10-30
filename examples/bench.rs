//! The caller's cost of a field tick, measured the way a caller pays it.
//!
//! `cargo run --release --example bench` prints one line per case:
//! `stig: <case> grid=<w>x<h> ch=<n> threads=<t> median_us=<x> p99_us=<y> hash=<h>`.
//!
//! - **Throughput:** the tick per call over 64², 256² and 1024² grids with ~70% floor, at 1, 4 and 8
//!   channels: 20 warm-up ticks, then the median and p99 of 200. `evaporate_diffuse` runs inline on the
//!   calling thread (`threads=caller`); `evaporate_diffuse_with_rayon` hands its chunks to the current
//!   rayon pool (`RAYON_NUM_THREADS` sets it) and `evaporate_diffuse_with_bevy` to Bevy's
//!   `ComputeTaskPool`, the two ways a caller with a large grid spreads it. `rally_evaporate` is
//!   `RallyGrid::evaporate`.
//! - **Busy pool** (issue #6): a 175×175 two-channel tick from inside a Bevy `App` system whose
//!   `ComputeTaskPool` has every other worker busy. What is timed is the caller's wall clock around the
//!   call, so a call that parks shows as the stall it is; the line adds `max_us`, `parked_ticks` (ticks in
//!   which the calling thread went to sleep: a park on a latch) and `preempted_ticks` (ticks in which the
//!   OS took its core away: the host's load, not the crate's doing).
//! - **Determinism:** a fixed 500-tick scenario hashed bit-for-bit, inline and with the stencil run in
//!   1, 2 and the default number of chunks on a rayon pool of as many threads, and on Bevy's pool. Every
//!   `determinism` line must print the same hash.
//!
//! `STIG_BENCH_QUICK=1` runs fewer ticks and only the small grids, for a smoke check;
//! `STIG_BENCH_ONLY=busy_pool` runs the busy-pool cases alone.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bevy::app::{App, TaskPoolOptions, TaskPoolPlugin, Update};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy::math::{IVec2, Vec2};
use bevy::tasks::ComputeTaskPool;
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

/// `(median, p99, max)` in microseconds.
fn stats(mut us: Vec<f64>) -> (f64, f64, f64) {
    us.sort_by(f64::total_cmp);
    let median = us[us.len() / 2];
    let p99 = us[((us.len() as f64 * 0.99).ceil() as usize).saturating_sub(1).min(us.len() - 1)];
    (median, p99, us[us.len() - 1])
}

fn line(case: &str, w: usize, h: usize, ch: usize, threads: &str, us: Vec<f64>, hash: u64) {
    let (median, p99, _) = stats(us);
    println!(
        "stig: {case} grid={w}x{h} ch={ch} threads={threads} median_us={median:.1} p99_us={p99:.1} hash={hash:016x}"
    );
}

/// The thread count the global rayon pool runs at, as `RAYON_NUM_THREADS` sets it.
fn pool_threads() -> String {
    std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "default".into())
}

/// The default tick: inline on the calling thread.
fn tick<const N: usize>(grid: &mut StigGrid<N>, dt: f32) {
    grid.evaporate_diffuse(dt);
}

/// The caller-run tick on rayon: one chunk per thread of the current rayon pool — what a headless batch
/// caller with a large grid and its own pool writes.
fn tick_rayon<const N: usize>(grid: &mut StigGrid<N>, dt: f32) {
    let chunks = rayon::current_num_threads();
    grid.evaporate_diffuse_with(dt, chunks, |c| {
        rayon::scope(|s| c.for_each(|chunk| s.spawn(move |_| chunk.run())))
    });
}

/// The caller-run tick on Bevy's `ComputeTaskPool`, written exactly as the README shows it.
fn tick_bevy<const N: usize>(grid: &mut StigGrid<N>, dt: f32) {
    let pool = ComputeTaskPool::get();
    grid.evaporate_diffuse_with(dt, pool.thread_num(), |chunks| {
        pool.scope(|s| chunks.for_each(|chunk| s.spawn(async move { chunk.run() })));
    });
}

fn throughput<const N: usize>(
    case: &str,
    threads: &str,
    w: usize,
    h: usize,
    tick: fn(&mut StigGrid<N>, f32),
) {
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

fn throughput_all<const N: usize>(w: usize, h: usize) {
    throughput::<N>("evaporate_diffuse", "caller", w, h, tick::<N>);
    throughput::<N>("evaporate_diffuse_with_rayon", &pool_threads(), w, h, tick_rayon::<N>);
    let bevy_threads = format!("bevy{}", ComputeTaskPool::get().thread_num());
    throughput::<N>("evaporate_diffuse_with_bevy", &bevy_threads, w, h, tick_bevy::<N>);
}

fn rally(w: usize, h: usize) {
    let mut grid = RallyGrid::new(w, h, floor(w, h).into_iter(), RallyDef::default());
    let mut us = Vec::with_capacity(measured_ticks());
    for t in 0..WARMUP + measured_ticks() {
        for k in 0..32u64 {
            let r = mix((t as u64) << 24 ^ k);
            let c = IVec2::new((r % w as u64) as i32, ((r >> 32) % h as u64) as i32);
            grid.deposit(c, Vec2::new(1.0, -0.5));
        }
        let start = Instant::now();
        grid.evaporate(0.1);
        let el = start.elapsed().as_secs_f64() * 1e6;
        if t >= WARMUP {
            us.push(el);
        }
    }
    let hash = fnv(grid.cells().iter().flat_map(|v| [v.x.to_bits(), v.y.to_bits()]));
    line("rally_evaporate", w, h, 1, "caller", us, hash);
}

/// This thread's `(voluntary, involuntary)` context switches from Linux's `/proc/thread-self/status`;
/// `None` elsewhere. A voluntary switch is the thread going to sleep — parking on a latch. An
/// involuntary one is the OS preempting it, which a loaded host does to any thread.
fn context_switches() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/thread-self/status").ok()?;
    let field = |name: &str| {
        text.lines().find_map(|l| l.strip_prefix(name)).and_then(|v| v.trim().parse::<u64>().ok())
    };
    Some((field("voluntary_ctxt_switches:")?, field("nonvoluntary_ctxt_switches:")?))
}

/// The field a busy-pool system ticks, and what it has measured so far.
#[derive(Resource)]
struct BusyField {
    grid: StigGrid<2>,
    tick: fn(&mut StigGrid<2>, f32),
    t: usize,
    us: Vec<f64>,
    parked: Option<usize>,
    preempted: Option<usize>,
    thread: String,
}

/// The system under test: deposit, then time the tick on whichever thread the executor gave it.
fn busy_tick(mut field: ResMut<BusyField>) {
    let f = &mut *field;
    let (w, h) = f.grid.dimensions();
    deposit(&mut f.grid, w, h, f.t);
    let c0 = context_switches();
    let start = Instant::now();
    (f.tick)(&mut f.grid, 0.1);
    let el = start.elapsed().as_secs_f64() * 1e6;
    let c1 = context_switches();
    if f.t >= WARMUP {
        f.us.push(el);
        match (c0, c1) {
            (Some((v0, n0)), Some((v1, n1))) => {
                f.parked = f.parked.map(|p| p + usize::from(v1 > v0));
                f.preempted = f.preempted.map(|p| p + usize::from(n1 > n0));
            }
            _ => (f.parked, f.preempted) = (None, None),
        }
    }
    if f.thread.is_empty() {
        f.thread = std::thread::current().name().unwrap_or("unnamed").replace(' ', "_");
    }
    f.t += 1;
}

/// Sanctuary's shape (issue #6): a 175×175 field with mass in both channels, ticked from a system of a
/// Bevy `App` while every other `ComputeTaskPool` worker spins on work of its own.
fn busy_pool(case: &str, tick: fn(&mut StigGrid<2>, f32)) {
    let (w, h) = (175, 175);
    let ticks = if quick() { 200 } else { 1000 };
    let pool = ComputeTaskPool::get();
    let workers = pool.thread_num();
    let stop = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicUsize::new(0));
    // Leave exactly one worker free: Bevy's executor runs a `Send` system on a pool worker, never on the
    // thread that called `update`, so a pool with no free worker would never run it at all.
    let busy = workers.saturating_sub(1);
    for _ in 0..busy {
        let (stop, started) = (stop.clone(), started.clone());
        pool.spawn(async move {
            started.fetch_add(1, Ordering::SeqCst);
            let mut x = 1u64;
            while !stop.load(Ordering::Relaxed) {
                x = std::hint::black_box(mix(x));
            }
        })
        .detach();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while started.load(Ordering::SeqCst) < busy && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }

    let mut grid = StigGrid::<2>::new(w, h, floor(w, h).into_iter(), defs::<2>());
    grid.deposit(0, IVec2::new(40, 40), 50.0);
    grid.deposit(1, IVec2::new(130, 120), 50.0);
    let mut app = App::new();
    app.add_plugins(TaskPoolPlugin::default());
    app.insert_resource(BusyField {
        grid,
        tick,
        t: 0,
        us: Vec::with_capacity(ticks),
        parked: Some(0),
        preempted: Some(0),
        thread: String::new(),
    });
    app.add_systems(Update, busy_tick);
    for _ in 0..WARMUP + ticks {
        app.update();
    }
    stop.store(true, Ordering::Relaxed);

    let Some(field) = app.world_mut().remove_resource::<BusyField>() else {
        println!("stig: {case} lost its field resource");
        return;
    };
    let hash = grid_hash(&field.grid);
    let (median, p99, max) = stats(field.us);
    let count = |c: Option<usize>| c.map_or("n/a".to_string(), |c| c.to_string());
    println!(
        "stig: {case} grid={w}x{h} ch=2 threads=bevy{workers}_busy{busy} median_us={median:.1} p99_us={p99:.1} hash={hash:016x} max_us={max:.1} p99_over_median={:.2} parked_ticks={}/{ticks} preempted_ticks={}/{ticks} system_thread={}",
        p99 / median,
        count(field.parked),
        count(field.preempted),
        field.thread
    );
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

/// The fixed scenario with its stencil run in `threads` chunks on a rayon pool of `threads`.
fn determinism(threads: usize) {
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build();
    let Ok(pool) = pool else {
        println!("stig: determinism threads={threads} pool build failed");
        return;
    };
    let hash = pool.install(|| determinism_run(tick_rayon::<4>));
    line("determinism", 128, 128, 4, &format!("rayon{threads}"), vec![0.0], hash);
}

fn main() {
    // Bevy's own default pools, sized as `TaskPoolPlugin` sizes them, so every case shares one
    // `ComputeTaskPool` exactly as the systems of an app do.
    TaskPoolOptions::default().create_default_pools();
    if std::env::var("STIG_BENCH_ONLY").is_ok_and(|v| v == "busy_pool") {
        busy_pool("busy_pool", tick::<2>);
        busy_pool("busy_pool_with_bevy", tick_bevy::<2>);
        return;
    }
    let sizes: &[usize] = if quick() { &[64, 256] } else { &[64, 256, 1024] };
    for &s in sizes {
        throughput_all::<1>(s, s);
        throughput_all::<4>(s, s);
        throughput_all::<8>(s, s);
        rally(s, s);
    }
    busy_pool("busy_pool", tick::<2>);
    busy_pool("busy_pool_with_bevy", tick_bevy::<2>);
    line("determinism", 128, 128, 4, "caller", vec![0.0], determinism_run(tick::<4>));
    let bevy_threads = format!("bevy{}", ComputeTaskPool::get().thread_num());
    line("determinism", 128, 128, 4, &bevy_threads, vec![0.0], determinism_run(tick_bevy::<4>));
    let default = std::thread::available_parallelism().map_or(4, |n| n.get());
    for threads in [1, 2, default] {
        determinism(threads);
    }
}
