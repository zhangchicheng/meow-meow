//! Enemy waves and a simple keep-distance-and-shoot AI.

use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;
use rand::{Rng, seq::{IndexedRandom, SliceRandom}};

use crate::{
    GameState,
    actor::{Actor, AnimState, BODY_OFFSET, CHARACTER_ANCHOR, Dead, Health, Velocity, ysort_z},
    assets::GameAssets,
    combat::{Gun, GunSpec, Team, gun, spawn_fx, play_sfx},
    hud::Banner,
    map::{Level, Obstacle},
    pathfinding::FlowField,
    player::{Player, Score},
};

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::Playing), reset_waves)
        .add_systems(
            Update,
            (
                run_waves.before(crate::combat::CombatSystems).run_if(in_state(GameState::Playing).and_then(resource_exists::<Level>)),
                finish_spawns,
                think,
            ),
        )
        // Drain completion messages before a hurt actor can enter the death state.
        .add_systems(Update, remove_dead.before(crate::actor::start_dying))
        .add_systems(Update, play_death_sound.after(crate::actor::start_dying));
}

struct EnemyKind {
    hp: i32,
    speed: f32,
    /// Distance it tries to keep from the player.
    range: f32,
    gun: GunSpec,
    score: u32,
}

static KINDS: [EnemyKind; 4] = [
    EnemyKind {
        hp: 2,
        speed: 60.0,
        range: 80.0,
        gun: GunSpec {
            palette: Some(0),
            hold: Vec2::new(0.0, 7.0),
            reach: 10.0,
            slice: "gun5",
            // Gun sheet (117, 76), relative to slice origin (94, 71) and grip (7, 7).
            muzzle: Vec2::new(16.0, 2.0),
            bullet: "bullet2_2_2",
            flash: "3Flash",
            // Align flash pixel (20, 24); flash frames are 48x48, centered at (24, 24).
            flash_offset: Vec2::new(4.0, 0.0),
            hit: "2Hit",
            cooldown: 1.4,
            speed: 120.0,
            damage: 2,
            pellets: 1,
            spread: 0.1,
            magazine: 4,
            reload_time: 1.6,
            pitch: 1.15,
        },
        score: 100,
    },
    EnemyKind {
        hp: 2,
        speed: 60.0,
        range: 40.0,
        gun: GunSpec {
            palette: Some(1),
            hold: Vec2::new(0.0, 7.0),
            reach: 11.0,
            slice: "gun6",
            // Gun sheet (66, 90), relative to slice origin (42, 86) and grip (7, 6).
            muzzle: Vec2::new(17.0, 2.0),
            bullet: "bullet2_2_2",
            flash: "5Flash",
            // Align flash pixel (18, 24).
            flash_offset: Vec2::new(6.0, 0.0),
            hit: "3Hit",
            cooldown: 0.4,
            speed: 110.0,
            damage: 1,
            pellets: 1,
            spread: 0.3,
            magazine: 8,
            reload_time: 1.5,
            pitch: 0.85,
        },
        score: 150,
    },
    EnemyKind {
        hp: 2,
        speed: crate::player::RUN_SPEED,
        range: 75.0,
        gun: GunSpec {
            palette: Some(2),
            hold: Vec2::new(0.0, 7.0),
            reach: 11.0,
            slice: "gun0",
            // Gun sheet (67, 40), relative to slice origin (39, 37) and grip (8, 7).
            muzzle: Vec2::new(20.0, 4.0),
            bullet: "bullet2_2_2",
            flash: "1Flash",
            // Align flash pixel (19, 24).
            flash_offset: Vec2::new(5.0, 0.0),
            hit: "2Hit",
            cooldown: 0.55,
            speed: 130.0,
            damage: 1,
            pellets: 1,
            spread: 0.12,
            magazine: 8,
            reload_time: 1.8,
            pitch: 1.2,
        },
        score: 200,
    },
    EnemyKind {
        hp: 4,
        speed: 55.0,
        range: 110.0,
        gun: GunSpec {
            palette: Some(3),
            hold: Vec2::new(0.0, 7.0),
            reach: 14.0,
            slice: "gun7",
            // Gun sheet (120, 90), relative to slice origin (93, 88) and grip (8, 7).
            muzzle: Vec2::new(19.0, 5.0),
            bullet: "bullet2_2_2",
            flash: "6Flash",
            // Align flash pixel (21, 24).
            flash_offset: Vec2::new(3.0, 0.0),
            hit: "4Hit",
            cooldown: 0.9,
            speed: 140.0,
            damage: 1,
            pellets: 1,
            spread: 0.05,
            magazine: 6,
            reload_time: 2.2,
            pitch: 0.75,
        },
        score: 300,
    },
];

