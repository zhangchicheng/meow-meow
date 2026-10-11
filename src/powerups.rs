use bevy::{prelude::*, time::TimeSystems};

use crate::{GameState, actor::Health, debug::{InspectionControls, MuzzleInspection}, map::PickupKind};

pub const EFFECT_DURATION: f32 = 10.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<Powerups>()
        .add_systems(First, update_clock.after(InspectionControls).before(TimeSystems))
        .add_systems(OnEnter(GameState::Restart), clear_powerups)
        .add_systems(OnEnter(GameState::GameOver), clear_powerups);
}

#[derive(Resource, Default)]
pub struct Powerups {
    pub time_left: f32,
    pub speed_left: f32,
}

impl Powerups {
    pub fn apply(&mut self, kind: PickupKind, health: &mut Health) {
        match kind {
            PickupKind::Health => health.hp = health.max,
            PickupKind::Time => self.time_left = EFFECT_DURATION,
            PickupKind::Speed => self.speed_left = EFFECT_DURATION,
        }
    }

    pub fn time_factor(&self) -> f32 {
        if self.time_left > 0.0 { 0.5 } else { 1.0 }
    }

    pub fn movement_factor(&self) -> f32 {
        if self.speed_left > 0.0 { 2.0 } else { 1.0 }
    }

    fn tick(&mut self, seconds: f32) {
        self.time_left = (self.time_left - seconds).max(0.0);
        self.speed_left = (self.speed_left - seconds).max(0.0);
    }
}

fn update_clock(
    real_time: Res<Time<Real>>,
    state: Res<State<GameState>>,
    mut powerups: ResMut<Powerups>,
    inspection: Option<Res<MuzzleInspection>>,
    mut time: ResMut<Time<Virtual>>,
) {
    // Use real seconds so slow time doesn't extend its own duration; pause freezes buffs.
    if *state.get() == GameState::Playing && !time.is_paused() {
        powerups.tick(real_time.delta_secs());
    }
    let debug_speed = inspection.as_ref().map_or(1.0, |debug| debug.speed());
    time.set_relative_speed(debug_speed * powerups.time_factor());
}

fn clear_powerups(mut powerups: ResMut<Powerups>) {
    *powerups = Powerups::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_refills_and_effects_refresh_without_multiplying() {
        let mut effects = Powerups::default();
        let mut health = Health::new(10);
        health.hp = 2;
        effects.apply(PickupKind::Health, &mut health);
        assert_eq!(health.hp, 10);
        effects.apply(PickupKind::Time, &mut health);
        effects.apply(PickupKind::Speed, &mut health);
        effects.tick(3.0);
        assert_eq!(effects.time_left, 7.0);
        assert_eq!(effects.speed_left, 7.0);
        effects.apply(PickupKind::Time, &mut health);
        assert_eq!(effects.time_left, EFFECT_DURATION);
        assert_eq!(effects.speed_left, 7.0);
        assert_eq!(effects.time_factor() * effects.movement_factor(), 1.0);
        effects.tick(8.0);
        assert_eq!(effects.movement_factor(), 1.0);
        assert_eq!(effects.time_factor(), 0.5);
        effects.tick(3.0);
        assert_eq!(effects.time_factor(), 1.0);
    }

    #[test]
    fn clock_composes_debug_speed_and_preserves_buffs_during_pause() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(State::new(GameState::Playing))
            .insert_resource(Powerups { time_left: 10.0, speed_left: 10.0 })
            .add_systems(First, update_clock.before(TimeSystems));
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Powerups>().time_left, 10.0);
        assert_eq!(app.world().resource::<Time<Virtual>>().relative_speed(), 0.5);
        app.world_mut().resource_mut::<Time<Virtual>>().unpause();
        *app.world_mut().resource_mut::<Powerups>() = Powerups::default();
        app.update();
        assert_eq!(app.world().resource::<Time<Virtual>>().relative_speed(), 1.0);
    }
}
