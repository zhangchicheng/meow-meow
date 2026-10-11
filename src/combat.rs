//! Guns, bullets, explosive barrels and one-shot effects.

use bevy::{audio::Volume, prelude::*, sprite::Anchor};
use bevy_aseprite_ultra::prelude::*;
use rand::Rng;

use crate::{
    GameState,
    actor::{Actor, AnimState, BODY_OFFSET, Dead, Health},
    assets::GameAssets,
    camera::Shake,
    map::{BLOCK_SHOT, BULLET_MARGIN, Explosive, Level, Obstacle},
    player::Player,
};

pub fn plugin(app: &mut App) {
    app.add_message::<Explosion>().add_systems(
        Update,
        (
            aim_guns,
            fire_guns.run_if(in_state(GameState::Playing).or_else(in_state(GameState::GameOver))),
            move_bullets,
            explode_barrels,
            apply_explosions,
            despawn_finished_fx,
        )
            .chain()
            .in_set(CombatSystems),
    );
}

const FX_Z: f32 = 20.0;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CombatSystems;
const BULLET_Z: f32 = 15.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Team {
    Player,
    Enemy,
}

/// Static description of a weapon.
pub struct GunSpec {
    pub palette: Option<usize>,
    /// Centre the gun circles around, relative to the character's feet (the actor origin), in pixels.
    /// y is up; x points towards the facing direction and is mirrored when facing left,
    /// so keep it at 0 for a seamless circle (any other value makes the gun jump when
    /// the aim crosses straight up / down).
    pub hold: Vec2,
    /// Radius of that circle: the grip sits `reach` px from `hold` along the aim direction.
    pub reach: f32,
    /// Slice name in guns.aseprite.
    pub slice: &'static str,
    /// Barrel point relative to the grip (slice pivot), in gun-local pixels (y up).
    pub muzzle: Vec2,
    /// Slice name in bullets.aseprite.
    pub bullet: &'static str,
    /// Tags in muzzle_flash.aseprite / bullet_hit.aseprite.
    pub flash: &'static str,
    /// Centre of the muzzle flash relative to the barrel's end (x: forward along the barrel, y: up).
    pub flash_offset: Vec2,
    pub hit: &'static str,
    pub cooldown: f32,
    pub speed: f32,
    pub damage: i32,
    pub pellets: u32,
    pub spread: f32,
    /// Shots per magazine and seconds to reload it.
    pub magazine: u32,
    pub reload_time: f32,
    /// Playback speed of the shot sound: < 1 sounds heavier, > 1 lighter.
    pub pitch: f32,
}

impl GunSpec {
    pub fn muzzle_position(&self) -> Vec3 {
        self.muzzle.extend(0.0)
    }

    pub fn flash_position(&self) -> Vec3 {
        (self.muzzle + self.flash_offset).extend(0.02)
    }
}

pub static PLAYER_GUN: GunSpec = GunSpec {
    palette: None,
    hold: Vec2::new(0.0, 7.0),
    reach: 12.0,
    slice: "gun7",
    muzzle: Vec2::new(19.0, 1.0),
    bullet: "bullet2_0_1",
    flash: "1Flash",
    flash_offset: Vec2::new(4.0, 4.0),
    hit: "1Hit",
    cooldown: 0.13,
    speed: 280.0,
    damage: 1,
    pellets: 1,
    spread: 0.05,
    magazine: 30,
    reload_time: 1.1,
    pitch: 1.0,
};

#[derive(Component)]
pub struct Gun {
    pub spec: &'static GunSpec,
    pub team: Team,
    pub cooldown: Timer,
    /// Set by the owner's controller every frame it wants to shoot.
    pub trigger: bool,
    /// Set by the owner's controller to reload before the magazine is empty.
    pub reload_request: bool,
    pub ammo: u32,
    /// Running while reloading.
    pub reload: Timer,
}

impl Gun {
    pub fn is_reloading(&self) -> bool {
        !self.reload.is_finished()
    }
}