#[derive(Component)]
pub struct Enemy {
    kind: usize,
    /// +1 / -1: which way to circle around the player.
    strafe: f32,
    retarget: Timer,
}

/// Spawn effect is playing; the enemy appears when the timer ends.
#[derive(Component)]
struct PendingSpawn {
    timer: Timer,
    kind: usize,
    pos: Vec2,
}

#[derive(Resource)]
struct Waves {
    wave: u32,
    remaining: u32,
    timer: Timer,
    palette_order: [usize; 4],
    palette_cursor: usize,
}

impl Waves {
    /// Use every costume once before reshuffling, so small waves have variety.
    fn next_character2_palette(&mut self, rng: &mut impl Rng) -> usize {
        if self.palette_cursor == self.palette_order.len() {
            self.palette_order.shuffle(rng);
            self.palette_cursor = 0;
        }
        let palette = self.palette_order[self.palette_cursor];
        self.palette_cursor += 1;
        palette
    }
}

fn reset_waves(mut commands: Commands) {
    commands.insert_resource(Waves {
        wave: 0,
        remaining: 0,
        timer: Timer::from_seconds(1.5, TimerMode::Once),
        palette_order: [0, 1, 2, 3],
        palette_cursor: 4,
    });
}

fn run_waves(
    time: Res<Time>,
    mut commands: Commands,
    mut waves: ResMut<Waves>,
    mut banner: ResMut<Banner>,
    level: Res<Level>,
    assets: Res<GameAssets>,
    enemies: Query<(), Or<(With<Enemy>, With<PendingSpawn>)>>,
    player: Query<&Transform, With<Player>>,
    pickups: Query<Entity, With<crate::map::Pickup>>,
    obstacles: Query<&Obstacle>,
    portals: Query<&crate::portal::Portal>,
    server: Res<AssetServer>,
    barrels: Query<(Entity, &crate::map::BarrelHome)>,
) {
    waves.timer.tick(time.delta());
    if !waves.timer.is_finished() {
        return;
    }
    let player_pos = player.single().map_or(level.player_spawn, |t| t.translation.truncate());
    if waves.remaining == 0 {
        if !enemies.is_empty() {
            return;
        }
        waves.wave += 1;
        crate::map::reset_barrels(&mut commands, &barrels);
        crate::pickups::refresh(&mut commands, &server, &level, player_pos, &pickups, &obstacles, &portals, &barrels);
        waves.remaining = 2 + waves.wave * 2;
        // Start a fresh shuffled color cycle for each wave.
        waves.palette_cursor = 4;
        banner.show(format!("WAVE {}", waves.wave));
        waves.timer = Timer::from_seconds(2.0, TimerMode::Once);
        if waves.wave > 1 {
            // Celebrate clearing a wave.
            let fx = Transform::from_translation((player_pos + Vec2::new(0.0, 14.0)).extend(20.0));
            spawn_fx(&mut commands, &assets.effects2, "15_Level Up", fx);
        }
        return;
    }

    let mut rng = rand::rng();
    let far: Vec<Vec2> = level.enemy_spawns.iter().copied().filter(|p| p.distance(player_pos) > 140.0).collect();
    let Some(&pos) = far.choose(&mut rng).or_else(|| level.enemy_spawns.choose(&mut rng)) else { return };
    let kind = waves.next_character2_palette(&mut rng);
    spawn_fx(&mut commands, &assets.effects1, "1_Spawn", Transform::from_translation((pos + Vec2::new(0.0, 12.0)).extend(20.0)));
    commands.spawn((PendingSpawn { timer: Timer::from_seconds(0.5, TimerMode::Once), kind, pos }, DespawnOnEnter(GameState::Restart)));
    waves.remaining -= 1;
    waves.timer = Timer::from_seconds(rng.random_range(0.4..1.0), TimerMode::Once);
}

