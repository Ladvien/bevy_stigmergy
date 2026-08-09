//! **A path nobody planned.**
//!
//! Ninety foragers, a nest, a food source, and a wall with two gaps in it. No agent has a map, a
//! route, or any idea where the gaps are. Each one reads three numbers at the cell it is standing on
//! and steers by the gradient — and within a few seconds a bright trail is running from the nest,
//! through a gap, to the food, and back.
//!
//! That is stigmergy: the coordination is in the environment, not in the agents. They are not
//! communicating. They are editing a shared field and reacting to it, which is the mechanism Grassé
//! named for termites in 1959 and the one ant colony optimisation formalised.
//!
//! Three channels, each with its own evaporation, diffusion and deposit radius:
//!
//! - **HOME** is emitted at the nest and diffuses widely. A laden forager climbs it to get back.
//! - **FOOD** is emitted at the food and does the same in reverse.
//! - **TRAIL** is laid *only by foragers carrying food*, and evaporates fastest. Nothing seeds it and
//!   nothing maintains it except success, which is exactly why it means anything.
//!
//! **Watch what evaporation is for.** It is not decay for its own sake — it is how the colony forgets
//! a route that stopped paying. Early on you will see trail smeared toward both gaps; the one that
//! keeps delivering gets reinforced faster than it fades, and the other quietly disappears. Turn
//! evaporation off and the first accident the colony has becomes permanent.
//!
//! Note also that the field routes *around* the wall on its own. Diffusion only moves value between
//! floor cells, so the gradient bends through the gaps without anything here computing a path.
//!
//! This is the only example here that needs a GPU; `trail` and `rally` print to a terminal.
//!
//! Run: `cargo run -p bevy_stigmergy --example foraging`

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy_stigmergy::{ChannelDef, StigGrid};

const W: usize = 84;
const H: usize = 54;
/// Screen pixels per cell.
const CELL: f32 = 11.0;

const HOME: usize = 0;
const FOOD: usize = 1;
const TRAIL: usize = 2;

const NEST: IVec2 = IVec2::new(24, 46);
const SOURCE: IVec2 = IVec2::new(24, 6);

const AGENTS: usize = 90;
const SPEED: f32 = 14.0;
/// Fixed step, so the picture is the same on any monitor.
const DT: f32 = 1.0 / 60.0;

/// A wall across the middle with two gaps. Nothing tells the foragers where the gaps are.
fn is_wall(c: IVec2) -> bool {
    let (x, y) = (c.x, c.y);
    if x <= 0 || y <= 0 || x >= W as i32 - 1 || y >= H as i32 - 1 {
        return true;
    }
    if y == 26 && !(18..=29).contains(&x) && !(56..=67).contains(&x) {
        return true;
    }
    false
}

/// Ascending row-major and free of repeats, which is what `StigGrid::new` asks for.
fn floor_cells() -> impl Iterator<Item = IVec2> {
    (0..H as i32)
        .flat_map(|y| (0..W as i32).map(move |x| IVec2::new(x, y)))
        .filter(|c| !is_wall(*c))
}

fn world_of(p: Vec2) -> Vec3 {
    Vec3::new(
        (p.x - W as f32 * 0.5 + 0.5) * CELL,
        (H as f32 * 0.5 - p.y - 0.5) * CELL,
        1.0,
    )
}

#[derive(Resource)]
struct Field(StigGrid<3>);

#[derive(Component)]
struct Tile(IVec2);

#[derive(Component)]
struct Forager {
    pos: Vec2,
    dir: Vec2,
    laden: bool,
    /// Per-agent wander phase, so they do not all jitter in lockstep.
    seed: f32,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_stigmergy — a path nobody planned".into(),
                // 0.19 takes physical pixels as `u32` — there is no `(f32, f32)` conversion.
                resolution: ((W as f32 * CELL) as u32, (H as f32 * CELL) as u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (step_field, paint_tiles).chain())
        .run();
}