/// Gun entity to add as a child of an actor.
pub fn gun(assets: &GameAssets, spec: &'static GunSpec, team: Team, initial_delay: f32) -> impl Bundle {
    let mut cooldown = Timer::from_seconds(spec.cooldown, TimerMode::Once);
    cooldown.set_elapsed(std::time::Duration::from_secs_f32((spec.cooldown - initial_delay).max(0.0)));
    let mut reload = Timer::from_seconds(spec.reload_time, TimerMode::Once);
    reload.finish();
    (
        Gun { spec, team, cooldown, trigger: false, reload_request: false, ammo: spec.magazine, reload },
        AseSlice {
            name: spec.slice.into(),
            aseprite: spec.palette.map_or_else(|| assets.guns.clone(), |i| assets.enemy_guns[i].clone()),
        },
        Sprite::default(),
        // Replaced by the slice pivot set in aseprite.
        Anchor::default(),
        Transform::from_translation(spec.hold.extend(0.01)),
    )
}

#[derive(Component)]
struct Bullet {
    velocity: Vec2,
    damage: i32,
    team: Team,
    life: f32,
    hit: &'static str,
}

/// Effect that despawns itself once its animation has played.
#[derive(Component)]
pub struct OneShotFx;

pub fn spawn_fx(commands: &mut Commands, aseprite: &Handle<Aseprite>, tag: &str, transform: Transform) -> Entity {
    commands
        .spawn((
            OneShotFx,
            AseAnimation {
                aseprite: aseprite.clone(),
                // `Count(n)` = n extra repeats after the first play, so 0 plays it once.
                animation: Animation::tag(tag).with_repeat(AnimationRepeat::Count(0)),
            },
            Sprite::default(),
            transform,
            DespawnOnEnter(GameState::Restart),
        ))
        .id()
}

#[derive(Message)]
struct Explosion {
    pos: Vec2,
}

fn aim_guns(actors: Query<&Actor>, mut guns: Query<(&Gun, &ChildOf, &mut Transform, &mut Visibility)>) {
    for (gun, child_of, mut transform, mut visibility) in &mut guns {
        let Ok(actor) = actors.get(child_of.parent()) else { continue };
        let aim = actor.aim;
        let left = aim.x < 0.0;
        // Tip the barrel down and back up while reloading.
        let dip = if gun.is_reloading() {
            let t = gun.reload.fraction();
            -0.9 * (t * std::f32::consts::PI).sin() * if left { -1.0 } else { 1.0 }
        } else {
            0.0
        };
        transform.rotation = Quat::from_rotation_z(aim.to_angle() + dip);
        // Mirror instead of rotating upside down when aiming to the left.
        transform.scale.y = if left { -1.0 } else { 1.0 };
        let spec = gun.spec;
        let center = Vec2::new(if left { -spec.hold.x } else { spec.hold.x }, spec.hold.y);
        // Plain circle around the body, like the reference animation.
        let grip = center + aim * spec.reach;
        transform.translation.x = grip.x;
        transform.translation.y = grip.y;
        // Hold the gun behind the body when aiming up.
        transform.translation.z = if aim.y > 0.6 { -0.01 } else { 0.01 };
        *visibility = match actor.state {
            AnimState::Roll | AnimState::Death => Visibility::Hidden,
            _ => Visibility::Inherited,
        };
    }
}