fn finish_spawns(
    time: Res<Time>,
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut pending: Query<(Entity, &mut PendingSpawn)>,
) {
    let mut rng = rand::rng();
    for (entity, mut spawn) in &mut pending {
        if !spawn.timer.tick(time.delta()).is_finished() {
            continue;
        }
        commands.entity(entity).despawn();
        let kind = &KINDS[spawn.kind];
        let pos = spawn.pos;
        let aseprite = assets.character2_variants[spawn.kind].clone();
        commands
            .spawn((
                Name::new("Enemy"),
                Enemy {
                    kind: spawn.kind,
                    strafe: if rng.random_bool(0.5) { 1.0 } else { -1.0 },
                    retarget: Timer::from_seconds(rng.random_range(1.5..3.0), TimerMode::Repeating),
                },
                Actor::new("2"),
                Health::new(kind.hp),
                AseAnimation {
                    aseprite,
                    animation: Animation::tag("2Idle"),
                },
                Sprite::default(),
                CHARACTER_ANCHOR,
                Transform::from_translation(pos.extend(ysort_z(pos.y))),
                DespawnOnEnter(GameState::Restart),
            ))
            .with_child(gun(&assets, &kind.gun, Team::Enemy, rng.random_range(0.5..1.5)));
    }
}

pub(crate) fn think(
    time: Res<Time>,
    level: Option<Res<Level>>,
    flow: Res<FlowField>,
    obstacles: Query<&Obstacle>,
    mut players: Query<(&Transform, &mut Actor, &mut Health, Has<Dead>), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(&mut Enemy, &mut Actor, &mut Velocity, &Transform, &Children), (Without<Dead>, Without<Player>)>,
    mut guns: Query<&mut Gun>,
    mut contact_cooldown: Local<f32>,
) {
    let Some(level) = level else { return };
    let Ok((player_tf, mut player_actor, mut player_health, player_dead)) = players.single_mut() else { return };
    let player_pos = player_tf.translation.truncate();
    *contact_cooldown -= time.delta_secs();

    let obstacles: Vec<&Obstacle> = obstacles.iter().collect();
    let positions: Vec<Vec2> = enemies.iter().map(|(_, _, _, t, _)| t.translation.truncate()).collect();
    let mut rng = rand::rng();

    for (mut enemy, mut actor, mut velocity, transform, children) in &mut enemies {
        let kind = &KINDS[enemy.kind];
        let pos = transform.translation.truncate();
        let to_player = player_pos - pos;
        let dist = to_player.length();
        let toward = to_player.normalize_or_zero();
        let sees = !player_dead && level.line_of_sight(&obstacles, pos + BODY_OFFSET, player_pos + BODY_OFFSET);

        if enemy.retarget.tick(time.delta()).just_finished() && rng.random_bool(0.4) {
            enemy.strafe = -enemy.strafe;
        }

        let mut dir = if player_dead {
            Vec2::ZERO
        } else if !sees || dist > kind.range + 15.0 {
            flow.direction(&level, pos).unwrap_or(toward)
        } else if dist < kind.range - 20.0 {
            -toward
        } else {
            toward.perp() * enemy.strafe
        };
        // Keep some room between enemies.
        for other in &positions {
            let away = pos - *other;
            let d = away.length();
            if d > 0.01 && d < 16.0 {
                dir += away / d * (16.0 - d) / 8.0;
            }
        }
        let speed = if actor.is_hurt() { kind.speed * 0.3 } else { kind.speed };
        velocity.0 = dir.normalize_or_zero() * speed;

        if !player_dead && toward != Vec2::ZERO {
            actor.aim = toward;
        }
        actor.state = if actor.is_hurt() {
            AnimState::Hurt
        } else if velocity.0.length_squared() > 1.0 {
            AnimState::Run
        } else {
            AnimState::Idle
        };

        for child in children {
            if let Ok(mut gun) = guns.get_mut(*child) {
                gun.trigger = sees && dist < 220.0;
            }
        }

        // Touching the player hurts too.
        if !player_dead && dist < 10.0 && *contact_cooldown <= 0.0 && player_actor.state != AnimState::Roll {
            player_actor.take_hit(&mut player_health, 1);
            *contact_cooldown = 0.8;
        }
    }
}

fn play_death_sound(
    mut commands: Commands,
    assets: Res<GameAssets>,
    enemies: Query<(), (With<Enemy>, Added<Dead>)>,
) {
    for _ in &enemies {
        play_sfx(&mut commands, &assets.meow, 0.6, 1.0);
    }
}