fn disc_image(px: u32) -> Image {
    let mut data = vec![0u8; (px * px * 4) as usize];
    let c = px as f32 * 0.5;
    for y in 0..px {
        for x in 0..px {
            let d = Vec2::new(x as f32 + 0.5 - c, y as f32 + 0.5 - c).length();
            let a = ((c - d).clamp(0.0, 1.0) * 255.0) as u8;
            let i = ((y * px + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = a;
        }
    }
    Image::new(
        Extent3d { width: px, height: px, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    )
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn(Camera2d);
    spawn_legend(&mut commands);

    // Each channel's personality is entirely in these three numbers.
    let grid = StigGrid::<3>::new(
        W,
        H,
        floor_cells(),
        [
            // HOME: a long-lived beacon. Slow to fade, spreads far, so it is readable anywhere.
            ChannelDef { evaporate: 0.06, diffuse: 0.34, deposit_radius: 2.0 },
            // FOOD: the same, from the other end.
            ChannelDef { evaporate: 0.06, diffuse: 0.34, deposit_radius: 2.0 },
            // TRAIL: short-lived and tight. It has to fade fast enough that a route which stops
            // paying disappears, and stay narrow enough to read as a line rather than a fog.
            ChannelDef { evaporate: 0.55, diffuse: 0.06, deposit_radius: 1.1 },
        ],
    );
    commands.insert_resource(Field(grid));

    for c in (0..H as i32).flat_map(|y| (0..W as i32).map(move |x| IVec2::new(x, y))) {
        commands.spawn((
            Tile(c),
            Sprite { color: Color::BLACK, custom_size: Some(Vec2::splat(CELL)), ..default() },
            Transform::from_translation(world_of(Vec2::new(c.x as f32, c.y as f32)).with_z(0.0)),
        ));
    }

    let disc = images.add(disc_image(48));
    for i in 0..AGENTS {
        let t = i as f32;
        commands.spawn((
            Forager {
                pos: NEST.as_vec2(),
                dir: Vec2::new((t * 1.7).cos(), (t * 1.7).sin()).normalize_or_zero(),
                laden: false,
                seed: t * 0.618,
            },
            Sprite {
                image: disc.clone(),
                color: Color::srgb(0.85, 0.88, 0.95),
                custom_size: Some(Vec2::splat(CELL * 0.7)),
                ..default()
            },
            Transform::from_translation(world_of(NEST.as_vec2())),
        ));
    }
}

/// Deposit, decide, move, then let the field evaporate and diffuse.
fn step_field(
    time: Res<Time>,
    mut field: ResMut<Field>,
    mut foragers: Query<(&mut Forager, &mut Transform, &mut Sprite)>,
) {
    let t = time.elapsed_secs();

    // The two constant sources. Everything else in the field is laid by the foragers.
    field.0.deposit(HOME, NEST, 90.0 * DT);
    field.0.deposit(FOOD, SOURCE, 90.0 * DT);

    for (mut f, mut tf, mut sprite) in &mut foragers {
        let cell = IVec2::new(f.pos.x.round() as i32, f.pos.y.round() as i32);

        // A laden forager is the only thing that writes TRAIL. That is the whole reinforcement rule:
        // the field remembers routes that worked, in proportion to how many succeeded on them.
        if f.laden {
            field.0.deposit(TRAIL, cell, 9.0 * DT);
        }

        // What it is climbing depends only on whether its hands are full.
        let (beacon, target) = if f.laden { (HOME, NEST) } else { (FOOD, SOURCE) };
        let mut steer = field.0.gradient_cell(beacon, cell) * 55.0;
        // Outbound foragers also follow the trail, which is what turns one lucky route into a road.
        if !f.laden {
            steer += field.0.gradient_cell(TRAIL, cell) * 30.0;
        }

        // Wander, so the colony keeps exploring instead of committing to its first idea. Without it
        // nothing new is ever found and the trail cannot move when the food does.
        //
        // **Scale it against the gradients it competes with, not by taste.** Measured here, a beacon
        // gradient averages ~0.09 per cell; at the first wander weight tried (2.2) the noise term was
        // stronger than the homing signal, so laden foragers wandered the whole field laying trail
        // everywhere and the "path" was a wash. It has to stay well under `beacon_weight * |grad|`.
        let w = (t * 1.9 + f.seed * 6.28).sin() * 0.9 + (t * 0.7 + f.seed * 3.1).cos() * 0.6;
        let perp = Vec2::new(-f.dir.y, f.dir.x);
        steer += perp * w * 0.8;

        let want = (f.dir + steer * DT).normalize_or_zero();
        if want != Vec2::ZERO {
            f.dir = want;
        }

        // Walls are the caller's business — the field only ever spread between floor cells.
        let step = f.dir * SPEED * DT;
        let nx = Vec2::new(f.pos.x + step.x, f.pos.y);
        if !is_wall(IVec2::new(nx.x.round() as i32, nx.y.round() as i32)) {
            f.pos.x = nx.x;
        } else {
            f.dir.x = -f.dir.x;
        }
        let ny = Vec2::new(f.pos.x, f.pos.y + step.y);
        if !is_wall(IVec2::new(ny.x.round() as i32, ny.y.round() as i32)) {
            f.pos.y = ny.y;
        } else {
            f.dir.y = -f.dir.y;
        }

        if f.pos.distance(target.as_vec2()) < 2.0 {
            f.laden = !f.laden;
            f.dir = -f.dir;
            sprite.color = if f.laden {
                Color::srgb(1.0, 0.78, 0.32)
            } else {
                Color::srgb(0.85, 0.88, 0.95)
            };
        }

        tf.translation = world_of(f.pos);
    }

    field.0.evaporate_diffuse(DT);

}

fn paint_tiles(field: Res<Field>, mut tiles: Query<(&Tile, &mut Sprite)>) {
    for (tile, mut sprite) in &mut tiles {
        if is_wall(tile.0) {
            sprite.color = Color::srgb(0.11, 0.12, 0.16);
            continue;
        }
        let home = field.0.sample_cell(HOME, tile.0);
        let food = field.0.sample_cell(FOOD, tile.0);
        let trail = field.0.sample_cell(TRAIL, tile.0);
        // The beacons stay dim and cool on purpose: they are scaffolding, and the emergent thing is
        // the trail. Gamma-lifted so the faint tail a forager is actually steering on is visible.
        let b = (home * 0.18).clamp(0.0, 1.0).powf(0.6) * 0.30;
        let f = (food * 0.18).clamp(0.0, 1.0).powf(0.6) * 0.30;
        let tr = (trail * 0.30).clamp(0.0, 1.0).powf(0.55);
        sprite.color = Color::srgb(
            0.045 + tr * 0.95 + f * 0.10,
            0.055 + tr * 0.70 + b * 0.30 + f * 0.45,
            0.075 + tr * 0.20 + b * 0.60 + f * 0.55,
        );
    }
}

/// ASCII only, deliberately: Bevy's embedded default font carries 95 codepoints, so an arrow or a
/// middle dot renders as nothing at all and the label silently loses a word.
fn spawn_legend(commands: &mut Commands) {
    let lines = [
        "bevy_stigmergy - coordination stored in the environment",
        "90 foragers, no map, no route, no messages",
        "each reads 3 numbers at its own cell and steers by the gradient",
        "pale = searching, amber = carrying food (and laying TRAIL)",
        "the bright path is emergent; evaporation is what deletes the bad ones",
    ];
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|p| {
            for (i, line) in lines.into_iter().enumerate() {
                p.spawn((
                    Text::new(line),
                    // 0.19 makes `font_size` a `FontSize` enum rather than a bare f32.
                    TextFont { font_size: FontSize::Px(if i == 0 { 14.0 } else { 12.0 }), ..default() },
                    TextColor(if i == 0 {
                        Color::srgb(1.0, 0.93, 0.78)
                    } else {
                        Color::srgb(0.84, 0.86, 0.92)
                    }),
                ));
            }
        });
}