fn fire_guns(
    time: Res<Time>,
    assets: Res<GameAssets>,
    mut commands: Commands,
    actors: Query<&Actor, Without<Dead>>,
    players: Query<&Transform, With<Player>>,
    mut guns: Query<(Entity, &mut Gun, &GlobalTransform, &ChildOf)>,
) {
    let mut rng = rand::rng();
    let listener = players.single().ok().map(|t| t.translation.truncate());
    for (entity, mut gun, global, child_of) in &mut guns {
        gun.cooldown.tick(time.delta());
        if gun.reload.tick(time.delta()).just_finished() {
            gun.ammo = gun.spec.magazine;
        }
        let trigger = std::mem::take(&mut gun.trigger);
        let reload_request = std::mem::take(&mut gun.reload_request);
        let Ok(actor) = actors.get(child_of.parent()) else { continue };
        let pos = global.translation().truncate();
        // Enemy sounds fade out with distance from the player.
        let loudness = match gun.team {
            Team::Player => 1.0,
            Team::Enemy => listener.map_or(0.0, |l| (1.0 - l.distance(pos) / 320.0).clamp(0.0, 1.0).powi(2) * 0.7),
        };

        let wants_reload = reload_request && gun.ammo < gun.spec.magazine || trigger && gun.ammo == 0;
        if wants_reload && !gun.is_reloading() && actor.state != AnimState::Roll {
            gun.reload.reset();
            play_sfx(&mut commands, &assets.reload, 0.7 * loudness, 1.0);
            continue;
        }
        if !trigger || gun.is_reloading() || !gun.cooldown.is_finished() || actor.state == AnimState::Roll {
            continue;
        }
        gun.cooldown.reset();
        gun.ammo -= 1;
        let spec = gun.spec;
        let muzzle = global.transform_point(spec.muzzle_position()).truncate();
        for i in 0..spec.pellets {
            let fan = if spec.pellets > 1 {
                (i as f32 / (spec.pellets - 1) as f32 - 0.5) * spec.spread * 2.0
            } else {
                0.0
            };
            let angle = actor.aim.to_angle() + fan + rng.random_range(-spec.spread..=spec.spread) * 0.5;
            let dir = Vec2::from_angle(angle);
            commands.spawn((
                Bullet { velocity: dir * spec.speed, damage: spec.damage, team: gun.team, life: 1.6, hit: spec.hit },
                AseSlice {
                    name: spec.bullet.into(),
                    aseprite: spec.palette.map_or_else(|| assets.bullets.clone(), |i| assets.enemy_bullets[i].clone()),
                },
                Sprite::default(),
                Transform::from_translation(muzzle.extend(BULLET_Z)).with_rotation(Quat::from_rotation_z(angle)),
                DespawnOnEnter(GameState::Restart),
            ));
        }
        // Slight random pitch so rapid fire doesn't sound like a loop.
        play_sfx(&mut commands, &assets.shot, 0.45 * loudness, spec.pitch * rng.random_range(0.93..1.07));
        // Flash attached to the gun so it follows the barrel.
        let flash = spawn_fx(
            &mut commands,
            &assets.muzzle_flash,
            spec.flash,
            Transform::from_translation(spec.flash_position()),
        );
        commands.entity(entity).add_child(flash);
        // Auto-reload once the magazine is empty.
        if gun.ammo == 0 {
            gun.reload.reset();
            play_sfx(&mut commands, &assets.reload, 0.7 * loudness, 1.0);
        }
    }
}

/// Fire-and-forget sound effect (the global volume from Setting is applied by Bevy).
pub fn play_sfx(commands: &mut Commands, sound: &Handle<AudioSource>, volume: f32, speed: f32) {
    if volume <= 0.01 {
        return;
    }
    commands.spawn((
        AudioPlayer(sound.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)).with_speed(speed),
    ));
}

fn move_bullets(
    time: Res<Time>,
    level: Option<Res<Level>>,
    assets: Res<GameAssets>,
    mut commands: Commands,
    mut bullets: Query<(Entity, &mut Transform, &mut Bullet), Without<Actor>>,
    mut actors: Query<(&Transform, &mut Actor, &mut Health, Has<crate::enemy::Enemy>), (Without<Bullet>, Without<Dead>)>,
    mut props: Query<(&Obstacle, Option<&mut Health>), Without<Actor>>,
) {
    let Some(level) = level else { return };
    let dt = time.delta_secs();
    'bullets: for (entity, mut transform, mut bullet) in &mut bullets {
        bullet.life -= dt;
        if bullet.life <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let start = transform.translation.truncate();
        let travel = bullet.velocity * dt;
        // Sub-step so fast bullets can't tunnel through thin things.
        let steps = (travel.length() / 3.0).ceil().max(1.0) as i32;
        for i in 1..=steps {
            let p = start + travel * (i as f32 / steps as f32);
            let mut hit = level.flags_at(p) & BLOCK_SHOT != 0;
            if !hit {
                for (obstacle, health) in &mut props {
                    if obstacle.contains(p, BULLET_MARGIN) {
                        if let Some(mut health) = health {
                            health.hp -= bullet.damage;
                        }
                        hit = true;
                        break;
                    }
                }
            }
            if !hit {
                for (actor_tf, mut actor, mut health, is_enemy) in &mut actors {
                    let target_team = if is_enemy { Team::Enemy } else { Team::Player };
                    if target_team == bullet.team || actor.state == AnimState::Roll {
                        continue;
                    }
                    let body = actor_tf.translation.truncate() + BODY_OFFSET;
                    if (p - body).abs().max_element() < 7.0 {
                        actor.take_hit(&mut health, bullet.damage);
                        hit = true;
                        break;
                    }
                }
            }
            if hit {
                spawn_fx(&mut commands, &assets.bullet_hit, bullet.hit, Transform::from_translation(p.extend(FX_Z)));
                commands.entity(entity).despawn();
                continue 'bullets;
            }
        }
        transform.translation += travel.extend(0.0);
    }
}

