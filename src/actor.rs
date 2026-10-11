//! Shared character logic: health, animation state, movement with collisions.

use bevy::{prelude::*, sprite::Anchor};
use bevy_aseprite_ultra::prelude::*;

use crate::map::{Level, Obstacle};

/// The character sprites are 80x80 frames with the feet ~30px above the bottom edge.
/// Anchoring there puts an actor's `Transform` at its feet, which is what we y-sort
/// and collide with.
pub const CHARACTER_ANCHOR: Anchor = Anchor(Vec2::new(0.0, -0.125));
/// Offset from the feet to the middle of the body (where bullets hit / are fired from).
pub const BODY_OFFSET: Vec2 = Vec2::new(0.0, 8.0);
/// Half-size of the collision box around the feet.
pub const FEET: Vec2 = Vec2::new(5.0, 3.0);

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (tick_hurt, start_dying, move_actors, play_animations)
            .chain()
            .after(crate::player::control_player)
            .after(crate::enemy::think)
            .after(crate::combat::CombatSystems),
    );
}

/// Depth for y-sorted sprites: lower on screen = closer to the camera.
pub fn ysort_z(y: f32) -> f32 {
    10.0 - y * 0.001
}

#[derive(Component)]
pub struct Health {
    pub hp: i32,
    pub max: i32,
}

impl Health {
    pub fn new(max: i32) -> Self {
        Self { hp: max, max }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimState {
    Idle,
    Run,
    Roll,
    Hurt,
    Death,
}

#[derive(Component)]
#[require(Velocity)]
pub struct Actor {
    /// Prefix of the aseprite tags, e.g. "1" for "1Idle", "1Run", ...
    pub prefix: &'static str,
    pub state: AnimState,
    playing: Option<AnimState>,
    /// Normalised aim direction; also decides which way the sprite faces.
    pub aim: Vec2,
    pub hurt: Timer,
}

impl Actor {
    pub fn new(prefix: &'static str) -> Self {
        let mut hurt = Timer::from_seconds(0.3, TimerMode::Once);
        hurt.finish();
        Self { prefix, state: AnimState::Idle, playing: None, aim: Vec2::X, hurt }
    }

    pub fn is_hurt(&self) -> bool {
        !self.hurt.is_finished()
    }

    pub fn take_hit(&mut self, health: &mut Health, damage: i32) {
        health.hp -= damage;
        self.hurt.reset();
    }
}

#[derive(Component, Default)]
pub struct Velocity(pub Vec2);

/// Death animation is playing; removed together with the entity (or kept for the player).
#[derive(Component)]
pub struct Dead;

fn tick_hurt(time: Res<Time>, mut actors: Query<&mut Actor>) {
    for mut actor in &mut actors {
        actor.hurt.tick(time.delta());
    }
}

pub(crate) fn start_dying(
    mut commands: Commands,
    mut actors: Query<(Entity, &mut Actor, &mut Velocity, &Health), Without<Dead>>,
) {
    for (entity, mut actor, mut velocity, health) in &mut actors {
        if health.hp <= 0 {
            actor.state = AnimState::Death;
            velocity.0 = Vec2::ZERO;
            commands.entity(entity).insert(Dead);
        }
    }
}

/// Moves `pos` by `delta`, sliding along walls and props one axis at a time.
pub fn move_and_slide(level: &Level, obstacles: &[&Obstacle], pos: Vec2, delta: Vec2) -> Vec2 {
    let blocked = |p: Vec2| level.box_blocked(p, FEET) || obstacles.iter().any(|o| o.contains(p, FEET));
    if blocked(pos) {
        // Already overlapping something (e.g. spawned on a bad spot): let it walk out.
        return pos + delta;
    }
    let mut p = pos;
    for axis in [Vec2::X, Vec2::Y] {
        let step = delta * axis;
        if step != Vec2::ZERO && !blocked(p + step) {
            p += step;
        }
    }
    p
}

pub(crate) fn move_actors(
    time: Res<Time>,
    level: Option<Res<Level>>,
    obstacles: Query<&Obstacle>,
    mut actors: Query<(&mut Transform, &Velocity), With<Actor>>,
) {
    let Some(level) = level else { return };
    let obstacles: Vec<&Obstacle> = obstacles.iter().collect();
    for (mut transform, velocity) in &mut actors {
        let pos = transform.translation.truncate();
        let new = move_and_slide(&level, &obstacles, pos, velocity.0 * time.delta_secs());
        transform.translation = new.extend(ysort_z(new.y));
    }
}

fn play_animations(
    aseprites: Res<Assets<Aseprite>>,
    mut actors: Query<(&mut Actor, &mut AseAnimation, &mut AnimationState, &mut Sprite)>,
) {
    for (mut actor, mut anim, mut animation_state, mut sprite) in &mut actors {
        sprite.flip_x = actor.aim.x < 0.0;
        if actor.playing == Some(actor.state) {
            continue;
        }
        let tag = format!("{}{:?}", actor.prefix, actor.state);
        // A new action starts with its full first frame, without the old elapsed time.
        if let Some(aseprite) = aseprites.get(&anim.aseprite)
            && let Some(meta) = aseprite.tags.get(&tag)
        {
            animation_state.current_frame = *meta.range.start();
            animation_state.relative_frame = 0;
            animation_state.elapsed = std::time::Duration::ZERO;
        }
        match actor.state {
            AnimState::Idle | AnimState::Run => anim.animation.play_loop(tag),
            // `Count(n)` = n extra repeats after the first play, so 0 plays it once.
            _ => anim.animation.play(tag, AnimationRepeat::Count(0)),
        }
        actor.playing = Some(actor.state);
    }
}
