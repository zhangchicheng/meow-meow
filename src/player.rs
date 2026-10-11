use bevy::prelude::*;
use bevy_aseprite_ultra::prelude::*;

use crate::{
    GameState,
    actor::{Actor, AnimState, CHARACTER_ANCHOR, Dead, Health, Velocity, ysort_z},
    assets::GameAssets,
    camera::CursorWorld,
    combat::{Gun, PLAYER_GUN, Team, gun, play_sfx},
    map::Pickup,
    powerups::Powerups,
};

pub(crate) const RUN_SPEED: f32 = 85.0;
const ROLL_SPEED: f32 = 170.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<PlayerCharacter>().add_systems(
        Update,
        (collect_pickups, control_player).chain().run_if(in_state(GameState::Playing)),
    ).add_systems(Update, detect_death);
}

#[derive(Component)]
#[require(crate::portal::PortalTraveler)]
pub struct Player {
    roll: Timer,
    roll_cooldown: Timer,
    roll_dir: Vec2,
}

impl Player {
    pub(crate) fn new() -> Self {
        let mut roll = Timer::from_seconds(0.45, TimerMode::Once);
        roll.finish();
        let mut roll_cooldown = Timer::from_seconds(0.8, TimerMode::Once);
        roll_cooldown.finish();
        Self { roll, roll_cooldown, roll_dir: Vec2::X }
    }
}

#[derive(Resource, Default)]
pub struct Score(pub u32);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum PlayerCharacter {
    #[default]
    One,
    Three,
    Four,
}

impl PlayerCharacter {
    pub fn name(self) -> &'static str {
        match self { Self::One => "Luna", Self::Three => "Iris", Self::Four => "Nora" }
    }

    pub fn index(self) -> usize {
        match self { Self::One => 0, Self::Three => 1, Self::Four => 2 }
    }

    pub fn prefix(self) -> &'static str {
        match self { Self::One => "1", Self::Three => "3", Self::Four => "4" }
    }
}

pub fn spawn_player(commands: &mut Commands, assets: &GameAssets, pos: Vec2, character: PlayerCharacter) {
    commands
        .spawn((
            Name::new("Player"),
            Player::new(),
            Actor::new(character.prefix()),
            Health::new(10),
            AseAnimation {
                aseprite: assets.characters[character.index()].clone(),
                animation: Animation::tag(&format!("{}Idle", character.prefix())),
            },
            Sprite::default(),
            CHARACTER_ANCHOR,
            Transform::from_translation(pos.extend(ysort_z(pos.y))),
            DespawnOnEnter(GameState::Restart),
        ))
        .with_child(gun(assets, &PLAYER_GUN, Team::Player, 0.0));
}

pub fn control_player(
    time: Res<Time>,
    powerups: Res<Powerups>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor: Res<CursorWorld>,
    mut players: Query<(&mut Player, &mut Actor, &mut Velocity, &Transform, &Children), Without<Dead>>,
    mut guns: Query<&mut Gun>,
) {
    let Ok((mut player, mut actor, mut velocity, transform, children)) = players.single_mut() else {
        return;
    };
    player.roll.tick(time.delta());
    player.roll_cooldown.tick(time.delta());

    let mut dir = Vec2::ZERO;
    for (key, alt, d) in [
        (KeyCode::KeyW, KeyCode::ArrowUp, Vec2::Y),
        (KeyCode::KeyS, KeyCode::ArrowDown, Vec2::NEG_Y),
        (KeyCode::KeyA, KeyCode::ArrowLeft, Vec2::NEG_X),
        (KeyCode::KeyD, KeyCode::ArrowRight, Vec2::X),
    ] {
        if keys.pressed(key) || keys.pressed(alt) {
            dir += d;
        }
    }
    let dir = dir.normalize_or_zero();

    let body = transform.translation.truncate() + crate::actor::BODY_OFFSET;
    if let Some(target) = cursor.0
        && let Some(aim) = (target - body).try_normalize()
    {
        actor.aim = aim;
    }

    let wants_roll = keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Right);
    if wants_roll && player.roll_cooldown.is_finished() {
        player.roll.reset();
        player.roll_cooldown.reset();
        player.roll_dir = if dir != Vec2::ZERO { dir } else { actor.aim };
    }

    let rolling = !player.roll.is_finished();
    if rolling {
        velocity.0 = player.roll_dir * ROLL_SPEED * powerups.movement_factor();
        // Face the roll direction.
        if player.roll_dir.x != 0.0 {
            actor.aim.x = player.roll_dir.x.signum() * actor.aim.x.abs();
        }
        actor.state = AnimState::Roll;
    } else {
        velocity.0 = dir * RUN_SPEED * powerups.movement_factor();
        actor.state = if actor.is_hurt() {
            AnimState::Hurt
        } else if dir != Vec2::ZERO {
            AnimState::Run
        } else {
            AnimState::Idle
        };
    }

    let shooting = mouse.pressed(MouseButton::Left) || keys.pressed(KeyCode::KeyJ);
    let reload = keys.just_pressed(KeyCode::KeyR);
    for child in children {
        if let Ok(mut gun) = guns.get_mut(*child) {
            gun.trigger = shooting && !rolling;
            gun.reload_request |= reload;
        }
    }
}

fn collect_pickups(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut powerups: ResMut<Powerups>,
    mut players: Query<(&Transform, &mut Health), (With<Player>, Without<Dead>)>,
    pickups: Query<(Entity, &Pickup, &GlobalTransform)>,
) {
    let Ok((player, mut health)) = players.single_mut() else { return };
    if health.hp <= 0 { return; }
    let p = player.translation.truncate();
    for (entity, pickup, transform) in &pickups {
        let center = transform.transform_point(pickup.center_offset.extend(0.0)).truncate();
        if center.distance(p) < 12.0 {
            powerups.apply(pickup.kind, &mut health);
            play_sfx(&mut commands, &assets.collect, 0.7, 1.0);
            commands.entity(entity).despawn();
        }
    }
}

fn detect_death(
    players: Query<(), (With<Player>, Added<Dead>)>,
    mut next: ResMut<NextState<GameState>>,
) {
    if !players.is_empty() {
        next.set(GameState::GameOver);
    }
}