fn explode_barrels(
    mut commands: Commands,
    barrels: Query<(Entity, &Obstacle, &Health), With<Explosive>>,
    mut explosions: MessageWriter<Explosion>,
) {
    for (entity, obstacle, health) in &barrels {
        if health.hp <= 0 {
            commands.entity(entity).insert(Visibility::Hidden).remove::<Obstacle>();
            explosions.write(Explosion { pos: obstacle.center });
        }
    }
}

const EXPLOSION_RADIUS: f32 = 40.0;

fn apply_explosions(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut shake: ResMut<Shake>,
    mut explosions: MessageReader<Explosion>,
    mut actors: Query<(&Transform, &mut Actor, &mut Health), Without<Dead>>,
    mut barrels: Query<(&Obstacle, &mut Health), (With<Explosive>, Without<Actor>)>,
) {
    for explosion in explosions.read() {
        // The blast in 6_Explosion sits a bit below the middle of its 144px frame.
        spawn_fx(
            &mut commands,
            &assets.effects1,
            "6_Explosion",
            Transform::from_translation((explosion.pos + Vec2::new(0.0, 8.0)).extend(FX_Z)),
        );
        shake.add(0.6);
        for (transform, mut actor, mut health) in &mut actors {
            let body = transform.translation.truncate() + BODY_OFFSET;
            if body.distance(explosion.pos) < EXPLOSION_RADIUS && actor.state != AnimState::Roll {
                actor.take_hit(&mut health, 3);
            }
        }
        // Chain reaction: neighbouring barrels go off next frame.
        for (obstacle, mut health) in &mut barrels {
            if obstacle.center.distance(explosion.pos) < EXPLOSION_RADIUS && health.hp > 0 {
                health.hp = 0;
            }
        }
    }
}

fn despawn_finished_fx(
    mut commands: Commands,
    mut events: MessageReader<AnimationEvents>,
    fx: Query<(), With<OneShotFx>>,
) {
    for event in events.read() {
        if let AnimationEvents::Finished(entity) = event
            && fx.contains(*entity)
        {
            commands.entity(*entity).try_despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{BarrelHome, reset_barrels};

    #[test]
    fn one_hit_barrel_disappears_and_can_explode_again_after_wave_reset() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Explosion>()
            .add_systems(Update, explode_barrels);
        let collider = Obstacle { center: Vec2::new(100.0, 80.0), half: Vec2::new(8.0, 5.0) };
        let barrel = app.world_mut().spawn((
            Explosive, BarrelHome(collider.clone()), collider, Health::new(1), Visibility::Inherited,
        )).id();
        app.world_mut().get_mut::<Health>(barrel).unwrap().hp -= PLAYER_GUN.damage;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(barrel).unwrap(), Visibility::Hidden);
        assert!(app.world().get::<Obstacle>(barrel).is_none());
        assert!(app.world().get::<BarrelHome>(barrel).is_some());
        assert_eq!(app.world().resource::<Messages<Explosion>>().len(), 1);

        let restore = app.world_mut().register_system(
            |mut commands: Commands, barrels: Query<(Entity, &BarrelHome)>| reset_barrels(&mut commands, &barrels),
        );
        app.world_mut().run_system(restore).unwrap();
        assert_eq!(*app.world().get::<Visibility>(barrel).unwrap(), Visibility::Inherited);
        assert_eq!(app.world().get::<Health>(barrel).unwrap().hp, 1);
        assert_eq!(app.world().get::<Obstacle>(barrel).unwrap().center, Vec2::new(100.0, 80.0));
        app.world_mut().get_mut::<Health>(barrel).unwrap().hp -= PLAYER_GUN.damage;
        app.update();
        assert!(app.world().get::<Obstacle>(barrel).is_none());
        assert_eq!(*app.world().get::<Visibility>(barrel).unwrap(), Visibility::Hidden);
    }
}