fn remove_dead(
    mut commands: Commands,
    mut events: MessageReader<AnimationEvents>,
    dead: Query<(&Enemy, &AseAnimation), With<Dead>>,
    mut score: ResMut<Score>,
) {
    for event in events.read() {
        if let AnimationEvents::Finished(entity) = event
            && let Ok((enemy, animation)) = dead.get(*entity)
            && animation.animation.tag.as_deref() == Some("2Death")
        {
            score.0 += KINDS[enemy.kind].score;
            commands.entity(*entity).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    #[test]
    fn specified_flash_and_gun_pixels_overlap_when_rotated_and_mirrored() {
        // Source sheet coordinates, including the existing slice origin and grip pivot.
        let references = [
            ("gun5", "3Flash", Vec2::new(117.0, 76.0), Vec2::new(94.0, 71.0), Vec2::new(7.0, 7.0), Vec2::new(20.0, 24.0)),
            ("gun6", "5Flash", Vec2::new(66.0, 90.0), Vec2::new(42.0, 86.0), Vec2::new(7.0, 6.0), Vec2::new(18.0, 24.0)),
            ("gun0", "1Flash", Vec2::new(67.0, 40.0), Vec2::new(39.0, 37.0), Vec2::new(8.0, 7.0), Vec2::new(19.0, 24.0)),
            ("gun7", "6Flash", Vec2::new(120.0, 90.0), Vec2::new(93.0, 88.0), Vec2::new(8.0, 7.0), Vec2::new(21.0, 24.0)),
        ];
        for (kind, (slice, tag, gun_point, origin, grip, flash_point)) in KINDS.iter().zip(references) {
            let spec = &kind.gun;
            assert_eq!(spec.slice, slice);
            assert_eq!(spec.flash, tag);
            let gun_local = (gun_point - origin - grip) * Vec2::new(1.0, -1.0);
            assert_eq!(spec.muzzle_position().truncate(), gun_local);
            let flash_local = spec.flash_position().truncate()
                + (flash_point - Vec2::splat(24.0)) * Vec2::new(1.0, -1.0);
            for flip in [1.0, -1.0] {
                for angle in [0.0, 0.7, std::f32::consts::PI, -1.3] {
                    let transform = Mat4::from_scale_rotation_translation(
                        Vec3::new(1.0, flip, 1.0), Quat::from_rotation_z(angle), Vec3::new(50.0, -30.0, 10.0),
                    );
                    let gun_world = transform.transform_point3(gun_local.extend(0.0));
                    let flash_world = transform.transform_point3(flash_local.extend(0.0));
                    assert!(gun_world.distance(flash_world) < 0.0001);
                }
            }
        }
    }

    #[test]
    fn hurt_completion_does_not_skip_death_animation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Aseprite>>()
            .init_resource::<Score>()
            .add_message::<AnimationEvents>()
            .add_plugins(crate::actor::plugin)
            .add_systems(Update, remove_dead.before(crate::actor::start_dying));
        let mut actor = Actor::new("2");
        actor.state = AnimState::Hurt;
        let enemy = app.world_mut().spawn((
            Enemy { kind: 0, strafe: 1.0, retarget: Timer::from_seconds(2.0, TimerMode::Repeating) },
            actor, Health::new(0), Sprite::default(), Transform::default(),
            AseAnimation {
                aseprite: Handle::default(),
                animation: Animation::tag("2Hurt").with_repeat(AnimationRepeat::Count(0)),
            },
        )).id();
        app.world_mut().write_message(AnimationEvents::Finished(enemy));
        app.update();
        assert!(app.world().get::<Dead>(enemy).is_some());
        let animation = &app.world().get::<AseAnimation>(enemy).unwrap().animation;
        assert_eq!(animation.tag.as_deref(), Some("2Death"));
        assert!(matches!(animation.repeat, AnimationRepeat::Count(0)));
        assert_eq!(app.world().resource::<Score>().0, 0);
        app.update();
        assert!(app.world().get::<Enemy>(enemy).is_some());
        // Only completion of the death animation removes the actor and awards points.
        app.world_mut().write_message(AnimationEvents::Finished(enemy));
        app.update();
        assert!(app.world().get::<Enemy>(enemy).is_none());
        assert_eq!(app.world().resource::<Score>().0, 100);
        app.world_mut().write_message(AnimationEvents::Finished(enemy));
        app.update();
        assert_eq!(app.world().resource::<Score>().0, 100);
    }

    #[test]
    fn character2_colors_cover_every_variant_before_repeating() {
        for seed in 0..32 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut waves = Waves {
                wave: 1, remaining: 4,
                timer: Timer::from_seconds(1.0, TimerMode::Once),
                palette_order: [0, 1, 2, 3], palette_cursor: 4,
            };
            for _ in 0..8 {
                let mut colors = std::array::from_fn::<_, 4, _>(|_| waves.next_character2_palette(&mut rng));
                colors.sort_unstable();
                assert_eq!(colors, [0, 1, 2, 3]);
            }
        }
    }
}
